use futures_util::StreamExt;
use prism_openai::{serde_json::Value, Client, ResponseRequest};

#[tokio::main]
async fn main() -> prism_openai::Result<()> {
    let client = Client::from_env()?;
    let model = std::env::var("OPENAI_MODEL")
        .expect("set OPENAI_MODEL to a model available to your account");
    let mut events = client
        .responses()
        .stream(&ResponseRequest::new(model, "Explain borrowing briefly."))
        .await?;
    while let Some(event) = events.next().await {
        let body: Value = event?.json()?;
        if body["type"] == "response.output_text.delta" {
            if let Some(delta) = body["delta"].as_str() {
                print!("{delta}");
            }
        }
    }
    println!();
    Ok(())
}
