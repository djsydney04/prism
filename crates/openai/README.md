# prism-openai

An independent, unofficial asynchronous Rust SDK for OpenAI. Written against
official HTTP documentation; no third-party SDK implementation is vendored.
Rust 1.88 or newer is required.

Common generation APIs have extensible typed inputs and outputs. Advanced APIs
use an owned `Resource` handle that supports JSON, pagination queries, multipart
uploads, actions, and streaming. Responses, Realtime, and Live also support
WebSocket connections. Unknown fields and event types are retained.

## Install

```toml
[dependencies]
prism-openai = "0.2.2" # x-release-please-version
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
futures-util = "0.3"
```

Import the crate in Rust as `prism_openai`.

Set `OPENAI_API_KEY` and `OPENAI_MODEL` in your environment. API calls may incur
provider charges. No API key is required for the mock test suite.

```rust,no_run
use prism_openai::{Client, ResponseRequest};

#[tokio::main]
async fn main() -> prism_openai::Result<()> {
    let client = Client::from_env()?;
    let model = std::env::var("OPENAI_MODEL").expect("set OPENAI_MODEL");
    let request = ResponseRequest::new(model, "Explain Rust ownership in one sentence.");
    println!("{}", client.responses().create(&request).await?.output_text());
    Ok(())
}
```

Model names are caller-provided strings. Multimodal input, tools, structured
outputs, reasoning configuration, stateful conversations, background execution,
and new parameters can use `serde_json::json!` values or `with_extra`:

```rust,no_run
use prism_openai::{Client, ResponseRequest, serde_json::json};

# async fn example(client: &Client, model: String) -> prism_openai::Result<()> {
let request = ResponseRequest::new(model, json!([
    {"role":"user","content":[{"type":"input_text","text":"Find the answer"}]}
]))
.with_extra("reasoning", json!({"effort":"low"}))
.with_extra("text", json!({"format":{"type":"json_object"}}));
let response = client.responses().create(&request).await?;
# Ok(())
# }
```

Use dedicated struct fields for known keys; extension keys must not duplicate
dedicated fields. These types retain protocol data, rather than running model
tools or following generated instructions.

## Streaming

```rust,no_run
use futures_util::StreamExt;
use prism_openai::{Client, ResponseRequest, serde_json::Value};

# async fn example(client: &Client, model: String) -> prism_openai::Result<()> {
let mut stream = client.responses().stream(&ResponseRequest::new(model, "Hello")).await?;
while let Some(event) = stream.next().await {
    let event = event?;
    let data: Value = event.json()?;
    if data["type"] == "response.output_text.delta" {
        if let Some(delta) = data["delta"].as_str() { print!("{delta}"); }
    }
}
# Ok(())
# }
```

The stream preserves SSE event names, IDs, and JSON payloads; `[DONE]` terminates
OpenAI chat streams. Unknown event types are available to the caller. Drop the
stream to stop reading. Streaming methods force `stream=true`; typed non-stream
JSON methods force `stream=false` when the operation supports both modes.
Multipart streaming forms must explicitly include `.text("stream", "true")`.

## Persistent WebSocket sessions

`responses().connect()` opens a Responses WebSocket. Send a `response.create`
event with the same model/input fields as HTTP, omitting HTTP-only `stream` and
`background` parameters. Handle lifecycle and error events explicitly:

```rust,no_run
use prism_openai::{Client, serde_json::{json, Value}};

# async fn example(client: &Client, model: String) -> prism_openai::Result<()> {
let mut session = client.responses().connect().await?;
session.send_json(&json!({"type":"response.create","model":model,"input":"Hello"})).await?;
while let Some(event) = session.recv_json::<Value>().await? {
    if event["type"] == "response.completed" { break; }
    if event["type"] == "error" || event["type"] == "response.failed" {
        return Err(prism_openai::Error::Protocol("response failed".into()));
    }
}
session.close().await?;
# Ok(())
# }
```

`realtime_connect(model)` connects to Realtime. `live_connect(&[])` connects to
Live; send `session.start` with your model and configuration, and wait for
`session.started` before sending audio. `websocket(path, query)` covers sidebands,
forks, and other documented socket endpoints on the configured API origin.
`split()` supplies independent sender/receiver handles for concurrent audio I/O.
JSON event contents remain open to support tool events, new fields, and Responses
`stream_id` multiplexing. Callers own model event handling and reconnection policy;
application messages are never silently replayed.

## Chat, embeddings, images, audio, files, and moderation

The typed entry points are `responses()`, `chat()`, `embeddings()`,
`images()`, `audio()`, `files()`, and `moderations()`:

```rust,no_run
use prism_openai::{Client, ChatCompletionRequest, ChatMessage, EmbeddingRequest};

# async fn example(client: &Client, chat_model: String, embedding_model: String) -> prism_openai::Result<()> {
let request = ChatCompletionRequest::new(chat_model, vec![ChatMessage::user("Hello")]);
let completion = client.chat().create(&request).await?;
let embeddings = client.embeddings().create(&EmbeddingRequest::new(embedding_model, "Hello")).await?;
let file = client.files().upload("input.jsonl", b"{}\n".to_vec(), "batch").await?;
let bytes = client.files().content(&file.id).await?;
# Ok(())
# }
```

`audio().speech` returns encoded bytes. Transcription and translation support
simple uploads, fully custom multipart forms, JSON responses, and raw bytes for
text/SRT/VTT formats. `audio().transcribe_stream` and `images().edit_stream`
accept custom multipart forms for SSE. Image JSON edits are also available.

## Every REST surface and pagination

Resource handles can select any nested endpoint. `child` encodes each dynamic
ID as one path segment; use it instead of interpolating IDs into strings.
`create` and `update` issue POST; `patch`, `delete`, `get`, and `request`
support the other HTTP operations. Return any deserializable type, including
`Value` when a schema has new fields.

```rust,no_run
use prism_openai::{Client, List, serde_json::{Value, json}};

# async fn example(client: &Client) -> prism_openai::Result<()> {
let result: Value = client.vector_stores().child("vs_123").action("search")
    .create(&json!({"query":"Rust ownership","max_num_results":5})).await?;

let batch: Value = client.batches().create(&json!({
    "input_file_id":"file_123", "endpoint":"/v1/responses", "completion_window":"24h"
})).await?;

let mut query = vec![("limit".into(), "20".into())];
loop {
    let page: List<Value> = client.files().resource().get(&query).await?;
    // Process page.data here.
    let Some(after) = page.next_after()? else { break };
    query.retain(|(key, _)| key != "after");
    query.push(("after".into(), after.into()));
}
# Ok(())
# }
```

Pagination remains explicit so callers control request volume and persist their
own cursors. Resources that use offset pagination or different cursor fields can
use the raw JSON result and documented query keys.

## Configuration and errors

```rust,no_run
use prism_openai::{Client, Config, BASE_URL};

# fn example(key: String) -> prism_openai::Result<()> {
let config = Config::new(BASE_URL, key)
    .header("OpenAI-Organization", "org_example")?
    .header("OpenAI-Project", "proj_example")?;
let client = Client::with_config(config)?;
# Ok(())
# }
```

Use a separate client with an Admin API key for `organization()` endpoints.
Optional beta APIs may require documented `OpenAI-Beta` headers. Custom API
roots can target trusted gateways or local mock servers. Relative resource paths
preserve the configured API prefix; a leading slash selects the origin root.

`Error::Api` includes status, request ID when available, and parsed provider error
data. The shared core validates endpoint origins and disables redirects to help
keep credentials on the configured server. Credentials are redacted from client
debug output. Automatic retries follow the shared core's idempotency rules;
uploads and generation POSTs are not silently repeated.

## Coverage and validation

See [the detailed coverage matrix](../../docs/openai.md) for typed and generic
REST support, exact limitations, and official sources. The included contract
tests run against local mock HTTP servers and verify request routing, headers,
serialization, multimodal/tool data, streams, multipart uploads, binary formats,
pagination, advanced resources, and error preservation. They do not establish
live account permissions or every provider-side schema combination.

Verify incoming webhooks before parsing their raw body:

```rust,no_run
use prism_openai::webhooks::{WebhookHeaders, WebhookVerifier};
# fn handle(raw_body: &[u8], headers: &prism_openai::reqwest::header::HeaderMap)
# -> Result<(), Box<dyn std::error::Error>> {
let secret = std::env::var("OPENAI_WEBHOOK_SECRET")?;
let verifier = WebhookVerifier::new(&secret)?;
let verified = verifier.verify(raw_body, WebhookHeaders::from_headers(headers)?)?;
let event: prism_openai::Value = verified.json()?;
// Deduplicate verified.id() in durable storage before processing event.
# let _ = event;
# Ok(()) }
```

This crate does not provide a WebRTC media engine, execute generated tool calls,
or make model output trustworthy.
Applications must treat model and retrieved content as untrusted input and
authorize tool execution themselves.
