use futures_util::StreamExt;
use prism_gemini::{
    json, Client, Config, Error, GenerateContentRequest, GenerateContentResponse,
    InteractionRequest, UploadSession, Value,
};
use wiremock::{
    matchers::{body_json, header, headers, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

fn client(server: &MockServer) -> Client {
    Client::with_config(Config::new(server.uri(), "test-secret").max_retries(0)).unwrap()
}

#[tokio::test]
async fn oauth_rest_uses_bearer_and_quota_project_without_api_key_header() {
    let server = MockServer::start().await;
    let sdk = Client::with_oauth_config(
        Config::new(server.uri(), "caller-managed-token")
            .with_api_key_header("x-goog-api-key")
            .unwrap()
            .header("x-goog-user-project", "quota-project")
            .unwrap(),
    )
    .unwrap();
    Mock::given(method("GET"))
        .and(path("/v1beta/models"))
        .and(header("authorization", "Bearer caller-managed-token"))
        .and(header("x-goog-user-project", "quota-project"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"models":[]})))
        .expect(1)
        .mount(&server)
        .await;
    assert!(sdk.models().list(&[]).await.unwrap().models.is_empty());
    assert!(!server.received_requests().await.unwrap()[0]
        .headers
        .contains_key("x-goog-api-key"));
    assert!(!format!("{sdk:?}").contains("caller-managed-token"));
}

async fn endpoint(server: &MockServer, verb: &str, route: &str, response: Value) {
    Mock::given(method(verb))
        .and(path(route))
        .and(header("x-goog-api-key", "test-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn generate_uses_google_auth_and_preserves_multimodal_tool_fields() {
    let server = MockServer::start().await;
    let body = json!({"contents":[{"role":"user","parts":[{"text":"Describe"},{"inlineData":{"mimeType":"image/png","data":"AAAA"}}]}],"tools":[{"functionDeclarations":[{"name":"search","parameters":{"type":"OBJECT"}}]}],"generationConfig":{"responseMimeType":"application/json","thinkingConfig":{"thinkingBudget":128}},"futureRequest":true});
    Mock::given(method("POST")).and(path("/v1beta/models/future-model:generateContent"))
        .and(header("x-goog-api-key", "test-secret")).and(body_json(&body))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"candidates":[{"content":{"parts":[{"text":"answer"},{"functionCall":{"id":"call1","name":"search","args":{"q":"rust"}},"thoughtSignature":"sig"}]},"finishReason":"NEW_REASON"}],"usageMetadata":{"totalTokenCount":17,"newUsage":3},"newResponse":true})))
        .expect(1).mount(&server).await;
    let response = client(&server)
        .models()
        .generate_content("models/future-model", &body)
        .await
        .unwrap();
    assert_eq!(response.text(), "answer");
    assert_eq!(
        response.function_calls().next().unwrap().id.as_deref(),
        Some("call1")
    );
    assert_eq!(
        response.candidates[0].finish_reason.as_deref(),
        Some("NEW_REASON")
    );
    assert_eq!(response.usage_metadata.unwrap().extra["newUsage"], 3);
    assert_eq!(response.extra["newResponse"], true);
    let requests = server.received_requests().await.unwrap();
    assert!(!requests[0].headers.contains_key("authorization"));
    assert!(!requests[0].url.query_pairs().any(|(k, _)| k == "key"));
}

#[tokio::test]
async fn streaming_requests_sse_and_decodes_typed_chunks() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1beta/models/a:streamGenerateContent"))
        .and(query_param("alt", "sse")).and(header("accept", "text/event-stream"))
        .and(body_json(json!({"contents":[{"role":"user","parts":[{"text":"hi"}]}]})))
        .respond_with(ResponseTemplate::new(200)
            .set_body_raw("data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"one\"}]}}]}\n\ndata: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"two\"}]}}]}\n\n", "text/event-stream"))
        .expect(1).mount(&server).await;
    let mut stream = client(&server)
        .models()
        .stream_generate_content("a", &GenerateContentRequest::text("hi"))
        .await
        .unwrap();
    let first: GenerateContentResponse = stream.next().await.unwrap().unwrap().json().unwrap();
    let second: GenerateContentResponse = stream.next().await.unwrap().unwrap().json().unwrap();
    assert_eq!(first.text(), "one");
    assert_eq!(second.text(), "two");
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn model_metadata_embeddings_batches_and_predictions_have_correct_routes() {
    let server = MockServer::start().await;
    let sdk = client(&server);
    Mock::given(method("GET")).and(path("/v1beta/models")).and(query_param("pageSize", "5")).and(query_param("pageToken", "cursor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"models":[{"name":"models/a","supportedGenerationMethods":["generateContent"],"future":true}],"nextPageToken":"next"})))
        .expect(1).mount(&server).await;
    let list = sdk
        .models()
        .list(&[
            ("pageSize".into(), "5".into()),
            ("pageToken".into(), "cursor".into()),
        ])
        .await
        .unwrap();
    assert_eq!(list.next_page_token.as_deref(), Some("next"));
    assert_eq!(list.models[0].extra["future"], true);
    endpoint(
        &server,
        "GET",
        "/v1beta/models/a",
        json!({"name":"models/a"}),
    )
    .await;
    assert_eq!(sdk.models().get("a").await.unwrap().name, "models/a");
    endpoint(
        &server,
        "POST",
        "/v1beta/models/a:countTokens",
        json!({"totalTokens":3}),
    )
    .await;
    assert_eq!(
        sdk.models()
            .count_tokens("a", &json!({"contents":[]}))
            .await
            .unwrap()
            .total_tokens,
        3
    );
    endpoint(
        &server,
        "POST",
        "/v1beta/models/a:embedContent",
        json!({"embedding":{"values":[0.5,1.0]}}),
    )
    .await;
    assert_eq!(
        sdk.models()
            .embed_content("a", &json!({"content":{"parts":[{"text":"hi"}]}}))
            .await
            .unwrap()
            .embedding
            .values,
        [0.5, 1.0]
    );
    endpoint(
        &server,
        "POST",
        "/v1beta/models/a:batchEmbedContents",
        json!({"embeddings":[{"values":[1.0]}]}),
    )
    .await;
    assert_eq!(
        sdk.models()
            .batch_embed_contents("a", &json!({"requests":[]}))
            .await
            .unwrap()
            .embeddings
            .len(),
        1
    );
    endpoint(
        &server,
        "POST",
        "/v1beta/models/a:batchGenerateContent",
        json!({"name":"batches/a"}),
    )
    .await;
    assert_eq!(
        sdk.models()
            .batch_generate_content("a", &json!({"batch":{}}))
            .await
            .unwrap()
            .name,
        "batches/a"
    );
    endpoint(
        &server,
        "POST",
        "/v1beta/models/a:asyncBatchEmbedContent",
        json!({"name":"batches/b"}),
    )
    .await;
    assert_eq!(
        sdk.models()
            .async_batch_embed_content("a", &json!({"batch":{}}))
            .await
            .unwrap()
            .name,
        "batches/b"
    );
    endpoint(
        &server,
        "POST",
        "/v1beta/models/a:predict",
        json!({"predictions":[{"bytesBase64Encoded":"AAAA"}]}),
    )
    .await;
    assert_eq!(
        sdk.models()
            .predict("a", &json!({"instances":[]}))
            .await
            .unwrap()["predictions"][0]["bytesBase64Encoded"],
        "AAAA"
    );
    endpoint(
        &server,
        "POST",
        "/v1beta/models/a:predictLongRunning",
        json!({"name":"models/a/operations/b","done":false}),
    )
    .await;
    assert!(
        !sdk.models()
            .predict_long_running("a", &json!({"instances":[]}))
            .await
            .unwrap()
            .done
    );
    endpoint(
        &server,
        "GET",
        "/v1beta/models/a/operations/b",
        json!({"name":"models/a/operations/b","done":true,"response":{"videos":[]}}),
    )
    .await;
    assert!(
        sdk.operations()
            .get("models/a/operations/b")
            .await
            .unwrap()
            .done
    );
}

#[tokio::test]
async fn cache_batch_and_file_search_routes_preserve_updates_and_force() {
    let server = MockServer::start().await;
    let sdk = client(&server);
    endpoint(
        &server,
        "POST",
        "/v1beta/cachedContents",
        json!({"name":"cachedContents/a"}),
    )
    .await;
    sdk.cached_contents()
        .create(&json!({"model":"models/a","ttl":"60s"}))
        .await
        .unwrap();
    endpoint(
        &server,
        "GET",
        "/v1beta/cachedContents",
        json!({"cachedContents":[]}),
    )
    .await;
    sdk.cached_contents().list(&[]).await.unwrap();
    endpoint(
        &server,
        "GET",
        "/v1beta/cachedContents/a",
        json!({"name":"cachedContents/a"}),
    )
    .await;
    sdk.cached_contents().get("a").await.unwrap();
    Mock::given(method("PATCH"))
        .and(path("/v1beta/cachedContents/a"))
        .and(query_param("updateMask", "ttl"))
        .and(body_json(json!({"ttl":"120s"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ttl":"120s"})))
        .expect(1)
        .mount(&server)
        .await;
    sdk.cached_contents()
        .update(
            "a",
            &json!({"ttl":"120s"}),
            &[("updateMask".into(), "ttl".into())],
        )
        .await
        .unwrap();
    endpoint(&server, "DELETE", "/v1beta/cachedContents/a", json!({})).await;
    sdk.cached_contents().delete("a").await.unwrap();
    endpoint(&server, "GET", "/v1beta/batches", json!({"operations":[]})).await;
    sdk.batches().list(&[]).await.unwrap();
    endpoint(
        &server,
        "GET",
        "/v1beta/batches/a",
        json!({"name":"batches/a"}),
    )
    .await;
    sdk.batches().get("a").await.unwrap();
    endpoint(&server, "POST", "/v1beta/batches/a:cancel", json!({})).await;
    sdk.batches().cancel("a").await.unwrap();
    endpoint(
        &server,
        "PATCH",
        "/v1beta/batches/a:updateGenerateContentBatch",
        json!({"name":"batches/a","state":"BATCH_STATE_SUCCEEDED","priority":"3"}),
    )
    .await;
    let generated = sdk
        .batches()
        .update_generate_content("a", &json!({}), &[])
        .await
        .unwrap();
    assert_eq!(generated.state.as_deref(), Some("BATCH_STATE_SUCCEEDED"));
    assert_eq!(generated.priority.as_deref(), Some("3"));
    endpoint(
        &server,
        "PATCH",
        "/v1beta/batches/a:updateEmbedContentBatch",
        json!({"name":"batches/a","state":"BATCH_STATE_PENDING","inputConfig":{"fileName":"files/input"}}),
    )
    .await;
    let embeddings = sdk
        .batches()
        .update_embed_content("a", &json!({}), &[])
        .await
        .unwrap();
    assert_eq!(embeddings.state.as_deref(), Some("BATCH_STATE_PENDING"));
    assert_eq!(embeddings.input_config.unwrap()["fileName"], "files/input");
    endpoint(&server, "DELETE", "/v1beta/batches/a", json!({})).await;
    sdk.batches().delete("a").await.unwrap();
    endpoint(
        &server,
        "POST",
        "/v1beta/fileSearchStores",
        json!({"name":"fileSearchStores/a"}),
    )
    .await;
    sdk.file_search_stores()
        .create(&json!({"displayName":"Docs"}))
        .await
        .unwrap();
    endpoint(
        &server,
        "GET",
        "/v1beta/fileSearchStores",
        json!({"fileSearchStores":[]}),
    )
    .await;
    sdk.file_search_stores().list(&[]).await.unwrap();
    endpoint(
        &server,
        "GET",
        "/v1beta/fileSearchStores/a",
        json!({"name":"fileSearchStores/a"}),
    )
    .await;
    sdk.file_search_stores().get("a").await.unwrap();
    endpoint(
        &server,
        "POST",
        "/v1beta/fileSearchStores/a:importFile",
        json!({"name":"fileSearchStores/a/operations/o"}),
    )
    .await;
    sdk.file_search_stores()
        .import_file("a", &json!({"fileName":"files/a"}))
        .await
        .unwrap();
    endpoint(
        &server,
        "GET",
        "/v1beta/fileSearchStores/a/documents",
        json!({"documents":[]}),
    )
    .await;
    sdk.file_search_stores()
        .list_documents("a", &[])
        .await
        .unwrap();
    endpoint(
        &server,
        "GET",
        "/v1beta/fileSearchStores/a/documents/d",
        json!({"name":"fileSearchStores/a/documents/d"}),
    )
    .await;
    sdk.file_search_stores()
        .get_document("fileSearchStores/a/documents/d")
        .await
        .unwrap();
    Mock::given(method("DELETE"))
        .and(path("/v1beta/fileSearchStores/a/documents/d"))
        .and(query_param("force", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    sdk.file_search_stores()
        .delete_document("fileSearchStores/a/documents/d", true)
        .await
        .unwrap();
    Mock::given(method("DELETE"))
        .and(path("/v1beta/fileSearchStores/a"))
        .and(query_param("force", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    sdk.file_search_stores().delete("a", true).await.unwrap();
}

#[tokio::test]
async fn resumable_upload_transfers_real_bytes_and_retains_query_tokens() {
    let server = MockServer::start().await;
    let sdk = client(&server);
    Mock::given(method("POST"))
        .and(path("/upload/v1beta/files"))
        .and(header("x-goog-api-key", "test-secret"))
        .and(header("x-goog-upload-protocol", "resumable"))
        .and(header("x-goog-upload-command", "start"))
        .and(header("x-goog-upload-header-content-length", "3"))
        .and(header("x-goog-upload-header-content-type", "text/plain"))
        .and(body_json(json!({"file":{"displayName":"notes"}})))
        .respond_with(ResponseTemplate::new(200).insert_header(
            "x-goog-upload-url",
            format!("{}/upload/session?upload_id=opaque", server.uri()),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST")).and(path("/upload/session")).and(query_param("upload_id", "opaque"))
        .and(headers("x-goog-upload-command", vec!["upload", "finalize"])).and(header("x-goog-upload-offset", "0"))
        .and(|request: &wiremock::Request| request.body == b"abc")
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"file":{"name":"files/a","mimeType":"text/plain","uri":"gs://file","state":"ACTIVE","future":3}})))
        .expect(1).mount(&server).await;
    let uploaded = sdk
        .files()
        .upload(
            &json!({"file":{"displayName":"notes"}}),
            "text/plain",
            b"abc".to_vec(),
        )
        .await
        .unwrap();
    assert_eq!(uploaded.file.name, "files/a");
    assert_eq!(uploaded.file.extra["future"], 3);
}

#[tokio::test]
async fn upload_query_cancel_download_and_registration_work() {
    let server = MockServer::start().await;
    let sdk = client(&server);
    let session = UploadSession::from_url(format!("{}/upload/session", server.uri()));
    Mock::given(method("POST"))
        .and(path("/upload/session"))
        .and(header("x-goog-upload-command", "query"))
        .respond_with(ResponseTemplate::new(200).insert_header("x-goog-upload-size-received", "10"))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        sdk.files().query_upload(&session).await.unwrap().headers["x-goog-upload-size-received"],
        "10"
    );
    Mock::given(method("POST"))
        .and(path("/upload/session"))
        .and(header("x-goog-upload-command", "cancel"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    sdk.files().cancel_upload(&session).await.unwrap();
    Mock::given(method("GET"))
        .and(path("/download/video"))
        .and(query_param("alt", "media"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes([0_u8, 1, 255]))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        sdk.files()
            .download(&format!("{}/download/video", server.uri()))
            .await
            .unwrap(),
        [0, 1, 255]
    );
    endpoint(
        &server,
        "POST",
        "/v1beta/files:register",
        json!({"files":[{"name":"files/a"}]}),
    )
    .await;
    assert_eq!(
        sdk.files()
            .register(&json!({"uris":["gs://bucket/a"]}))
            .await
            .unwrap()
            .files[0]
            .name,
        "files/a"
    );
    endpoint(
        &server,
        "GET",
        "/v1beta/files",
        json!({"files":[],"nextPageToken":"next"}),
    )
    .await;
    assert_eq!(
        sdk.files()
            .list(&[])
            .await
            .unwrap()
            .next_page_token
            .as_deref(),
        Some("next")
    );
    endpoint(&server, "GET", "/v1beta/files/a", json!({"name":"files/a"})).await;
    sdk.files().get("files/a").await.unwrap();
    endpoint(&server, "DELETE", "/v1beta/files/a", json!({})).await;
    sdk.files().delete("a").await.unwrap();
    assert!(!format!("{session:?}").contains("/upload/session"));
}

#[tokio::test]
async fn unsafe_names_and_foreign_upload_or_download_urls_send_no_requests() {
    let server = MockServer::start().await;
    let sdk = client(&server);
    for id in ["../escape", "//evil.invalid", "a?key=x", "a%2fb", "a#x"] {
        assert!(matches!(sdk.models().get(id).await, Err(Error::Config(_))));
        assert!(matches!(sdk.files().get(id).await, Err(Error::Config(_))));
    }
    let foreign = UploadSession::from_url("https://evil.invalid/steal");
    assert!(matches!(
        sdk.files()
            .upload_chunk(&foreign, 0, b"secret".to_vec(), true)
            .await,
        Err(Error::Config(_))
    ));
    assert!(matches!(
        sdk.files().download("https://evil.invalid/steal").await,
        Err(Error::Config(_))
    ));
    assert!(sdk
        .file_search_stores()
        .get_document("fileSearchStores/a/../secret")
        .await
        .is_err());
    assert!(sdk
        .operations()
        .get("models/a/operations/../secret")
        .await
        .is_err());
    assert!(sdk
        .environments()
        .files("a", "../secret", &[])
        .await
        .is_err());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn file_search_upload_and_operation_routes_work() {
    let server = MockServer::start().await;
    let sdk = client(&server);
    Mock::given(method("POST"))
        .and(path(
            "/upload/v1beta/fileSearchStores/a:uploadToFileSearchStore",
        ))
        .and(body_json(
            json!({"displayName":"notes","customMetadata":[{"key":"kind","stringValue":"guide"}]}),
        ))
        .respond_with(ResponseTemplate::new(200).insert_header(
            "x-goog-upload-url",
            format!("{}/upload/search", server.uri()),
        ))
        .expect(1)
        .mount(&server)
        .await;
    endpoint(
        &server,
        "POST",
        "/upload/search",
        json!({"name":"fileSearchStores/a/upload/operations/o"}),
    )
    .await;
    let operation = sdk
        .file_search_stores()
        .upload(
            "a",
            &json!({"displayName":"notes","customMetadata":[{"key":"kind","stringValue":"guide"}]}),
            "text/plain",
            b"abc".to_vec(),
        )
        .await
        .unwrap();
    endpoint(
        &server,
        "GET",
        "/v1beta/fileSearchStores/a/upload/operations/o",
        json!({"name":operation.name,"done":true}),
    )
    .await;
    assert!(sdk.operations().get(&operation.name).await.unwrap().done);
}

#[tokio::test]
async fn missing_upload_url_and_api_error_are_reported() {
    let server = MockServer::start().await;
    let sdk = client(&server);
    Mock::given(method("POST"))
        .and(path("/upload/v1beta/files"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        sdk.files().start_upload(&json!({}), "text/plain", 1).await,
        Err(Error::Protocol(_))
    ));
    Mock::given(method("GET"))
        .and(path("/v1beta/models/nope"))
        .respond_with(
            ResponseTemplate::new(404)
                .insert_header("x-goog-request-id", "request123")
                .set_body_json(
                    json!({"error":{"code":404,"status":"NOT_FOUND","message":"No model"}}),
                ),
        )
        .expect(1)
        .mount(&server)
        .await;
    match sdk.models().get("nope").await.unwrap_err() {
        Error::Api {
            status,
            message,
            request_id,
            body,
        } => {
            assert_eq!(status, 404);
            assert_eq!(message, "No model");
            assert_eq!(request_id.as_deref(), Some("request123"));
            assert_eq!(body["error"]["status"], "NOT_FOUND");
        }
        error => panic!("unexpected error: {error}"),
    }
}

#[tokio::test]
async fn interactions_use_snake_case_and_support_stream_resumption() {
    let server = MockServer::start().await;
    let sdk = client(&server);
    let body = InteractionRequest {
        model: Some("future-model".into()),
        input: json!("hi"),
        previous_interaction_id: Some("previous".into()),
        ..InteractionRequest::default()
    };
    Mock::given(method("POST")).and(path("/v1beta/interactions")).and(body_json(json!({"model":"future-model","input":"hi","previous_interaction_id":"previous"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"i","status":"completed","steps":[{"type":"model_output","content":[{"type":"text","text":"hello"}]}],"future":true})))
        .expect(1).mount(&server).await;
    let interaction = sdk.interactions().create(&body).await.unwrap();
    assert_eq!(interaction.text(), "hello");
    assert_eq!(interaction.extra["future"], true);
    Mock::given(method("GET"))
        .and(path("/v1beta/interactions/i"))
        .and(query_param("stream", "true"))
        .and(query_param("last_event_id", "e1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "event: content.delta\nid: e2\ndata: {\"text\":\"hi\"}\n\n",
            "text/event-stream",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let mut stream = sdk
        .interactions()
        .resume_stream("i", &[("last_event_id".into(), "e1".into())])
        .await
        .unwrap();
    let event = stream.next().await.unwrap().unwrap();
    assert_eq!(event.id.as_deref(), Some("e2"));
    assert_eq!(event.event.as_deref(), Some("content.delta"));
    assert_eq!(event.json::<Value>().unwrap()["text"], "hi");
    endpoint(
        &server,
        "POST",
        "/v1beta/interactions/i/cancel",
        json!({"id":"i","status":"cancelled"}),
    )
    .await;
    sdk.interactions().cancel("i").await.unwrap();
    endpoint(&server, "DELETE", "/v1beta/interactions/i", Value::Null).await;
    sdk.interactions().delete("i").await.unwrap();
}

#[tokio::test]
async fn collection_routes_cover_managed_agents_and_platform_resources() {
    let server = MockServer::start().await;
    let sdk = client(&server);
    macro_rules! check_collection {
        ($service:ident, $collection:literal) => {
            endpoint(&server, "POST", concat!("/v1beta/", $collection), json!({"id":"a"})).await;
            sdk.$service().create(&json!({"display_name":"a"})).await.unwrap();
            endpoint(&server, "GET", concat!("/v1beta/", $collection), json!({"items":[]})).await;
            sdk.$service().list(&[]).await.unwrap();
            endpoint(&server, "GET", concat!("/v1beta/", $collection, "/a"), json!({"id":"a"})).await;
            sdk.$service().get("a").await.unwrap();
            endpoint(&server, "DELETE", concat!("/v1beta/", $collection, "/a"), Value::Null).await;
            sdk.$service().delete("a").await.unwrap();
        };
    }
    check_collection!(agents, "agents");
    check_collection!(webhooks, "webhooks");
    check_collection!(triggers, "triggers");
    check_collection!(environments, "environments");
    check_collection!(voices, "voices");
    check_collection!(credentials, "credentials");
    endpoint(&server, "PATCH", "/v1beta/webhooks/a", json!({})).await;
    sdk.webhooks().update("a", &json!({}), &[]).await.unwrap();
    endpoint(&server, "PATCH", "/v1beta/triggers/a", json!({})).await;
    sdk.triggers().update("a", &json!({}), &[]).await.unwrap();
    endpoint(&server, "PATCH", "/v1beta/credentials/a", json!({})).await;
    sdk.credentials()
        .update("a", &json!({}), &[])
        .await
        .unwrap();
    endpoint(&server, "POST", "/v1beta/webhooks/a:ping", json!({})).await;
    sdk.webhooks().ping("a").await.unwrap();
    endpoint(
        &server,
        "POST",
        "/v1beta/webhooks/a:rotateSigningSecret",
        json!({}),
    )
    .await;
    sdk.webhooks().rotate_signing_secret("a").await.unwrap();
    endpoint(&server, "POST", "/v1beta/triggers/a/executions", json!({})).await;
    sdk.triggers().run("a", &json!({})).await.unwrap();
    endpoint(&server, "GET", "/v1beta/triggers/a/executions", json!({})).await;
    sdk.triggers().executions("a", &[]).await.unwrap();
    endpoint(
        &server,
        "GET",
        "/v1beta/environments/a/files/dir/file.txt",
        json!({"name":"file.txt"}),
    )
    .await;
    sdk.environments()
        .files("a", "dir/file.txt", &[])
        .await
        .unwrap();
    endpoint(
        &server,
        "POST",
        "/v1beta/auth_tokens",
        json!({"name":"token"}),
    )
    .await;
    sdk.create_auth_token(&json!({"uses":1})).await.unwrap();
}
