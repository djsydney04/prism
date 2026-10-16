use sydney_gemini::{Client, GenerateContentRequest};

#[tokio::main]
async fn main() -> sydney_gemini::Result<()> {
    let client = Client::from_env()?;
    let model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-flash-latest".into());
    let response = client.models().generate_content(&model, &GenerateContentRequest::text("Explain Rust ownership in one paragraph.")).await?;
    println!("{}", response.text());
    Ok(())
}
