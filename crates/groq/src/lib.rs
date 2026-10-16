//! An independently implemented, unofficial async Groq REST SDK.
//!
//! Use [`Client::from_env`] to read `GROQ_API_KEY`. All model names are strings,
//! and unknown JSON fields remain available through `extra` maps.
//! Remote tool calls are returned as data; this crate does not execute tools.
//!
//! ```no_run
//! # async fn example() -> prism_groq::Result<()> {
//! use prism_groq::{Client, ChatRequest, Message};
//! let client = Client::from_env()?;
//! let request = ChatRequest::new("llama-3.3-70b-versatile", vec![Message::user("Hi")]);
//! let response = client.chat().create(&request).await?;
//! println!("{:?}", response.choices[0].message.content);
//! # Ok(()) }
//! ```

mod types;
pub use types::*;

use prism_ai_core::Client as HttpClient;
pub use prism_ai_core::{reqwest, Config, Error, EventStream, Method, Resource, Result};
use serde::{de::DeserializeOwned, Serialize};
pub use serde_json::{self, Value};

/// Origin used by the public Groq API.
pub const BASE_URL: &str = "https://api.groq.com/";

/// Cloneable async Groq client backed by a shared HTTP connection pool.
#[derive(Clone, Debug)]
pub struct Client {
    http: HttpClient,
}

impl Client {
    /// Create a client with an explicit API key.
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Self::with_config(Config::new(BASE_URL, api_key))
    }

    /// Read the API key from `GROQ_API_KEY`.
    pub fn from_env() -> Result<Self> {
        Self::with_config(Config::from_env(BASE_URL, "GROQ_API_KEY")?)
    }

    /// Configure timeouts, retries, headers, and a custom API origin.
    ///
    /// The configured base URL is the origin (or a proxy prefix), without
    /// `openai/v1`; the client adds the appropriate endpoint prefix.
    pub fn with_config(config: Config) -> Result<Self> {
        Ok(Self {
            http: HttpClient::new(config)?,
        })
    }

    /// Shared transport for advanced requests.
    pub fn http(&self) -> &HttpClient {
        &self.http
    }
    /// Generic resource access relative to the configured base URL.
    pub fn resource(&self, path: &str) -> Resource {
        self.http.resource(path)
    }
    /// Chat completions, including tools, vision, and SSE streaming.
    pub fn chat(&self) -> Chat<'_> {
        Chat(&self.http)
    }
    /// Responses beta endpoint.
    pub fn responses(&self) -> Responses<'_> {
        Responses(&self.http)
    }
    /// Transcription, translation, and speech synthesis.
    pub fn audio(&self) -> Audio<'_> {
        Audio(&self.http)
    }
    /// Model discovery.
    pub fn models(&self) -> Models<'_> {
        Models(&self.http)
    }
    /// Batch input and output files.
    pub fn files(&self) -> Files<'_> {
        Files(&self.http)
    }
    /// Asynchronous batch inference.
    pub fn batches(&self) -> Batches<'_> {
        Batches(&self.http)
    }
    /// Fine tuning metadata management (closed beta; requires provider access).
    pub fn fine_tunings(&self) -> FineTunings<'_> {
        FineTunings(&self.http)
    }
}

/// Chat completions resource.
pub struct Chat<'a>(&'a HttpClient);
impl Chat<'_> {
    /// Create a completion. Forces non-streaming output regardless of `stream`.
    pub async fn create(&self, request: &ChatRequest) -> Result<ChatCompletion> {
        let mut body = serde_json::to_value(request)?;
        body["stream"] = Value::Bool(false);
        self.0.post("openai/v1/chat/completions", &body).await
    }
    /// Start SSE output; forces `stream=true` on a copy of the request.
    /// Deserialize event data as [`ChatChunk`]. Tool-call arguments may arrive
    /// in partial JSON fragments and should be joined before parsing.
    pub async fn stream(&self, request: &ChatRequest) -> Result<EventStream> {
        let mut body = serde_json::to_value(request)?;
        body["stream"] = Value::Bool(true);
        self.0
            .stream(Method::POST, "openai/v1/chat/completions", &[], Some(&body))
            .await
    }
    /// Send an arbitrary serializable body and deserialize the response.
    pub async fn create_with<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        body: &B,
    ) -> Result<T> {
        self.0.post("openai/v1/chat/completions", body).await
    }
}

/// Responses beta resource. Only create is documented by Groq.
pub struct Responses<'a>(&'a HttpClient);
impl Responses<'_> {
    /// Create a response with typed common fields and extensible output items.
    pub async fn create(&self, request: &ResponseRequest) -> Result<Response> {
        let mut body = serde_json::to_value(request)?;
        body["stream"] = Value::Bool(false);
        self.0.post("openai/v1/responses", &body).await
    }
    /// Create a response as SSE events.
    pub async fn stream(&self, request: &ResponseRequest) -> Result<EventStream> {
        let mut body = serde_json::to_value(request)?;
        body["stream"] = Value::Bool(true);
        self.0
            .stream(Method::POST, "openai/v1/responses", &[], Some(&body))
            .await
    }
    /// Send arbitrary JSON-compatible request and response types.
    pub async fn create_with<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        body: &B,
    ) -> Result<T> {
        self.0.post("openai/v1/responses", body).await
    }
}

/// Model discovery resource.
pub struct Models<'a>(&'a HttpClient);
impl Models<'_> {
    /// List models currently available to the API key.
    pub async fn list(&self) -> Result<List<Model>> {
        self.0.get("openai/v1/models", &[]).await
    }
    /// Retrieve a model; slashes and reserved characters in IDs are encoded.
    pub async fn retrieve(&self, id: &str) -> Result<Model> {
        self.0.get(&item_path("openai/v1/models", id), &[]).await
    }
}

/// Multipart file uploads and binary downloads.
pub struct Files<'a>(&'a HttpClient);
impl Files<'_> {
    /// Upload bytes with the provider's `batch` purpose.
    pub async fn upload(&self, filename: impl Into<String>, bytes: Vec<u8>) -> Result<File> {
        self.upload_for(filename, bytes, "batch").await
    }
    /// Upload with an explicit purpose for future-compatible API support.
    pub async fn upload_for(
        &self,
        filename: impl Into<String>,
        bytes: Vec<u8>,
        purpose: impl Into<String>,
    ) -> Result<File> {
        let form = reqwest::multipart::Form::new()
            .text("purpose", purpose.into())
            .part(
                "file",
                reqwest::multipart::Part::bytes(bytes).file_name(filename.into()),
            );
        self.0
            .multipart(Method::POST, "openai/v1/files", &[], form)
            .await
    }
    /// List uploaded files.
    pub async fn list(&self) -> Result<List<File>> {
        self.list_with(&[]).await
    }
    /// List files with optional query parameters supported by the service.
    pub async fn list_with(&self, query: &[(String, String)]) -> Result<List<File>> {
        self.0.get("openai/v1/files", query).await
    }
    /// Retrieve metadata for an uploaded file.
    pub async fn retrieve(&self, id: &str) -> Result<File> {
        self.0.get(&item_path("openai/v1/files", id), &[]).await
    }
    /// Delete an uploaded file.
    pub async fn delete(&self, id: &str) -> Result<Deletion> {
        self.0.delete(&item_path("openai/v1/files", id)).await
    }
    /// Download the original bytes, including JSONL batch results.
    pub async fn content(&self, id: &str) -> Result<Vec<u8>> {
        self.0
            .bytes(
                Method::GET,
                &format!("{}/content", item_path("openai/v1/files", id)),
                &[],
                None,
            )
            .await
    }
}

/// Batch job resource.
pub struct Batches<'a>(&'a HttpClient);
impl Batches<'_> {
    /// Create an asynchronous inference batch from an uploaded JSONL file.
    pub async fn create(&self, request: &BatchRequest) -> Result<Batch> {
        self.0.post("openai/v1/batches", request).await
    }
    /// Retrieve the latest state of a batch.
    pub async fn retrieve(&self, id: &str) -> Result<Batch> {
        self.0.get(&item_path("openai/v1/batches", id), &[]).await
    }
    /// List batches, optionally continuing from `after`.
    pub async fn list(&self, options: &ListOptions) -> Result<List<Batch>> {
        self.0.get("openai/v1/batches", &options.query()).await
    }
    /// Request cancellation of a batch.
    pub async fn cancel(&self, id: &str) -> Result<Batch> {
        self.0
            .post(
                &format!("{}/cancel", item_path("openai/v1/batches", id)),
                &serde_json::json!({}),
            )
            .await
    }
}

/// Closed-beta fine tuning metadata resource.
pub struct FineTunings<'a>(&'a HttpClient);
impl FineTunings<'_> {
    /// List previously uploaded fine tunes; provider beta access is required.
    pub async fn list(&self) -> Result<List<FineTuning>> {
        self.0.get("v1/fine_tunings", &[]).await
    }
    /// Register a fine tuning from an uploaded file.
    pub async fn create(&self, request: &FineTuningRequest) -> Result<FineTuningResponse> {
        self.0.post("v1/fine_tunings", request).await
    }
    /// Retrieve a registered fine tuning.
    pub async fn retrieve(&self, id: &str) -> Result<FineTuningResponse> {
        self.0.get(&item_path("v1/fine_tunings", id), &[]).await
    }
    /// Delete a registered fine tuning.
    pub async fn delete(&self, id: &str) -> Result<Deletion> {
        self.0.delete(&item_path("v1/fine_tunings", id)).await
    }
}

/// Audio endpoint collection.
pub struct Audio<'a>(&'a HttpClient);
impl Audio<'_> {
    /// Transcribe an uploaded file or a URL as JSON/verbose JSON.
    /// Set `response_format` to `json` or `verbose_json`; use
    /// [`Self::transcribe_bytes`] for `text` output.
    pub async fn transcribe(&self, request: AudioRequest) -> Result<Transcription> {
        let bytes = self.transcribe_bytes(request).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }
    /// Translate uploaded audio or a URL into English as JSON/verbose JSON.
    pub async fn translate(&self, request: AudioRequest) -> Result<Transcription> {
        let bytes = self.translate_bytes(request).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }
    /// Transcribe and return raw response bytes (including `text` format).
    pub async fn transcribe_bytes(&self, request: AudioRequest) -> Result<Vec<u8>> {
        self.0
            .multipart_bytes(
                Method::POST,
                "openai/v1/audio/transcriptions",
                &[],
                request.into_form()?,
            )
            .await
    }
    /// Translate and return raw response bytes (including `text` format).
    pub async fn translate_bytes(&self, request: AudioRequest) -> Result<Vec<u8>> {
        self.0
            .multipart_bytes(
                Method::POST,
                "openai/v1/audio/translations",
                &[],
                request.into_form()?,
            )
            .await
    }
    /// Generate audio bytes. Available formats and voices depend on the model.
    pub async fn speech(&self, request: &SpeechRequest) -> Result<Vec<u8>> {
        let body = serde_json::to_value(request)?;
        self.0
            .bytes(Method::POST, "openai/v1/audio/speech", &[], Some(&body))
            .await
    }
}

// Encode the entire untrusted identifier as one URL segment, never a path.
fn item_path(base: &str, id: &str) -> String {
    let mut path = String::from(base);
    path.push('/');
    for byte in id.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'~') {
            path.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(path, "%{byte:02X}").expect("writing to a String cannot fail");
        }
    }
    path
}
