# prism-groq

Independent, unofficial async Rust SDK for the Groq REST API. No third-party
Groq SDK is wrapped or copied. Not affiliated with Groq.

## Install

```toml
[dependencies]
prism-groq = "0.2.1" # x-release-please-version
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
futures-util = "0.3"
```

Import the crate in Rust as `prism_groq`.

Set `GROQ_API_KEY` in your environment. Never embed it in source control.

```rust,no_run
use prism_groq::{Client, ChatRequest, Message};

#[tokio::main]
async fn main() -> prism_groq::Result<()> {
    let client = Client::from_env()?;
    let mut request = ChatRequest::new(
        "llama-3.3-70b-versatile",
        vec![Message::system("Answer concisely."), Message::user("What is Rust?")],
    );
    request.max_completion_tokens = Some(256);
    let completion = client.chat().create(&request).await?;
    println!("{:?}", completion.choices[0].message.content);
    Ok(())
}
```

## Streaming

```rust,no_run
# async fn example(client: &prism_groq::Client, request: &prism_groq::ChatRequest) -> prism_groq::Result<()> {
use futures_util::StreamExt;
use prism_groq::ChatChunk;
let mut stream = client.chat().stream(request).await?;
while let Some(event) = stream.next().await {
    let chunk: ChatChunk = event?.json()?;
    for choice in chunk.choices {
        println!("{:?}", choice.delta.content);
    }
}
# Ok(()) }
```

The transport handles SSE framing and `[DONE]`. Drop the stream to cancel.
Tool-call arguments are fragments: join them by tool-call index before parsing.
Responses streaming preserves the provider's event names and raw JSON data.

## Tools, vision, and structured output

```rust,no_run
# fn example() {
use prism_groq::{serde_json::json, ChatRequest, ContentPart, Message, ResponseFormat, Tool};
let mut request = ChatRequest::new("your-vision-model", vec![Message::user(vec![
    ContentPart::text("Describe this image."),
    ContentPart::image("https://example.com/image.png"),
])]);
request.tools = Some(vec![Tool::function("lookup", json!({
    "type": "object", "properties": {"id": {"type": "string"}},
    "required": ["id"], "additionalProperties": false
}))]);
request.response_format = Some(ResponseFormat::json_schema(
    "answer", json!({"type": "object", "properties": {"text": {"type":"string"}},
                     "required": ["text"], "additionalProperties": false}), true,
));
# }
```

Choose a model supporting the requested capabilities. Returned tool calls are
data; the SDK never executes functions, visits image URLs, or runs remote code.
An SDK cannot make model prompts immune to prompt injection. Applications must
validate tool arguments and enforce authorization before acting on model output.

## Audio

```rust,no_run
# async fn example(client: &prism_groq::Client) -> Result<(), Box<dyn std::error::Error>> {
use prism_groq::{AudioRequest, SpeechRequest};
let request = AudioRequest::file("whisper-large-v3", "sample.wav", std::fs::read("sample.wav")?);
let transcript = client.audio().transcribe(request).await?;
println!("{}", transcript.text);

let translation = client.audio().translate(
    AudioRequest::url("whisper-large-v3", "https://example.com/audio.wav")
).await?;
println!("{}", translation.text);

let mut speech = SpeechRequest::new("your-tts-model", "Hello!", "your-voice");
speech.response_format = Some("wav".into());
std::fs::write("speech.wav", client.audio().speech(&speech).await?)?;
# Ok(()) }
```

`AudioRequest` exposes language, prompt, format, temperature, timestamp
granularities, and extra multipart fields. Use `transcribe_bytes` or
`translate_bytes` for `response_format = "text"`; the typed methods expect JSON.
Audio files are accepted as bytes, leaving file access and memory management to
the caller.

## Responses, files, batches, models, and fine tuning

```rust,no_run
# async fn example(client: &prism_groq::Client) -> prism_groq::Result<()> {
use prism_groq::{BatchRequest, ListOptions, ResponseRequest};
let response = client.responses().create(
    &ResponseRequest::new("openai/gpt-oss-120b", "Write a haiku.")
).await?;
println!("{}", response.output_text());
let models = client.models().list().await?;
println!("{} models", models.data.len());

let input = br#"{"custom_id":"one","method":"POST","url":"/v1/chat/completions","body":{"model":"llama-3.3-70b-versatile","messages":[{"role":"user","content":"Hi"}]}}
"#;
let file = client.files().upload("requests.jsonl", input.to_vec()).await?;
let batch = client.batches().create(&BatchRequest::new(file.id, "/v1/chat/completions")).await?;
println!("{}: {}", batch.id, batch.status);
let page = client.batches().list(&ListOptions::default()).await?;
println!("{} batches", page.data.len());
# Ok(()) }
```

Other operations include model retrieval; file listing, retrieval, download,
deletion; batch retrieval and cancellation; and closed-beta fine tuning listing,
creation, retrieval, and deletion. Responses is beta and supports create and
streaming; Groq does not document response storage or retrieval here.

Lists return pagination metadata. Continue with `ListOptions.after` using
`last_id` where available. No background polling or automatic tool execution is
started by the library.

## Configuration, errors, and API evolution

```rust,no_run
# fn example() -> prism_groq::Result<()> {
use prism_groq::{Client, Config};
let config = Config::new("https://api.groq.com/", "your-key");
let client = Client::with_config(config)?;
# let _ = client;
# Ok(()) }
```

Use an origin base URL, without `openai/v1`. The client adds `openai/v1` for
inference and `v1` for fine tuning. A proxy prefix can be supplied as the base
path. Core configuration exposes timeouts, read retry limits, response size
limits, and additional headers. API keys are redacted from debug formatting.
Redirects are disabled and credentials are never placed in query parameters.
Creates and uploads are not retried automatically.

Errors distinguish configuration, transport, JSON serialization, protocol, and
provider HTTP failures. `Error::Api` retains status, request ID, message, and
the original response body.

All model names and provider enum-like fields accept strings. Request/response
`extra` maps retain unknown fields. Polymorphic API objects use JSON values.
For new endpoints or custom wire models, use `client.resource(path)` or
`client.http()`, plus `chat().create_with` / `responses().create_with`.
These paths are relative to the configured origin, for example
`client.resource("openai/v1/models")`.

## Validation

```sh
cargo test -p prism-groq
cargo test -p prism-groq --doc
cargo run -p prism-groq --example chat
```

Mock-server tests cover every documented route, request serialization,
authentication, multipart uploads, text/binary audio, SSE events, pagination,
unknown fields, and provider errors. They do not contact Groq or incur charges.
The example requires a real API key and makes a billable chat request.

API coverage was checked against the [official reference](https://console.groq.com/docs/api-reference)
and [batch documentation](https://console.groq.com/docs/batch) on 2026-10-15.
