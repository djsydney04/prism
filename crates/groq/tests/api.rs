use futures_util::StreamExt;
use serde_json::{json, Value};
use sydney_groq::*;
use wiremock::{
    matchers::{body_json, header, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

fn client(server: &MockServer) -> Client {
    Client::with_config(Config::new(format!("{}/", server.uri()), "test-key")).unwrap()
}
fn completion() -> Value {
    json!({"id":"chat-1","object":"chat.completion","created":1,"model":"future/model",
        "choices":[{"index":0,"message":{"role":"assistant","content":"Hello","future_message":{"ok":true}},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":2,"completion_tokens":1,"total_tokens":3,"future_usage":7},"future_response":"retained"})
}
fn file() -> Value {
    json!({"id":"file-1","object":"file","bytes":12,"created_at":1,"filename":"batch.jsonl","purpose":"batch"})
}
fn batch() -> Value {
    json!({"id":"batch-1","object":"batch","endpoint":"/v1/chat/completions","input_file_id":"file-1", "completion_window":"24h","status":"validating","created_at":1,
        "output_file_id":null,"request_counts":{"total":1,"completed":0,"failed":0}})
}
fn fine_tuning() -> Value {
    json!({"id":"fine-1","name":"name","base_model":"model","type":"lora","input_file_id":"file-1","created_at":1,"fine_tuned_model":"my-model"})
}

#[tokio::test]
async fn chat_preserves_multimodal_tools_structured_output_and_future_fields() {
    let server = MockServer::start().await;
    let mut request = ChatRequest::new(
        "future/model",
        vec![Message::user(vec![
            ContentPart::text("Describe"),
            ContentPart::image("data:image/png;base64,YQ=="),
        ])],
    );
    request.tools = Some(vec![Tool::function(
        "lookup",
        json!({"type":"object","properties":{"id":{"type":"string"}}}),
    )]);
    request.response_format = Some(ResponseFormat::json_schema(
        "result",
        json!({"type":"object"}),
        true,
    ));
    request.reasoning_effort = Some("high".into());
    request
        .extra
        .insert("future_parameter".into(), json!({"enabled":true}));
    let mut body = serde_json::to_value(&request).unwrap();
    body["stream"] = json!(false);
    Mock::given(method("POST"))
        .and(path("/openai/v1/chat/completions"))
        .and(header("authorization", "Bearer test-key"))
        .and(body_json(body))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion()))
        .expect(1)
        .mount(&server)
        .await;
    let response = client(&server).chat().create(&request).await.unwrap();
    assert_eq!(response.extra["future_response"], "retained");
    assert_eq!(
        response.choices[0].message.extra["future_message"]["ok"],
        true
    );
    assert_eq!(response.usage.unwrap().extra["future_usage"], 7);
    assert!(
        request.stream.is_none(),
        "stream coercion must not modify the caller's request"
    );
}

#[tokio::test]
async fn chat_stream_forces_streaming_and_decodes_tool_argument_fragments() {
    let server = MockServer::start().await;
    let event = json!({"id":"chat-1","object":"chat.completion.chunk","created":1,"model":"future/model",
        "choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"id\":"}}]},"finish_reason":null}]});
    Mock::given(method("POST"))
        .and(path("/openai/v1/chat/completions"))
        .and(wiremock::matchers::body_partial_json(
            json!({"stream":true}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!("data: {event}\r\n\r\ndata: [DONE]\r\n\r\n"),
            "text/event-stream",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server);
    let mut request = ChatRequest::new("future/model", vec![Message::user("Hi")]);
    request.stream = Some(false);
    let mut stream = client.chat().stream(&request).await.unwrap();
    let chunk: ChatChunk = stream.next().await.unwrap().unwrap().json().unwrap();
    assert_eq!(
        chunk.choices[0].delta.tool_calls.as_ref().unwrap()[0]
            .function
            .as_ref()
            .unwrap()
            .arguments,
        "{\"id\":"
    );
    assert!(stream.next().await.is_none());
    assert_eq!(request.stream, Some(false));
}

#[tokio::test]
async fn responses_create_and_stream_use_documented_beta_endpoint() {
    let server = MockServer::start().await;
    let response = json!({"id":"resp-1","object":"response","created_at":1,"model":"future/model","status":"completed","output":[
        {"type":"message","content":[{"type":"output_text","text":"One"},{"type":"output_text","text":"Two"}]}],"new_response_field":1});
    Mock::given(method("POST"))
        .and(path("/openai/v1/responses"))
        .and(wiremock::matchers::body_partial_json(
            json!({"stream":false,"input":"Hi"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let request = ResponseRequest::new("future/model", "Hi");
    let response = c.responses().create(&request).await.unwrap();
    assert_eq!(response.output_text(), "OneTwo");
    assert_eq!(response.extra["new_response_field"], 1);
    Mock::given(method("POST"))
        .and(path("/openai/v1/responses"))
        .and(wiremock::matchers::body_partial_json(
            json!({"stream":true}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "event: response.output_text.delta\ndata: {\"delta\":\"hello\"}\n\n",
            "text/event-stream",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let mut stream = c.responses().stream(&request).await.unwrap();
    let event = stream.next().await.unwrap().unwrap();
    assert_eq!(event.event.as_deref(), Some("response.output_text.delta"));
    assert_eq!(event.json::<Value>().unwrap()["delta"], "hello");
}

#[tokio::test]
async fn model_discovery_handles_namespaced_ids() {
    let server = MockServer::start().await;
    let model = json!({"id":"openai/gpt-oss-120b","object":"model","created":1,"owned_by":"OpenAI","active":true,"context_window":131072,"future_field":1});
    Mock::given(method("GET"))
        .and(path("/openai/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"object":"list","data":[model.clone()]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/openai/v1/models/openai%2Fgpt-oss-120b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(model))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    assert_eq!(c.models().list().await.unwrap().data.len(), 1);
    assert_eq!(
        c.models()
            .retrieve("openai/gpt-oss-120b")
            .await
            .unwrap()
            .extra["future_field"],
        1
    );
}

#[tokio::test]
async fn files_upload_list_retrieve_download_and_delete() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/openai/v1/files"))
        .and(header("authorization", "Bearer test-key"))
        .and(wiremock::matchers::body_string_contains("name=\"purpose\""))
        .and(wiremock::matchers::body_string_contains(
            "filename=\"batch.jsonl\"",
        ))
        .and(wiremock::matchers::body_string_contains(
            "{\"custom_id\":\"one\"}",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(file()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/openai/v1/files"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"object":"list","data":[file()]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/openai/v1/files/file-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(file()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/openai/v1/files/file-1/content"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0, 255, 42]))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/openai/v1/files/file-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id":"file-1","object":"file","deleted":true})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    assert_eq!(
        c.files()
            .upload("batch.jsonl", b"{\"custom_id\":\"one\"}\n".to_vec())
            .await
            .unwrap()
            .purpose,
        "batch"
    );
    assert_eq!(c.files().list().await.unwrap().data.len(), 1);
    assert_eq!(c.files().retrieve("file-1").await.unwrap().bytes, 12);
    assert_eq!(c.files().content("file-1").await.unwrap(), [0, 255, 42]);
    assert!(c.files().delete("file-1").await.unwrap().deleted);
}

#[tokio::test]
async fn batches_cover_create_retrieve_pagination_and_cancellation() {
    let server = MockServer::start().await;
    let request = BatchRequest::new("file-1", "/v1/chat/completions");
    Mock::given(method("POST"))
        .and(path("/openai/v1/batches"))
        .and(body_json(serde_json::to_value(&request).unwrap()))
        .respond_with(ResponseTemplate::new(200).set_body_json(batch()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/openai/v1/batches/batch-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(batch()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/openai/v1/batches"))
        .and(query_param("after", "batch-0"))
        .and(query_param("limit", "25"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"object":"list","data":[batch()],"has_more":true,"last_id":"batch-1"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/openai/v1/batches/batch-1/cancel"))
        .respond_with(ResponseTemplate::new(200).set_body_json(batch()))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    assert_eq!(c.batches().create(&request).await.unwrap().id, "batch-1");
    assert_eq!(
        c.batches().retrieve("batch-1").await.unwrap().status,
        "validating"
    );
    let page = c
        .batches()
        .list(&ListOptions {
            after: Some("batch-0".into()),
            limit: Some(25),
            ..ListOptions::default()
        })
        .await
        .unwrap();
    assert_eq!(page.has_more, Some(true));
    assert_eq!(page.last_id.as_deref(), Some("batch-1"));
    assert_eq!(c.batches().cancel("batch-1").await.unwrap().id, "batch-1");
}

#[tokio::test]
async fn audio_upload_and_url_transcription_verbose_translation_and_raw_text() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/openai/v1/audio/transcriptions"))
        .and(wiremock::matchers::body_string_contains(
            "filename=\"sample.wav\"",
        ))
        .and(wiremock::matchers::body_string_contains(
            "name=\"timestamp_granularities[]\"",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"text":"Hi","segments":[{"start":0,"end":1,"text":"Hi"}],"duration":1.0}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/openai/v1/audio/translations"))
        .and(wiremock::matchers::body_string_contains(
            "https://example.com/a.wav",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"text":"Hello"})))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let mut request = AudioRequest::file("whisper-large-v3", "sample.wav", b"wav-data".to_vec());
    request.response_format = Some("verbose_json".into());
    request.timestamp_granularities = vec!["word".into(), "segment".into()];
    let response = c.audio().transcribe(request).await.unwrap();
    assert_eq!(response.segments.unwrap()[0]["text"], "Hi");
    assert_eq!(
        c.audio()
            .translate(AudioRequest::url(
                "whisper-large-v3",
                "https://example.com/a.wav"
            ))
            .await
            .unwrap()
            .text,
        "Hello"
    );
    Mock::given(method("POST"))
        .and(path("/openai/v1/audio/transcriptions"))
        .and(wiremock::matchers::body_string_contains(
            "https://example.com/text.wav",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("raw transcript\n"))
        .expect(1)
        .mount(&server)
        .await;
    let mut request = AudioRequest::url("whisper-large-v3", "https://example.com/text.wav");
    request.response_format = Some("text".into());
    assert_eq!(
        c.audio().transcribe_bytes(request).await.unwrap(),
        b"raw transcript\n"
    );
}

#[tokio::test]
async fn speech_returns_unmodified_binary_and_serializes_audio_parameters() {
    let server = MockServer::start().await;
    let mut request = SpeechRequest::new("future-tts", "Hello", "voice-name");
    request.response_format = Some("wav".into());
    request.sample_rate = Some(24000);
    request.speed = Some(1.25);
    Mock::given(method("POST"))
        .and(path("/openai/v1/audio/speech"))
        .and(body_json(serde_json::to_value(&request).unwrap()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0, 1, 255]))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        client(&server).audio().speech(&request).await.unwrap(),
        [0, 1, 255]
    );
}

#[tokio::test]
async fn fine_tuning_uses_native_v1_prefix_for_every_operation() {
    let server = MockServer::start().await;
    let wrapped = json!({"id":"fine-1","object":"object","data":fine_tuning()});
    Mock::given(method("GET"))
        .and(path("/v1/fine_tunings"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"object":"list","data":[fine_tuning()]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/fine_tunings"))
        .and(wiremock::matchers::body_partial_json(
            json!({"type":"lora","base_model":"model"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(wrapped.clone()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/fine_tunings/fine-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(wrapped))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/fine_tunings/fine-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id":"fine-1","object":"fine_tuning","deleted":true})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    assert_eq!(c.fine_tunings().list().await.unwrap().data.len(), 1);
    let request = FineTuningRequest {
        base_model: Some("model".into()),
        kind: Some("lora".into()),
        ..FineTuningRequest::default()
    };
    assert_eq!(
        c.fine_tunings().create(&request).await.unwrap().data.kind,
        "lora"
    );
    assert_eq!(
        c.fine_tunings()
            .retrieve("fine-1")
            .await
            .unwrap()
            .data
            .fine_tuned_model,
        "my-model"
    );
    assert!(c.fine_tunings().delete("fine-1").await.unwrap().deleted);
}

#[tokio::test]
async fn provider_errors_retain_status_message_request_id_and_payload() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/openai/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("x-request-id", "req-test")
                .set_body_json(
                    json!({"error":{"message":"slow down","type":"rate_limit","extra":42}}),
                ),
        )
        .expect(1)
        .mount(&server)
        .await;
    let error = client(&server)
        .chat()
        .create(&ChatRequest::new("model", vec![Message::user("Hi")]))
        .await
        .unwrap_err();
    match error {
        Error::Api {
            status,
            message,
            request_id,
            body,
        } => {
            assert_eq!(status, 429);
            assert_eq!(message, "slow down");
            assert_eq!(request_id.as_deref(), Some("req-test"));
            assert_eq!(body["error"]["extra"], 42);
        }
        other => panic!("unexpected error {other}"),
    }
}

#[test]
fn request_extensions_jsonl_and_message_tool_calls_round_trip() {
    let mut tool = Message::tool("call-1", "result");
    tool.extra.insert("new_field".into(), json!(true));
    let serialized = serde_json::to_value(&tool).unwrap();
    assert_eq!(serialized["tool_call_id"], "call-1");
    assert_eq!(serialized["role"], "tool");
    assert!(
        serde_json::from_value::<Message>(serialized).unwrap().extra["new_field"]
            .as_bool()
            .unwrap()
    );
    let item = BatchItem {
        custom_id: "item-1".into(),
        method: "POST".into(),
        url: "/v1/chat/completions".into(),
        body: ChatRequest::new("future-model", vec![Message::user("one\ntwo")]),
        extra: Extra::new(),
    };
    let jsonl = item.to_jsonl().unwrap();
    assert_eq!(jsonl.lines().count(), 1);
    assert!(jsonl.ends_with('\n'));
    assert_eq!(
        serde_json::from_str::<Value>(&jsonl).unwrap()["body"]["model"],
        "future-model"
    );
}
