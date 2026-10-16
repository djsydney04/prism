use futures_util::StreamExt;
use prism_ai_core::{Client, Config, Error, Method, Page};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn client(server: &MockServer) -> Client {
    Client::new(Config::new(format!("{}/v1", server.uri()), "secret-test-key").max_retries(0))
        .unwrap()
}

#[tokio::test]
async fn json_auth_base_path_and_queries() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .and(header("authorization", "Bearer secret-test-key"))
        .and(body_json(json!({"input":"hello"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok":true})))
        .expect(1)
        .mount(&s)
        .await;
    let body: Value = client(&s)
        .post("responses", &json!({"input":"hello"}))
        .await
        .unwrap();
    assert_eq!(body["ok"], true);
    Mock::given(method("GET"))
        .and(path("/v1/files"))
        .and(query_param("after", "a b&c"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&s)
        .await;
    let _: Value = client(&s)
        .get("files", &[("after".into(), "a b&c".into())])
        .await
        .unwrap();
}

#[tokio::test]
async fn api_key_header_replaces_bearer() {
    let s = MockServer::start().await;
    Mock::given(path("/v1/models"))
        .and(header("x-goog-api-key", "google-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&s)
        .await;
    let c = Client::new(
        Config::new(format!("{}/v1/", s.uri()), "google-key")
            .with_api_key_header("x-goog-api-key")
            .unwrap(),
    )
    .unwrap();
    let _: Value = c.get("models", &[]).await.unwrap();
    assert!(!s.received_requests().await.unwrap()[0]
        .headers
        .contains_key("authorization"));
}

#[tokio::test]
async fn api_errors_preserve_status_request_id_and_body() {
    let s = MockServer::start().await;
    Mock::given(path("/v1/fail"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("x-request-id", "req-123")
                .set_body_json(json!({"error":{"message":"slow down","type":"rate_limit"}})),
        )
        .mount(&s)
        .await;
    let e = client(&s).get::<Value>("fail", &[]).await.unwrap_err();
    assert_eq!(e.status(), Some(429));
    assert_eq!(e.request_id(), Some("req-123"));
    assert!(e.is_retryable());
    assert!(matches!(e,Error::Api{body,..} if body["error"]["type"]=="rate_limit"));
}

#[tokio::test]
async fn non_json_errors_and_empty_success() {
    let s = MockServer::start().await;
    Mock::given(path("/v1/bad"))
        .respond_with(ResponseTemplate::new(502).set_body_string("upstream unavailable"))
        .mount(&s)
        .await;
    assert!(matches!(
        client(&s).get::<Value>("bad", &[]).await,
        Err(Error::Api {
            status: 502,
            body: Value::String(_),
            ..
        })
    ));
    Mock::given(method("DELETE"))
        .and(path("/v1/item"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&s)
        .await;
    client(&s).delete::<()>("item").await.unwrap();
}

#[tokio::test]
async fn rejects_redirect_without_leaking_auth() {
    let a = MockServer::start().await;
    let b = MockServer::start().await;
    Mock::given(path("/v1/redirect"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", b.uri()))
        .mount(&a)
        .await;
    assert_eq!(
        client(&a)
            .get::<Value>("redirect", &[])
            .await
            .unwrap_err()
            .status(),
        Some(302)
    );
    assert!(b.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn rejects_unsafe_paths_before_network() {
    let s = MockServer::start().await;
    let c = client(&s);
    for p in [
        "https://evil.test/x",
        "//evil.test/x",
        "../x",
        "a/../x",
        "%2e%2e/x",
        "a/%2E%2E/x",
        "%2e%2e%2Fx",
        "x?key=y",
        "x#frag",
        "a\\b",
        "x\n",
        "http:evil",
    ] {
        assert!(
            matches!(c.get::<Value>(p, &[]).await, Err(Error::Config(_))),
            "unsafe path: {p}"
        );
    }
    assert!(s.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn raw_upload_enforces_origin_and_auth_headers() {
    let s = MockServer::start().await;
    let other = MockServer::start().await;
    let c = client(&s);
    assert!(c
        .raw(Method::POST, &other.uri(), &[], &[], vec![])
        .await
        .is_err());
    assert!(c
        .raw(
            Method::POST,
            "upload",
            &[],
            &[("Authorization".into(), "evil".into())],
            vec![]
        )
        .await
        .is_err());
    Mock::given(method("POST"))
        .and(path("/upload/v1beta/files"))
        .and(query_param("upload_id", "opaque"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-upload-status", "final")
                .set_body_json(json!({"ok":true})),
        )
        .expect(1)
        .mount(&s)
        .await;
    let url = format!("{}/upload/v1beta/files?upload_id=opaque", s.uri());
    let r = c
        .raw(
            Method::POST,
            &url,
            &[],
            &[("content-type".into(), "application/octet-stream".into())],
            vec![0, 1, 2],
        )
        .await
        .unwrap();
    assert_eq!(r.status, 200);
    assert_eq!(r.headers["x-upload-status"], "final");
    assert!(r.json::<Value>().unwrap()["ok"].as_bool().unwrap());
}

#[tokio::test]
async fn opaque_ids_are_encoded_without_injecting_query() {
    let s = MockServer::start().await;
    let c = client(&s);
    let r = c.resource("models").child("openai/gpt-oss-120b");
    assert_eq!(r.path(), "models/openai%2Fgpt-oss-120b");
    assert_eq!(
        c.resource("files").child("id?x=1#").path(),
        "files/id%3Fx%3D1%23"
    );
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&s)
        .await;
    let _: Value = r.get(&[]).await.unwrap();
}

#[tokio::test]
async fn retry_only_safe_requests() {
    let s = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    Mock::given(path("/retry"))
        .respond_with(move |_: &Request| {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(429).insert_header("retry-after", "0")
            } else {
                ResponseTemplate::new(200).set_body_json(json!({"ok":true}))
            }
        })
        .mount(&s)
        .await;
    let c = Client::new(Config::new(s.uri(), "key").max_retries(2)).unwrap();
    let _: Value = c.get("retry", &[]).await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
    Mock::given(method("POST"))
        .and(path("/mutate"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&s)
        .await;
    assert_eq!(
        c.post::<_, Value>("mutate", &json!({}))
            .await
            .unwrap_err()
            .status(),
        Some(500)
    );
}

#[tokio::test]
async fn bounded_responses_and_events() {
    let s = MockServer::start().await;
    Mock::given(path("/big"))
        .respond_with(ResponseTemplate::new(200).set_body_string("0123456789"))
        .mount(&s)
        .await;
    let c = Client::new(
        Config::new(s.uri(), "key")
            .max_response_bytes(5)
            .max_event_bytes(5),
    )
    .unwrap();
    assert!(matches!(
        c.get::<Value>("big", &[]).await,
        Err(Error::Protocol(_))
    ));
    Mock::given(path("/events"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("data: 1\n\n", "text/event-stream"))
        .mount(&s)
        .await;
    let mut stream = c.stream(Method::GET, "events", &[], None).await.unwrap();
    assert!(stream.next().await.unwrap().is_err());
}

#[tokio::test]
async fn stream_done_and_content_type_validation() {
    let s = MockServer::start().await;
    let c = client(&s);
    Mock::given(path("/v1/events"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "event: delta\ndata: {\"text\":\"hi\"}\n\ndata: [DONE]\n\ndata: ignored\n\n",
            "text/event-stream; charset=utf-8",
        ))
        .mount(&s)
        .await;
    let mut events = c
        .stream(Method::POST, "events", &[], Some(&json!({"stream":true})))
        .await
        .unwrap();
    let e = events.next().await.unwrap().unwrap();
    assert_eq!(e.event.as_deref(), Some("delta"));
    assert_eq!(e.json::<Value>().unwrap()["text"], "hi");
    assert!(events.next().await.is_none());
    Mock::given(path("/v1/not-events"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&s)
        .await;
    assert!(matches!(
        c.stream(Method::POST, "not-events", &[], None).await,
        Err(Error::Protocol(_))
    ));
}

#[tokio::test]
async fn multipart_and_binary_preserve_bytes() {
    let s = MockServer::start().await;
    let c = client(&s);
    Mock::given(method("POST"))
        .and(path("/v1/audio"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0, 255, 1]))
        .expect(1)
        .mount(&s)
        .await;
    let form = reqwest::multipart::Form::new()
        .text("model", "whisper")
        .part(
            "file",
            reqwest::multipart::Part::bytes(vec![0, 1, 255]).file_name("a.wav"),
        );
    assert_eq!(
        c.multipart_bytes(Method::POST, "audio", &[], form)
            .await
            .unwrap(),
        vec![0, 255, 1]
    );
    let request = &s.received_requests().await.unwrap()[0];
    assert!(request.headers["content-type"]
        .to_str()
        .unwrap()
        .starts_with("multipart/form-data; boundary="));
    assert!(request.body.windows(3).any(|w| w == [0, 1, 255]));
}

#[test]
fn config_validation_and_redaction() {
    let config = Config::new(
        "https://user:password@example.com/v1?api_key=secret",
        "SUPER_CREDENTIAL",
    );
    let debug = format!("{config:?}");
    for secret in ["user:", "password", "secret", "SUPER_CREDENTIAL"] {
        assert!(!debug.contains(secret), "{debug}");
    }
    assert!(Client::new(config).is_err());
    for url in [
        "invalid",
        "http://example.com/v1",
        "file:///tmp/x",
        "https://example.com/#frag",
    ] {
        assert!(Client::new(Config::new(url, "key")).is_err());
    }
    assert!(Client::new(Config::new("https://example.com", "")).is_err());
    assert!(Config::new("https://example.com", "key")
        .header("authorization", "bad")
        .is_err());
    let c = Client::new(Config::new("https://example.com", "SUPER_SECRET")).unwrap();
    assert!(!format!("{c:?}").contains("SUPER_SECRET"));
}

#[test]
fn pagination_rejects_missing_cursor() {
    let p: Page<Value> = serde_json::from_value(json!({"data":[],"has_more":true})).unwrap();
    assert!(p.next_after().is_err());
    let p: Page<Value> = serde_json::from_value(
        json!({"data":[{"id":"a"}],"has_more":true,"last_id":"a","new_field":42}),
    )
    .unwrap();
    assert_eq!(p.next_after().unwrap(), Some("a"));
    assert_eq!(p.extra["new_field"], 42);
}

#[tokio::test]
async fn transport_errors_strip_signed_urls_in_display_and_debug() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let c = Client::new(Config::new(format!("http://{address}"), "key").max_retries(0)).unwrap();
    let url = format!("http://{address}/upload?upload_id=TEST_SECRET_TOKEN");
    let error = c
        .raw(Method::POST, &url, &[], &[], vec![])
        .await
        .unwrap_err();
    assert!(!format!("{error}").contains("TEST_SECRET_TOKEN"));
    assert!(!format!("{error:?}").contains("TEST_SECRET_TOKEN"));
    let error = c
        .get::<Value>("/files", &[("signed".into(), "TEST_SECRET_QUERY".into())])
        .await
        .unwrap_err();
    assert!(!format!("{error:?}").contains("TEST_SECRET_QUERY"));
}

#[test]
fn ipv6_loopback_is_allowed_and_private_http_is_rejected() {
    assert!(Client::new(Config::new("http://[::1]:8080/v1", "key")).is_ok());
    assert!(Client::new(Config::new("http://192.168.1.1/v1", "key")).is_err());
}

#[tokio::test]
async fn downloads_follow_redirects_without_cross_origin_credentials() {
    let a = MockServer::start().await;
    let b = MockServer::start().await;
    let c = Client::new(
        Config::new(a.uri(), "secret")
            .header("x-private-header", "private")
            .unwrap(),
    )
    .unwrap();
    Mock::given(path("/start"))
        .and(header("authorization", "Bearer secret"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/signed?token=opaque", b.uri())),
        )
        .expect(1)
        .mount(&a)
        .await;
    Mock::given(path("/signed"))
        .and(query_param("token", "opaque"))
        .respond_with(
            ResponseTemplate::new(307).insert_header("location", format!("{}/finish", a.uri())),
        )
        .expect(1)
        .mount(&b)
        .await;
    Mock::given(path("/finish"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1, 0, 255]))
        .expect(1)
        .mount(&a)
        .await;
    assert_eq!(c.download("start").await.unwrap(), vec![1, 0, 255]);
    let external = b.received_requests().await.unwrap();
    assert!(!external[0].headers.contains_key("authorization"));
    assert!(!external[0].headers.contains_key("x-private-header"));
    let final_requests = a.received_requests().await.unwrap();
    assert!(!final_requests[1].headers.contains_key("authorization"));
    assert!(c.download(&format!("{}/signed", b.uri())).await.is_err());
}

#[tokio::test]
async fn downloads_bound_redirects_and_reject_insecure_target() {
    let s = MockServer::start().await;
    Mock::given(path("/loop"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/loop"))
        .expect(6)
        .mount(&s)
        .await;
    assert!(matches!(
        client(&s).download("/loop").await,
        Err(Error::Protocol(_))
    ));
    Mock::given(path("/insecure"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", "http://example.com/file"),
        )
        .expect(1)
        .mount(&s)
        .await;
    assert!(matches!(
        client(&s).download("/insecure").await,
        Err(Error::Protocol(_))
    ));
}
