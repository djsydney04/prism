// tungstenite's server callback requires a Result with its large HTTP error response.
#![allow(clippy::result_large_err)]

use futures_util::{SinkExt, StreamExt};
use prism_ai_core::{serde_json::json, Client, Config, Error, Value};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_tungstenite::{
    accept_async, accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response},
        Message,
    },
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    (listener, origin)
}

#[tokio::test]
async fn header_auth_json_binary_and_split_round_trip() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = accept_hdr_async(tcp, |request: &Request, response: Response| {
            assert_eq!(request.uri().path(), "/v1/realtime");
            assert_eq!(request.uri().query(), Some("model=future-model"));
            assert_eq!(request.headers()["authorization"], "Bearer test-secret");
            assert_eq!(request.headers()["x-test-header"], "example");
            Ok(response)
        })
        .await
        .unwrap();
        let message = socket.next().await.unwrap().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(message.to_text().unwrap()).unwrap(),
            json!({"type":"response.create"})
        );
        socket.send(Message::Ping(vec![1, 2].into())).await.unwrap();
        socket
            .send(Message::Binary(
                br#"{"type":"response.done","future":true}"#.to_vec().into(),
            ))
            .await
            .unwrap();
        while let Some(message) = socket.next().await {
            if message.unwrap().is_close() {
                break;
            }
        }
    });
    let sdk = Client::new(
        Config::new(origin, "test-secret")
            .header("x-test-header", "example")
            .unwrap(),
    )
    .unwrap();
    let session = sdk
        .websocket("/v1/realtime", &[("model".into(), "future-model".into())])
        .await
        .unwrap();
    assert!(!format!("{session:?}").contains("test-secret"));
    let (mut sender, mut receiver) = session.split();
    let request = json!({"type":"response.create"});
    let (sent, received) = tokio::join!(sender.send_json(&request), receiver.recv_json::<Value>());
    sent.unwrap();
    assert_eq!(received.unwrap().unwrap()["future"], true);
    sender.close().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn query_auth_encodes_key_and_omits_auth_headers() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = accept_hdr_async(tcp, |request: &Request, response: Response| {
            assert_eq!(request.uri().path(), "/ws/live");
            let uri = format!("http://localhost{}", request.uri());
            let url = prism_ai_core::reqwest::Url::parse(&uri).unwrap();
            let pairs: Vec<_> = url.query_pairs().collect();
            assert!(pairs.iter().any(|(k, v)| k == "key" && v == "token +/=?&"));
            assert!(!request.headers().contains_key("authorization"));
            assert!(!request.headers().contains_key("x-goog-api-key"));
            Ok(response)
        })
        .await
        .unwrap();
        socket
            .send(Message::Text("{\"setupComplete\":{}}".into()))
            .await
            .unwrap();
        socket.close(None).await.unwrap();
    });
    let sdk = Client::new(
        Config::new(origin, "token +/=?&")
            .with_api_key_header("x-goog-api-key")
            .unwrap(),
    )
    .unwrap();
    let mut socket = sdk
        .websocket_with_key_query("/ws/live", &[], "key")
        .await
        .unwrap();
    assert!(socket
        .recv_json::<Value>()
        .await
        .unwrap()
        .unwrap()
        .get("setupComplete")
        .is_some());
    assert!(socket.recv_json::<Value>().await.unwrap().is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn custom_key_header_is_supported_for_websockets() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = accept_hdr_async(tcp, |request: &Request, response: Response| {
            assert_eq!(request.headers()["x-api-key"], "secret");
            assert_eq!(request.uri().to_string(), "/ws");
            assert!(!request.headers().contains_key("authorization"));
            Ok(response)
        })
        .await
        .unwrap();
        socket.close(None).await.unwrap();
    });
    let sdk = Client::new(
        Config::new(origin, "secret")
            .with_api_key_header("x-api-key")
            .unwrap(),
    )
    .unwrap();
    let mut socket = sdk.websocket("/ws", &[]).await.unwrap();
    assert!(socket.recv_json::<Value>().await.unwrap().is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn invalid_utf8_websocket_frame_is_a_redacted_protocol_error() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(tcp).await.unwrap();
        // Write an unmasked server text frame containing an invalid UTF-8 byte.
        socket
            .get_mut()
            .write_all(&[0x81, 0x01, 0xff])
            .await
            .unwrap();
    });
    let sdk = Client::new(Config::new(origin, "secret-key")).unwrap();
    let mut socket = sdk.websocket("/ws", &[]).await.unwrap();
    let error = socket.recv_message().await.unwrap_err();
    assert!(matches!(error, Error::Protocol(_)));
    assert_eq!(
        error.to_string(),
        "protocol error: WebSocket protocol error"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn invalid_json_diagnostics_do_not_contain_payloads() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(tcp).await.unwrap();
        socket
            .send(Message::Text("secret-payload-should-not-appear".into()))
            .await
            .unwrap();
        socket.close(None).await.unwrap();
    });
    let sdk = Client::new(Config::new(origin, "secret-key")).unwrap();
    let mut socket = sdk.websocket("/ws", &[]).await.unwrap();
    let error = socket.recv_json::<Value>().await.unwrap_err();
    assert!(matches!(error, Error::Protocol(_)));
    assert!(!format!("{error:?}").contains("secret-payload"));
    server.await.unwrap();
}

#[tokio::test]
async fn oversized_received_and_sent_messages_are_rejected() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(tcp).await.unwrap();
        socket
            .send(Message::Text("x".repeat(65).into()))
            .await
            .unwrap();
    });
    let sdk = Client::new(Config::new(origin, "secret").max_event_bytes(64)).unwrap();
    let mut socket = sdk.websocket("/ws", &[]).await.unwrap();
    assert!(matches!(
        socket.send_text("x".repeat(65)).await,
        Err(Error::Protocol(_))
    ));
    assert!(matches!(
        socket.recv_message().await,
        Err(Error::Protocol(_))
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn redirect_handshakes_are_not_followed_and_errors_are_redacted() {
    let server = MockServer::start().await;
    let destination = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/redirect"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header(
                    "location",
                    format!("{}/steal?secret=key", destination.uri()),
                )
                .set_body_string("secret-response-body"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let sdk = Client::new(Config::new(server.uri(), "secret-key")).unwrap();
    let error = sdk
        .websocket_with_key_query("/redirect", &[], "key")
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(302));
    let debug = format!("{error:?}");
    for secret in ["secret-key", "secret-response-body", "/steal"] {
        assert!(!debug.contains(secret));
    }
    assert!(destination.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn invalid_paths_and_duplicate_authentication_are_rejected_before_connecting() {
    let server = MockServer::start().await;
    let sdk = Client::new(Config::new(server.uri(), "secret-key")).unwrap();
    for path in [
        "https://evil.invalid/ws",
        "//evil.invalid/ws",
        "/a/../ws",
        "/ws?key=steal",
        "/ws#fragment",
    ] {
        assert!(matches!(
            sdk.websocket(path, &[]).await,
            Err(Error::Config(_))
        ));
    }
    assert!(sdk
        .websocket_with_key_query("/ws", &[("key".into(), "replacement".into())], "key")
        .await
        .is_err());
    assert!(sdk
        .websocket_with_key_query("/ws", &[], "bad&key")
        .await
        .is_err());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn websocket_handshake_has_a_deadline() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let (_tcp, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    let sdk =
        Client::new(Config::new(origin, "secret-key").timeout(Duration::from_millis(20))).unwrap();
    let error = sdk.websocket("/ws", &[]).await.unwrap_err();
    assert!(matches!(error, Error::Protocol(_)));
    assert!(!format!("{error:?}").contains("secret-key"));
    server.await.unwrap();
}
