//! Explicit opt-in tests. Generation may incur provider charges.
use prism_gemini::{Client, GenerateContentRequest};

#[tokio::test]
#[ignore = "requires GEMINI_API_KEY and network access"]
async fn live_model_discovery() {
    let models = Client::from_env()
        .unwrap()
        .models()
        .list(&[])
        .await
        .unwrap();
    assert!(!models.models.is_empty());
}

#[tokio::test]
#[ignore = "requires GEMINI_API_KEY and GEMINI_MODEL; incurs API usage"]
async fn live_content_generation() {
    let client = Client::from_env().unwrap();
    let model = std::env::var("GEMINI_MODEL").expect("set GEMINI_MODEL");
    let response = client
        .models()
        .generate_content(
            &model,
            &GenerateContentRequest::text("Reply with the word hello."),
        )
        .await
        .unwrap();
    assert!(!response.text().is_empty());
}
