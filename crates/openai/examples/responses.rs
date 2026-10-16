use sydney_openai::{Client, ResponseRequest};

#[tokio::main]
async fn main() -> sydney_openai::Result<()> {
    let client = Client::from_env()?;
    let model = std::env::var("OPENAI_MODEL")
        .expect("set OPENAI_MODEL to a model available to your account");
    let response = client
        .responses()
        .create(&ResponseRequest::new(model, "Say hello in one sentence."))
        .await?;
    println!("{}", response.output_text());
    Ok(())
}
