use sydney_gemini::{json, Client, LiveServerMessage};

#[tokio::main]
async fn main() -> sydney_gemini::Result<()> {
    let client = Client::from_env()?;
    let model = std::env::var("GEMINI_LIVE_MODEL").unwrap_or_else(|_| "gemini-3.8-live".into());
    let mut session = client
        .live_connect(&json!({
            "model": format!("models/{model}"),
            "generationConfig": {"responseModalities": ["AUDIO"]},
            "outputAudioTranscription": {}
        }))
        .await?;
    while let Some(event) = session.recv_json::<LiveServerMessage>().await? {
        if event.setup_complete.is_some() {
            break;
        }
    }
    session
        .send_json(&json!({"clientContent": {
            "turns": [{"role":"user","parts":[{"text":"Say hello."}]}],
            "turnComplete": true
        }}))
        .await?;
    while let Some(event) = session.recv_json::<LiveServerMessage>().await? {
        if let Some(content) = event.server_content {
            if let Some(text) = content["outputTranscription"]["text"].as_str() {
                print!("{text}");
            }
            if content["turnComplete"] == true {
                break;
            }
        }
    }
    println!();
    session.close().await?;
    Ok(())
}
