use sydney_openai::{
    serde_json::{json, Value},
    BatchRequest, Client, List,
};

#[tokio::main]
async fn main() -> sydney_openai::Result<()> {
    let client = Client::from_env()?;
    // This read-only example lists models without creating a billable generation.
    let models: List<Value> = client.models().list(&[]).await?;
    println!("{} available models", models.data.len());

    // Build advanced operations as data; nothing is submitted until explicitly awaited.
    let _batch = BatchRequest::new("file_example", "/v1/responses");
    let _vector_search = client.vector_stores().child("vs_example").action("search");
    let _query = json!({"query":"Rust borrowing"});
    Ok(())
}
