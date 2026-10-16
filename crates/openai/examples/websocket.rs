use prism_openai::{
    serde_json::{json, Value},
    Client,
};

#[tokio::main]
async fn main() -> prism_openai::Result<()> {
    let client = Client::from_env()?;
    let model = std::env::var("OPENAI_MODEL")
        .expect("set OPENAI_MODEL to a model available to your account");
    let mut session = client.responses().connect().await?;
    session
        .send_json(&json!({
            "type":"response.create", "model":model, "input":"Explain ownership in one sentence."
        }))
        .await?;
    let mut completed = false;
    while let Some(event) = session.recv_json::<Value>().await? {
        match event["type"].as_str() {
            Some("response.output_text.delta") => {
                if let Some(delta) = event["delta"].as_str() {
                    print!("{delta}");
                }
            }
            Some("response.completed") => {
                println!();
                completed = true;
                break;
            }
            Some("error" | "response.failed" | "response.incomplete") => {
                return Err(prism_openai::Error::Protocol(
                    "response did not complete successfully".into(),
                ));
            }
            _ => {}
        }
    }
    session.close().await?;
    if !completed {
        return Err(prism_openai::Error::Protocol(
            "connection closed before the response completed".into(),
        ));
    }
    Ok(())
}
