use futures_util::StreamExt;
use serde_json::{json, Value};
use sydney_openai::{
    reqwest::multipart::{Form, Part},
    BatchRequest, ChatCompletionChunk, ChatCompletionRequest, ChatMessage, Client, Config,
    EmbeddingRequest, EmbeddingVector, Error, FineTuningRequest, ImageRequest, List, Method,
    ModerationRequest, Response, ResponseRequest, SpeechRequest,
};
use wiremock::{
    matchers::{
        body_json, body_partial_json, body_string_contains, header, method, path, query_param,
    },
    Mock, MockServer, ResponseTemplate,
};

fn client(server: &MockServer) -> Client {
    Client::with_config(Config::new(
        format!("{}/v1/", server.uri()),
        "test-openai-key",
    ))
    .unwrap()
}

fn response() -> Value {
    json!({"id":"resp_1", "object":"response", "model":"caller-model", "status":"completed",
        "output":[{"type":"message","role":"assistant","content":[
            {"type":"output_text","text":"Hello"}, {"type":"output_text","text":" world"},
            {"type":"refusal","refusal":"excluded"}]}], "future_response_field":{"retained":true}})
}

fn chat() -> Value {
    json!({"id":"chat_1","object":"chat.completion","created":1,"model":"caller-model",
        "choices":[{"index":0,"message":{"role":"assistant","content":"Hello", "future":7},
            "finish_reason":"stop"}],"usage":{"total_tokens":3}})
}

#[tokio::test]
async fn responses_forward_extensible_multimodal_input_and_preserve_output() {
    let server = MockServer::start().await;
    let input = json!([{"role":"user","content":[{"type":"input_text","text":"Describe"},
        {"type":"input_image","image_url":"https://example.invalid/photo.png"}]}]);
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .and(header("authorization", "Bearer test-openai-key"))
        .and(body_partial_json(
            json!({"model":"caller-model", "input": input,
            "reasoning":{"effort":"low"}, "stream":false}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(response()))
        .expect(1)
        .mount(&server)
        .await;
    let request = ResponseRequest::new("caller-model", input)
        .with_extra("reasoning", json!({"effort":"low"}));
    let result = client(&server).responses().create(&request).await.unwrap();
    assert_eq!(result.output_text(), "Hello world");
    assert_eq!(result.extra["future_response_field"]["retained"], true);
}

#[tokio::test]
async fn nonstreaming_creation_overrides_conflicting_stream_extension() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .and(body_partial_json(json!({"stream":false})))
        .respond_with(ResponseTemplate::new(200).set_body_json(response()))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .responses()
        .create_json(&json!({"model":"caller-model","input":"hello","stream":true}))
        .await
        .unwrap();
}

#[tokio::test]
async fn responses_stream_preserves_named_events_and_new_event_fields() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/responses"))
        .and(body_partial_json(json!({"stream":true})))
        .respond_with(ResponseTemplate::new(200)
            .set_body_raw("event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"Hi\",\"new\":1}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\"}\n\n", "text/event-stream"))
        .expect(1).mount(&server).await;
    let mut stream = client(&server)
        .responses()
        .stream(&ResponseRequest::new("caller-model", "hi"))
        .await
        .unwrap();
    let first = stream.next().await.unwrap().unwrap();
    assert_eq!(first.event.as_deref(), Some("response.output_text.delta"));
    assert_eq!(first.json::<Value>().unwrap()["new"], 1);
    assert_eq!(
        stream.next().await.unwrap().unwrap().event.as_deref(),
        Some("response.completed")
    );
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn response_management_routes_queries_and_ids() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/responses/resp_1"))
        .and(query_param("include[]", "output[*].logprobs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/responses/resp_1/cancel"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/responses/resp_1"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let api = client(&server);
    api.responses()
        .retrieve_with_query(
            "resp_1",
            &[("include[]".into(), "output[*].logprobs".into())],
        )
        .await
        .unwrap();
    api.responses().cancel("resp_1").await.unwrap();
    assert_eq!(api.responses().delete("resp_1").await.unwrap(), Value::Null);
}

#[tokio::test]
async fn background_response_stream_resumes_after_sequence() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/responses/resp_1"))
        .and(query_param("stream", "true"))
        .and(query_param("starting_after", "4"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "data: {\"type\":\"response.completed\"}\n\n",
            "text/event-stream",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let mut stream = client(&server)
        .responses()
        .retrieve_stream(
            "resp_1",
            &[
                ("stream".into(), "false".into()),
                ("starting_after".into(), "4".into()),
            ],
        )
        .await
        .unwrap();
    assert_eq!(
        stream
            .next()
            .await
            .unwrap()
            .unwrap()
            .json::<Value>()
            .unwrap()["type"],
        "response.completed"
    );
}

#[tokio::test]
async fn compact_count_and_input_items_are_distinct_operations() {
    let server = MockServer::start().await;
    for endpoint in ["compact", "input_tokens"] {
        Mock::given(method("POST"))
            .and(path(format!("/v1/responses/{endpoint}")))
            .and(body_json(json!({"model":"caller-model","input":"text"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok":true})))
            .expect(1)
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/v1/responses/resp_1/input_items"))
        .and(query_param("limit", "10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[],"has_more":false})))
        .expect(1)
        .mount(&server)
        .await;
    let api = client(&server);
    let body = json!({"model":"caller-model","input":"text"});
    api.responses().compact(&body).await.unwrap();
    api.responses().count_input_tokens(&body).await.unwrap();
    assert_eq!(
        api.responses()
            .input_items("resp_1", &[("limit".into(), "10".into())])
            .await
            .unwrap()
            .next_after()
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn chat_accepts_tool_results_and_arbitrary_model_names() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_partial_json(
            json!({"model":"caller-model","messages":[
            {"role":"developer","content":"Use tools"},
            {"role":"tool","content":"42","tool_call_id":"call_1"}],
            "response_format":{"type":"json_object"},"stream":false}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(chat()))
        .expect(1)
        .mount(&server)
        .await;
    let mut request = ChatCompletionRequest::new(
        "caller-model",
        vec![
            ChatMessage::developer("Use tools"),
            ChatMessage::tool("call_1", "42"),
        ],
    );
    request.response_format = Some(json!({"type":"json_object"}));
    let result = client(&server).chat().create(&request).await.unwrap();
    assert_eq!(result.choices[0].message.extra["future"], 7);
}

#[tokio::test]
async fn chat_stream_decodes_tool_call_deltas_and_usage_only_chunks() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/chat/completions"))
        .and(body_partial_json(json!({"stream":true,"stream_options":{"include_usage":true}})))
        .respond_with(ResponseTemplate::new(200)
            .set_body_raw(concat!(
                "data: {\"id\":\"c\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"caller-model\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\"}}]},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"c\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"caller-model\",\"choices\":[],\"usage\":{\"total_tokens\":9}}\n\n",
                "data: [DONE]\n\n"), "text/event-stream"))
        .expect(1).mount(&server).await;
    let request = ChatCompletionRequest::new("caller-model", vec![ChatMessage::user("hi")])
        .with_extra("stream_options", json!({"include_usage":true}));
    let mut stream = client(&server).chat().stream(&request).await.unwrap();
    let first = stream
        .next()
        .await
        .unwrap()
        .unwrap()
        .json::<ChatCompletionChunk>()
        .unwrap();
    assert_eq!(
        first.choices[0].delta["tool_calls"][0]["function"]["arguments"],
        "{"
    );
    let usage = stream
        .next()
        .await
        .unwrap()
        .unwrap()
        .json::<ChatCompletionChunk>()
        .unwrap();
    assert_eq!(usage.usage.unwrap()["total_tokens"], 9);
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn embeddings_support_float_and_base64_vectors() {
    let server = MockServer::start().await;
    for (encoding, embedding) in [
        ("float", json!([0.25, -0.5])),
        ("base64", json!("AACAPw==")),
    ] {
        Mock::given(method("POST")).and(path("/v1/embeddings"))
            .and(body_partial_json(json!({"model":"caller-model","input":[1,2,3],"encoding_format":encoding})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"object":"list","model":"caller-model",
                "data":[{"object":"embedding","index":0,"embedding":embedding}],"usage":{"total_tokens":3}})))
            .expect(1).mount(&server).await;
    }
    let api = client(&server);
    let mut request = EmbeddingRequest::new("caller-model", json!([1, 2, 3]));
    request.encoding_format = Some("float".into());
    let first = api.embeddings().create(&request).await.unwrap();
    assert!(
        matches!(&first.data[0].embedding, EmbeddingVector::Float(values) if values == &[0.25,-0.5])
    );
    request.encoding_format = Some("base64".into());
    let second = api.embeddings().create(&request).await.unwrap();
    assert!(
        matches!(&second.data[0].embedding, EmbeddingVector::Base64(value) if value == "AACAPw==")
    );
}

#[tokio::test]
async fn image_generation_and_edit_json_keep_new_parameters() {
    let server = MockServer::start().await;
    for endpoint in ["generations", "edits"] {
        Mock::given(method("POST"))
            .and(path(format!("/v1/images/{endpoint}")))
            .and(body_partial_json(
                json!({"model":"caller-model","prompt":"A tree","background":"transparent"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"created":1,"data":[{"b64_json":"YWJj"}],"usage":{"total_tokens":1}}),
            ))
            .expect(1)
            .mount(&server)
            .await;
    }
    let api = client(&server);
    let body = ImageRequest::new("caller-model", "A tree").with_extra("background", "transparent");
    assert_eq!(
        api.images().generate(&body).await.unwrap().data[0]
            .b64_json
            .as_deref(),
        Some("YWJj")
    );
    api.images().edit_json(&body).await.unwrap();
}

#[tokio::test]
async fn files_upload_binary_and_download_exact_bytes() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/files"))
        .and(header("authorization", "Bearer test-openai-key"))
        .and(body_string_contains("name=\"purpose\"\r\n\r\nbatch"))
        .and(body_string_contains("filename=\"input.jsonl\""))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id":"file_1","object":"file",
            "bytes":6,"created_at":1,"filename":"input.jsonl","purpose":"batch","future":true})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/files/file_1/content"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0, 1, 255, 10]))
        .expect(1)
        .mount(&server)
        .await;
    let api = client(&server);
    let uploaded = api
        .files()
        .upload("input.jsonl", b"{}\n{}\n".to_vec(), "batch")
        .await
        .unwrap();
    assert_eq!(uploaded.extra["future"], true);
    assert_eq!(
        api.files().content(&uploaded.id).await.unwrap(),
        vec![0, 1, 255, 10]
    );
}

#[tokio::test]
async fn audio_speech_returns_encoded_bytes() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/audio/speech"))
        .and(body_partial_json(json!({"model":"caller-model","voice":{"id":"voice_1"},"input":"Hello","response_format":"wav"})))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"RIFF\x00\x00".to_vec()))
        .expect(1).mount(&server).await;
    let mut request = SpeechRequest::new("caller-model", "Hello", json!({"id":"voice_1"}));
    request.response_format = Some("wav".into());
    assert_eq!(
        client(&server).audio().speech(&request).await.unwrap(),
        b"RIFF\x00\x00"
    );
}

#[tokio::test]
async fn audio_multipart_transcription_and_translation() {
    let server = MockServer::start().await;
    for endpoint in ["transcriptions", "translations"] {
        Mock::given(method("POST"))
            .and(path(format!("/v1/audio/{endpoint}")))
            .and(body_string_contains("filename=\"sample.wav\""))
            .and(body_string_contains("caller-model"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"text":"hello","segments":[{"start":0.0}]})),
            )
            .expect(1)
            .mount(&server)
            .await;
    }
    let api = client(&server);
    let result = api
        .audio()
        .transcribe("sample.wav", b"RIFF".to_vec(), "caller-model")
        .await
        .unwrap();
    assert!(result.extra.contains_key("segments"));
    assert_eq!(
        api.audio()
            .translate("sample.wav", b"RIFF".to_vec(), "caller-model")
            .await
            .unwrap()
            .text,
        "hello"
    );
}

#[tokio::test]
async fn audio_non_json_transcription_formats_are_returned_without_json_parsing() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/audio/transcriptions"))
        .and(body_string_contains("srt"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("1\n00:00:00,000 --> 00:00:01,000\nHello\n"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let result = client(&server)
        .audio()
        .transcribe_bytes(
            Form::new()
                .text("model", "caller-model")
                .text("response_format", "srt")
                .part(
                    "file",
                    Part::bytes(b"RIFF".to_vec()).file_name("sample.wav"),
                ),
        )
        .await
        .unwrap();
    assert!(String::from_utf8(result).unwrap().starts_with("1\n00:00:"));
}

#[tokio::test]
async fn multipart_transcription_and_image_edit_streams() {
    let server = MockServer::start().await;
    for endpoint in ["audio/transcriptions", "images/edits"] {
        Mock::given(method("POST"))
            .and(path(format!("/v1/{endpoint}")))
            .and(body_string_contains("name=\"stream\"\r\n\r\ntrue"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw("data: {\"type\":\"done\"}\n\n", "text/event-stream"),
            )
            .expect(1)
            .mount(&server)
            .await;
    }
    let api = client(&server);
    let mut audio = api
        .audio()
        .transcribe_stream(Form::new().text("stream", "true"))
        .await
        .unwrap();
    assert_eq!(
        audio
            .next()
            .await
            .unwrap()
            .unwrap()
            .json::<Value>()
            .unwrap()["type"],
        "done"
    );
    let mut image = api
        .images()
        .edit_stream(Form::new().text("stream", "true"))
        .await
        .unwrap();
    assert_eq!(
        image
            .next()
            .await
            .unwrap()
            .unwrap()
            .json::<Value>()
            .unwrap()["type"],
        "done"
    );
}

#[tokio::test]
async fn moderation_keeps_new_categories() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/moderations"))
        .and(body_json(json!({"input":"hello"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"mod_1","model":"caller-model",
            "results":[{"flagged":false,"categories":{"new/category":false},"category_scores":{"new/category":0.01}}]})))
        .expect(1).mount(&server).await;
    let result = client(&server)
        .moderations()
        .create(&ModerationRequest::new("hello"))
        .await
        .unwrap();
    assert_eq!(result.results[0].category_scores["new/category"], 0.01);
}

#[tokio::test]
async fn advanced_resources_support_nested_actions_and_extensible_requests() {
    let server = MockServer::start().await;
    for endpoint in [
        "batches",
        "fine_tuning/jobs",
        "vector_stores/vs_1/search",
        "evals/eval_1/runs",
        "containers/c_1/files",
        "uploads/up_1/complete",
        "conversations/conv_1/items",
        "realtime/client_secrets",
        "agents",
        "skills/skill_1/versions",
        "live/sessions",
        "webhook_endpoints",
        "decisions",
        "content_provenance_checks",
    ] {
        Mock::given(method("POST"))
            .and(path(format!("/v1/{endpoint}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok":true})))
            .expect(1)
            .mount(&server)
            .await;
    }
    let api = client(&server);
    let _: Value = api
        .batches()
        .create(&BatchRequest::new("file_1", "/v1/responses"))
        .await
        .unwrap();
    let _: Value = api
        .fine_tuning_jobs()
        .create(&FineTuningRequest::new("caller-model", "file_1"))
        .await
        .unwrap();
    for resource in [
        api.vector_stores().child("vs_1").action("search"),
        api.evals().child("eval_1").child("runs"),
        api.containers().child("c_1").child("files"),
        api.uploads().child("up_1").action("complete"),
        api.conversations().child("conv_1").child("items"),
        api.realtime().child("client_secrets"),
        api.agents(),
        api.skills().child("skill_1").child("versions"),
        api.live().child("sessions"),
        api.webhooks(),
        api.decisions(),
        api.content_provenance_checks(),
    ] {
        let _: Value = resource
            .create(&json!({"future_parameter":true}))
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn organization_admin_requests_use_configured_headers_and_queries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/organization/projects/project_1/rate_limits"))
        .and(header("authorization", "Bearer admin-test-key"))
        .and(header("openai-organization", "org_1"))
        .and(header("openai-project", "project_1"))
        .and(query_param("limit", "25"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
        .expect(1)
        .mount(&server)
        .await;
    let config = Config::new(format!("{}/v1/", server.uri()), "admin-test-key")
        .header("OpenAI-Organization", "org_1")
        .unwrap()
        .header("OpenAI-Project", "project_1")
        .unwrap();
    let api = Client::with_config(config).unwrap();
    let _: Value = api
        .organization()
        .child("projects")
        .child("project_1")
        .child("rate_limits")
        .get(&[("limit".into(), "25".into())])
        .await
        .unwrap();
}

#[tokio::test]
async fn dynamic_ids_cannot_inject_subresources_or_queries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/responses/bad%2Fid%3Fx%3D1%23fragment"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response()))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .responses()
        .retrieve("bad/id?x=1#fragment")
        .await
        .unwrap();
}

#[tokio::test]
async fn provider_errors_keep_status_body_and_request_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(400).insert_header("x-request-id","req_123")
            .set_body_json(json!({"error":{"type":"invalid_request_error","message":"bad model","param":"model"}})))
        .expect(1).mount(&server).await;
    match client(&server)
        .responses()
        .create(&ResponseRequest::new("caller-model", "hi"))
        .await
        .unwrap_err()
    {
        Error::Api {
            status,
            request_id,
            body,
            ..
        } => {
            assert_eq!(status, 400);
            assert_eq!(request_id.as_deref(), Some("req_123"));
            assert_eq!(body["error"]["param"], "model");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test]
async fn raw_rest_supports_new_endpoints_and_non_json_results() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/realtime/calls"))
        .and(header("content-type", "application/sdp"))
        .and(body_string_contains("v=0"))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("location", "/v1/realtime/calls/call_1")
                .set_body_string("v=0\r\nanswer"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let result = client(&server)
        .http()
        .raw(
            Method::POST,
            "realtime/calls",
            &[],
            &[("content-type".into(), "application/sdp".into())],
            b"v=0\r\noffer".to_vec(),
        )
        .await
        .unwrap();
    assert_eq!(result.status, 201);
    assert_eq!(result.body, b"v=0\r\nanswer");
    assert_eq!(result.headers["location"], "/v1/realtime/calls/call_1");
}

#[test]
fn response_output_text_skips_non_message_tools_and_missing_content() {
    let mut body = response();
    body["output"].as_array_mut().unwrap().extend([
        json!({"type":"function_call","content":[{"type":"output_text","text":"excluded"}]}),
        json!({"type":"message"}),
        json!({"type":"message","content":[{"type":"output_text","text":"!"}]}),
    ]);
    assert_eq!(
        serde_json::from_value::<Response>(body)
            .unwrap()
            .output_text(),
        "Hello world!"
    );
}

#[test]
fn pagination_rejects_invalid_cursor_and_keeps_metadata() {
    let page: List<Value> = serde_json::from_value(
        json!({"data":[],"has_more":true,"last_id":"after_1","next_page":"retained"}),
    )
    .unwrap();
    assert_eq!(page.next_after().unwrap(), Some("after_1"));
    assert_eq!(page.extra["next_page"], "retained");
    let malformed: List<Value> =
        serde_json::from_value(json!({"data":[],"has_more":true})).unwrap();
    assert!(malformed.next_after().is_err());
}

#[test]
fn requests_omit_absent_fields_and_round_trip_extension_properties() {
    let request = ResponseRequest::new("any-model-string", "hi")
        .with_extra("text", json!({"format":{"type":"json_object"}}));
    let wire = serde_json::to_value(&request).unwrap();
    assert!(!wire.as_object().unwrap().contains_key("tools"));
    let restored: ResponseRequest = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), wire);
}
