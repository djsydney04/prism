use prism_groq::{ChatRequest, Client, Message};

#[tokio::main]
async fn main() -> prism_groq::Result<()> {
    let client = Client::from_env()?;
    let model = std::env::var("GROQ_MODEL").unwrap_or_else(|_| "llama-3.3-70b-versatile".into());
    let request = ChatRequest::new(
        model,
        vec![Message::user("Explain Rust ownership in one sentence.")],
    );
    let result = client.chat().create(&request).await?;
    for choice in result.choices {
        println!("{:?}", choice.message.content);
    }
    Ok(())
}
