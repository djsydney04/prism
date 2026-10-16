use futures_util::StreamExt;
use sydney_gemini::{Client, GenerateContentRequest, GenerateContentResponse};

#[tokio::main]
async fn main() -> sydney_gemini::Result<()> {
    let client = Client::from_env()?;
    let model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-flash-latest".into());
    let mut stream = client
        .models()
        .stream_generate_content(
            &model,
            &GenerateContentRequest::text("Tell a short story about a crab."),
        )
        .await?;
    while let Some(event) = stream.next().await {
        let chunk: GenerateContentResponse = event?.json()?;
        print!("{}", chunk.text());
    }
    println!();
    Ok(())
}
