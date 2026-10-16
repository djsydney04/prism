//! Explicit opt-in tests. Generation may incur provider charges.
use sydney_groq::{ChatRequest, Client, Message};

#[tokio::test]
#[ignore = "requires GROQ_API_KEY and network access"]
async fn live_model_discovery() {
    let models = Client::from_env().unwrap().models().list().await.unwrap();
    assert!(!models.data.is_empty());
}

#[tokio::test]
#[ignore = "requires GROQ_API_KEY and GROQ_MODEL; incurs API usage"]
async fn live_chat_generation() {
    let client = Client::from_env().unwrap();
    let model = std::env::var("GROQ_MODEL").expect("set GROQ_MODEL");
    let response = client
        .chat()
        .create(&ChatRequest::new(
            model,
            vec![Message::user("Reply with the word hello.")],
        ))
        .await
        .unwrap();
    assert!(!response.choices.is_empty());
}
