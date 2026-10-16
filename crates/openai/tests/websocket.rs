// tungstenite fixes the server callback's large HTTP error response type.
#![allow(clippy::result_large_err)]

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sydney_openai::{Client, Config};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response},
        Message,
    },
};

async fn mock_socket(
    expected_uri: &str,
    expected_message: Value,
    server_events: Vec<Value>,
) -> (Client, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let expected_uri = expected_uri.to_owned();
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(5), async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket =
                accept_hdr_async(stream, move |request: &Request, response: Response| {
                    assert_eq!(request.uri().to_string(), expected_uri);
                    assert_eq!(request.headers()["authorization"], "Bearer test-openai-key");
                    assert_eq!(request.headers()["openai-project"], "project_1");
                    Ok(response)
                })
                .await
                .unwrap();
            let message = socket.next().await.unwrap().unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(message.to_text().unwrap()).unwrap(),
                expected_message
            );
            for event in server_events {
                socket
                    .send(Message::Text(event.to_string().into()))
                    .await
                    .unwrap();
            }
            socket.close(None).await.unwrap();
        })
        .await
        .expect("mock WebSocket timed out");
    });
    let client = Client::with_config(
        Config::new(format!("http://{address}/v1/"), "test-openai-key")
            .header("OpenAI-Project", "project_1")
            .unwrap(),
    )
    .unwrap();
    (client, server)
}

#[tokio::test]
async fn responses_websocket_authenticates_and_retains_new_multiplexed_events() {
    let event = json!({"type":"response.create","stream_id":"lane-1","model":"caller-model",
        "input":[{"role":"user","content":"Hello"}],"store":false});
    let (client, server) = mock_socket(
        "/v1/responses",
        event.clone(),
        vec![
            json!({"type":"response.output_text.delta","stream_id":"lane-1","delta":"Hello","future":true}),
            json!({"type":"response.completed","stream_id":"lane-1","response":{"id":"resp_1"}}),
        ],
    )
    .await;
    let mut session = client.responses().connect().await.unwrap();
    session.send_json(&event).await.unwrap();
    let delta: Value = session.recv_json().await.unwrap().unwrap();
    assert_eq!(delta["stream_id"], "lane-1");
    assert_eq!(delta["future"], true);
    let completed: Value = session.recv_json().await.unwrap().unwrap();
    assert_eq!(completed["response"]["id"], "resp_1");
    assert!(session.recv_json::<Value>().await.unwrap().is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn realtime_websocket_escapes_model_query_and_preserves_tool_call_events() {
    let event = json!({"type":"session.update","session":{"type":"realtime",
        "instructions":"Call the authorized tools when needed"}});
    let (client, server) = mock_socket(
        "/v1/realtime?model=caller%2Fmodel%3Fversion%3D1",
        event.clone(),
        vec![
            json!({"type":"response.function_call_arguments.done","name":"lookup",
            "call_id":"call_1","arguments":"{\"id\":42}"}),
        ],
    )
    .await;
    let mut session = client
        .realtime_connect("caller/model?version=1")
        .await
        .unwrap();
    session.send_json(&event).await.unwrap();
    let tool_call: Value = session.recv_json().await.unwrap().unwrap();
    assert_eq!(tool_call["call_id"], "call_1");
    assert_eq!(tool_call["arguments"], "{\"id\":42}");
    server.await.unwrap();
}

#[tokio::test]
async fn live_websocket_uses_primary_path_and_session_start_handshake() {
    let event = json!({"type":"session.start","session":{"model":"caller-live-model",
        "audio":{"format":{"type":"audio/pcm","rate":24000}},"delegation":{"type":"client"}}});
    let (client, server) = mock_socket(
        "/v1/live/sessions",
        event.clone(),
        vec![
            json!({"type":"session.started","session":{"id":"live_1","model":"caller-live-model"}}),
        ],
    )
    .await;
    let mut session = client.live_connect(&[]).await.unwrap();
    session.send_json(&event).await.unwrap();
    let ready: Value = session.recv_json().await.unwrap().unwrap();
    assert_eq!(ready["type"], "session.started");
    assert_eq!(ready["session"]["id"], "live_1");
    server.await.unwrap();
}

#[tokio::test]
async fn generic_websocket_supports_sideband_routes_and_split_io() {
    let event = json!({"type":"session.update","session":{"instructions":"Help the caller"}});
    let (client, server) = mock_socket(
        "/v1/realtime?call_id=call_123",
        event.clone(),
        vec![json!({"type":"session.updated","future_event_data":{"retained":true}})],
    )
    .await;
    let session = client
        .websocket("realtime", &[("call_id".into(), "call_123".into())])
        .await
        .unwrap();
    let (mut sender, mut receiver) = session.split();
    sender.send_json(&event).await.unwrap();
    let updated: Value = receiver.recv_json().await.unwrap().unwrap();
    assert_eq!(updated["future_event_data"]["retained"], true);
    server.await.unwrap();
}
