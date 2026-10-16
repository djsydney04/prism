//! Explicit opt-in tests. Generation may incur provider charges.
use sydney_openai::{Client, ResponseRequest, Value};

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and network access"]
async fn live_model_discovery() {
    let models: Value = Client::from_env()
        .unwrap()
        .models()
        .list(&[])
        .await
        .unwrap();
    assert!(models["data"].is_array());
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and OPENAI_MODEL; incurs API usage"]
async fn live_response_generation() {
    let client = Client::from_env().unwrap();
    let model = std::env::var("OPENAI_MODEL").expect("set OPENAI_MODEL");
    let response = client
        .responses()
        .create(&ResponseRequest::new(model, "Reply with the word hello."))
        .await
        .unwrap();
    assert!(!response.id.is_empty());
    assert!(!response.output_text().is_empty());
}
