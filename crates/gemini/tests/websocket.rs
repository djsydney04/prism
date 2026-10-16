// tungstenite's server callback requires a Result with its large HTTP error response.
#![allow(clippy::result_large_err)]

use futures_util::{SinkExt, StreamExt};
use prism_gemini::{json, Client, Config, GenerationConfig, LiveServerMessage, LiveSetup, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response},
        Message,
    },
};

#[tokio::test]
async fn live_key_handshake_setup_audio_video_text_and_tools() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = accept_hdr_async(tcp, |request: &Request, response: Response| {
            assert_eq!(
                request.uri().path(),
                "/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"
            );
            assert_eq!(request.uri().query(), Some("key=test-secret"));
            assert!(!request.headers().contains_key("x-goog-api-key"));
            assert!(!request.headers().contains_key("authorization"));
            Ok(response)
        })
        .await
        .unwrap();
        let setup = socket.next().await.unwrap().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(setup.to_text().unwrap()).unwrap(),
            json!({"setup":{"model":"models/future-live","generationConfig":{"responseModalities":["AUDIO"]}}})
        );
        socket
            .send(Message::Text(
                "{\"setupComplete\":{},\"newEventField\":true}".into(),
            ))
            .await
            .unwrap();
        for expected in [
            json!({"realtimeInput":{"text":"hi"}}),
            json!({"realtimeInput":{"audio":{"mimeType":"audio/pcm;rate=16000","data":"AAAA"}}}),
            json!({"realtimeInput":{"video":{"mimeType":"image/jpeg","data":"AAAA"}}}),
            json!({"toolResponse":{"functionResponses":[{"id":"call1","name":"lookup","response":{"result":42}}]}}),
        ] {
            let message = socket.next().await.unwrap().unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(message.to_text().unwrap()).unwrap(),
                expected
            );
        }
        socket.send(Message::Binary(br#"{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"mimeType":"audio/pcm;rate=24000","data":"AAAA"}}]},"turnComplete":true},"usageMetadata":{"totalTokenCount":5}}"#.to_vec().into())).await.unwrap();
        socket.close(None).await.unwrap();
    });
    let sdk = Client::with_config(Config::new(origin, "test-secret")).unwrap();
    let mut session = sdk
        .live_connect(&LiveSetup {
            model: "models/future-live".into(),
            generation_config: Some(GenerationConfig {
                response_modalities: vec!["AUDIO".into()],
                ..GenerationConfig::default()
            }),
            ..LiveSetup::default()
        })
        .await
        .unwrap();
    let setup: LiveServerMessage = session.recv_json().await.unwrap().unwrap();
    assert!(setup.setup_complete.is_some());
    assert_eq!(setup.extra["newEventField"], true);
    session
        .send_json(&json!({"realtimeInput":{"text":"hi"}}))
        .await
        .unwrap();
    session
        .send_json(
            &json!({"realtimeInput":{"audio":{"mimeType":"audio/pcm;rate=16000","data":"AAAA"}}}),
        )
        .await
        .unwrap();
    session
        .send_json(&json!({"realtimeInput":{"video":{"mimeType":"image/jpeg","data":"AAAA"}}}))
        .await
        .unwrap();
    session.send_json(&json!({"toolResponse":{"functionResponses":[{"id":"call1","name":"lookup","response":{"result":42}}]}})).await.unwrap();
    let output: LiveServerMessage = session.recv_json().await.unwrap().unwrap();
    assert_eq!(output.server_content.unwrap()["turnComplete"], true);
    assert_eq!(output.usage_metadata.unwrap()["totalTokenCount"], 5);
    assert!(session.recv_json::<Value>().await.unwrap().is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn ephemeral_live_uses_constrained_endpoint_and_access_token() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = accept_hdr_async(tcp, |request: &Request, response: Response| {
            assert_eq!(request.uri().path(), "/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContentConstrained");
            assert_eq!(request.uri().query(), Some("access_token=short-lived-token"));
            assert!(!request.headers().contains_key("x-goog-api-key"));
            assert!(!request.headers().contains_key("authorization"));
            Ok(response)
        }).await.unwrap();
        let setup = socket.next().await.unwrap().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(setup.to_text().unwrap()).unwrap(),
            json!({"setup":{"model":"models/future-live","sessionResumption":{"handle":"previous"}}})
        );
        socket
            .send(Message::Text("{\"setupComplete\":{}}".into()))
            .await
            .unwrap();
        socket.close(None).await.unwrap();
    });
    let sdk = Client::with_config(Config::new(origin, "short-lived-token")).unwrap();
    let mut session = sdk
        .live_connect_ephemeral(
            &json!({"model":"models/future-live","sessionResumption":{"handle":"previous"}}),
        )
        .await
        .unwrap();
    assert!(session
        .recv_json::<LiveServerMessage>()
        .await
        .unwrap()
        .unwrap()
        .setup_complete
        .is_some());
    server.await.unwrap();
}

#[tokio::test]
async fn live_rejects_nonobject_setup_before_connecting() {
    let sdk = Client::with_config(Config::new("http://127.0.0.1:1", "unused")).unwrap();
    assert!(sdk.live_connect(&json!("invalid")).await.is_err());
    assert!(sdk.live_connect_ephemeral(&json!([])).await.is_err());
}

#[tokio::test]
async fn live_rejects_oauth_client_before_connecting() {
    let sdk = Client::with_oauth_config(Config::new("http://127.0.0.1:1", "unused-token")).unwrap();
    for result in [
        sdk.live_connect(&json!({"model":"models/a"})).await,
        sdk.live_connect_ephemeral(&json!({"model":"models/a"}))
            .await,
    ] {
        match result {
            Err(prism_gemini::Error::Config(message)) => assert!(message.contains("OAuth")),
            other => panic!("unexpected result: {other:?}"),
        }
    }
}
