# sydney-gemini

Independent, unofficial asynchronous Rust bindings for Google's Gemini Developer API. Built from the official REST reference, with no dependency on another provider SDK. This crate is not affiliated with Google.

```toml
[dependencies]
sydney-gemini = "0.1.0"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
# Add this for streaming examples:
futures-util = "0.3"
```

Set `GEMINI_API_KEY`, then:

```rust,no_run
use sydney_gemini::{Client, GenerateContentRequest};

#[tokio::main]
async fn main() -> sydney_gemini::Result<()> {
    let client = Client::from_env()?;
    let response = client.models().generate_content(
        "gemini-flash-latest",
        &GenerateContentRequest::text("Explain Rust ownership in one paragraph."),
    ).await?;
    println!("{}", response.text());
    Ok(())
}
```

Model names are strings so new provider models do not require a crate release. Both `model-name` and `models/model-name` are accepted. Availability depends on the Google project and model.

## Streaming

```rust,no_run
use futures_util::StreamExt;
use sydney_gemini::{Client, GenerateContentRequest, GenerateContentResponse};

#[tokio::main]
async fn main() -> sydney_gemini::Result<()> {
    let client = Client::from_env()?;
    let mut stream = client.models().stream_generate_content(
        "gemini-flash-latest", &GenerateContentRequest::text("Tell a short story."),
    ).await?;
    while let Some(event) = stream.next().await {
        let chunk: GenerateContentResponse = event?.json()?;
        print!("{}", chunk.text());
    }
    Ok(())
}
```

Streams yield parsed SSE envelopes with data, event type, ID, and retry information. Errors during the stream are `Err` items. Streams are not automatically replayed.

## Stateful interactions

```rust,no_run
use sydney_gemini::{json, Client, InteractionRequest};

#[tokio::main]
async fn main() -> sydney_gemini::Result<()> {
    let client = Client::from_env()?;
    let first = client.interactions().create(&InteractionRequest {
        model: Some("gemini-flash-latest".into()),
        input: json!("Remember that my project is called Cedar."),
        ..InteractionRequest::default()
    }).await?;
    let next = client.interactions().create(&json!({
        "model": "gemini-flash-latest",
        "previous_interaction_id": first.id,
        "input": "What is my project called?"
    })).await?;
    println!("{}", next.text());
    Ok(())
}
```

`interactions().stream` enables `stream: true`; `resume_stream` accepts `last_event_id`. Interactions use snake_case JSON; `generateContent` uses camelCase. Preserve returned thought signatures and tool-call IDs when constructing subsequent turns. The SDK returns tool calls to your application and never executes them.

## Files and media

```rust,no_run
use sydney_gemini::{json, Client};

#[tokio::main]
async fn main() -> sydney_gemini::Result<()> {
    let client = Client::from_env()?;
    let file = client.files().upload(
        &json!({"file": {"displayName": "Project notes"}}),
        "text/plain", b"Cedar is a Rust project.".to_vec(),
    ).await?.file;
    let response = client.models().generate_content("gemini-flash-latest", &json!({
        "contents": [{"role": "user", "parts": [
            {"text": "Summarize these notes"},
            {"fileData": {"mimeType": "text/plain", "fileUri": file.uri}}
        ]}]
    })).await?;
    println!("{}", response.text());
    client.files().delete(&file.name).await?;
    Ok(())
}
```

`upload` holds supplied bytes in memory. For large files, use `start_upload`, `upload_chunk`, and `query_upload` with application-managed chunk buffers and offsets. A session URL may be restored with `UploadSession::from_url`; store it securely. File processing may be asynchronous: poll `files().get` until the provider reports `ACTIVE` before using media that requires processing.

`files().download(uri)` retrieves provider-generated media. The transport requires the URI to have the configured origin and does not follow redirects. Files uploaded by users generally cannot be downloaded. A redirect or foreign download host requires an application-controlled, unauthenticated download flow; do not forward your API key.

## Embeddings, batches, and file search

```rust,no_run
use sydney_gemini::{json, Client, Content, EmbedContentRequest};

#[tokio::main]
async fn main() -> sydney_gemini::Result<()> {
    let client = Client::from_env()?;
    let embedding = client.models().embed_content("gemini-embedding-001", &EmbedContentRequest {
        content: Content::user("A document about Rust"),
        task_type: Some("RETRIEVAL_DOCUMENT".into()),
        ..EmbedContentRequest::default()
    }).await?;
    println!("{} dimensions", embedding.embedding.values.len());

    let batch = client.models().batch_generate_content("gemini-flash-latest", &json!({
        "batch": {"displayName": "Summaries", "inputConfig": {"requests": {"requests": [
            {"request": {"contents": [{"parts": [{"text": "Summarize Rust"}]}]}, "metadata": {"key": "one"}}
        ]}}}
    })).await?;
    println!("Batch: {}", batch.name);
    let operation = client.batches().get(&batch.name).await?;
    println!("Completed: {}", operation.done);
    Ok(())
}
```

Cache updates accept `updateMask`. File search store/document deletion exposes the explicit `force` flag. List methods accept query pairs and return the provider's next-page token. Callers control pagination, polling cadence, and cancellation; the client does not create background tasks.

## Configuration and schema extensions

```rust,no_run
use std::time::Duration;
use sydney_gemini::{json, Client, Config, Value, DEFAULT_BASE_URL};

#[tokio::main]
async fn main() -> sydney_gemini::Result<()> {
    let config = Config::from_env(DEFAULT_BASE_URL, "GEMINI_API_KEY")?
        .timeout(Duration::from_secs(180))
        .max_retries(2)
        .max_response_bytes(128 * 1024 * 1024);
    let client = Client::with_config(config)?;
    let result: Value = client.resource("/v1beta/models/gemini-flash-latest:generateContent")
        .create(&json!({"contents": [{"parts": [{"text": "Hi"}]}]})).await?;
    println!("{result}");
    Ok(())
}
```

All body-taking methods accept any `Serialize` value. Typed common schemas retain new fields in `extra`; use `serde_json::Value` for less common schemas. The generic `resource` and `transport` APIs support new or legacy REST endpoints. No dynamic model catalog or exhaustive schema enum can block a newly introduced model or field.

Authentication is always the `x-goog-api-key` header. HTTP is allowed only for loopback mock servers. The core blocks cross-origin requests, disables redirects, bounds buffered responses and SSE events, and retries only eligible read requests. Inference and upload writes are not automatically repeated. Provider errors preserve status, message, request ID, and original JSON.

## Coverage and validation

The [coverage matrix](https://github.com/djsydney04/rust_sdks/blob/main/docs/gemini.md) lists named methods and limitations. This release targets the Gemini Developer API, not Vertex AI. Live WebSocket audio/video, OAuth, automatic pagination, and webhook signature verification are not implemented.

Offline contract tests exercise authentication, body encoding, API errors, SSE, query parameters, pagination metadata, binary upload/download, foreign-origin rejection, resource validation, and named resource routes. They require no credentials and incur no API charges. Passing mocks does not establish live account access, model availability, quotas, or feature-specific schema compatibility.

```sh
cargo test -p sydney-gemini --all-targets
cargo test -p sydney-gemini --doc
cargo run -p sydney-gemini --example generate
```

References: [Gemini API](https://ai.google.dev/api), [REST methods](https://ai.google.dev/api/all-methods), [content generation](https://ai.google.dev/api/generate-content), [interactions](https://ai.google.dev/api/interactions-api), [files](https://ai.google.dev/api/files), and [file search](https://ai.google.dev/api/file-search/file-search-stores).
