#![forbid(unsafe_code)]
//! Independent asynchronous bindings for the Gemini Developer REST API.
//!
//! This crate is unofficial and is not affiliated with Google. It does not execute
//! model tool calls. Common schemas are typed; every request also accepts any
//! serializable body, and unknown response fields are preserved.
//!
//! ```no_run
//! use sydney_gemini::{Client, GenerateContentRequest};
//! # async fn example() -> sydney_gemini::Result<()> {
//! let client = Client::from_env()?;
//! let response = client.models().generate_content(
//!     "gemini-flash-latest", &GenerateContentRequest::text("Hello!"),
//! ).await?;
//! println!("{}", response.text());
//! # Ok(()) }
//! ```

pub mod types;
mod resources;

pub use resources::*;
pub use sydney_ai_core::{Config, Error, EventStream, Method, RawResponse, Resource, Result};
pub use serde_json::{self, json, Value};
pub use types::*;

/// Default service origin. Endpoints use explicit `/v1beta/` paths.
pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";

/// A cheaply cloneable Gemini client. Keys travel in `x-goog-api-key` headers.
#[derive(Clone, Debug)]
pub struct Client {
    inner: sydney_ai_core::Client,
}

impl Client {
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Self::with_config(Config::new(DEFAULT_BASE_URL, api_key))
    }

    /// Read `GEMINI_API_KEY`. No key is placed in URLs or query strings.
    pub fn from_env() -> Result<Self> {
        Self::with_config(Config::from_env(DEFAULT_BASE_URL, "GEMINI_API_KEY")?)
    }

    /// Override transport settings and the service origin (for example, a mock server).
    pub fn with_config(config: Config) -> Result<Self> {
        Ok(Self { inner: sydney_ai_core::Client::new(config.with_api_key_header("x-goog-api-key")?)? })
    }

    pub fn models(&self) -> Models<'_> { Models(self) }
    pub fn files(&self) -> Files<'_> { Files(self) }
    pub fn cached_contents(&self) -> CachedContents<'_> { CachedContents(self) }
    pub fn batches(&self) -> Batches<'_> { Batches(self) }
    pub fn file_search_stores(&self) -> FileSearchStores<'_> { FileSearchStores(self) }
    pub fn interactions(&self) -> Interactions<'_> { Interactions(self) }
    pub fn operations(&self) -> Operations<'_> { Operations(self) }
    pub fn agents(&self) -> Agents<'_> { Agents(self) }
    pub fn webhooks(&self) -> Webhooks<'_> { Webhooks(self) }
    pub fn triggers(&self) -> Triggers<'_> { Triggers(self) }
    pub fn environments(&self) -> Environments<'_> { Environments(self) }
    pub fn voices(&self) -> Voices<'_> { Voices(self) }
    pub fn credentials(&self) -> Credentials<'_> { Credentials(self) }

    /// Create a short-lived token for a Live session. WebSocket transport is not included.
    pub async fn create_auth_token<B: serde::Serialize + ?Sized>(&self, body: &B) -> Result<Value> {
        self.inner.post("/v1beta/auth_tokens", body).await
    }

    /// Escape hatch for new/legacy REST resources; the transport validates paths.
    /// Example: `client.resource("/v1beta/tunedModels")`.
    pub fn resource(&self, path: &str) -> Resource { self.inner.resource(path) }

    /// Access the shared transport for custom JSON, SSE, and byte requests.
    pub fn transport(&self) -> &sydney_ai_core::Client { &self.inner }
}

fn segment(value: &str) -> Result<String> {
    if value.is_empty() || value == "." || value == ".."
        || value.chars().any(|c| c.is_control() || "/\\?#:%".contains(c))
    {
        return Err(Error::Config("invalid Gemini resource identifier".into()));
    }
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            result.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(result, "%{byte:02X}");
        }
    }
    Ok(result)
}

fn name(value: &str, collection: &str) -> Result<String> {
    let id = value.strip_prefix(collection).and_then(|v| v.strip_prefix('/')).unwrap_or(value);
    Ok(format!("{collection}/{}", segment(id)?))
}

fn model(value: &str) -> Result<String> {
    if value.starts_with("tunedModels/") { name(value, "tunedModels") } else { name(value, "models") }
}

fn nested_name(value: &str, pattern: &[&str]) -> Result<String> {
    let pieces: Vec<_> = value.split('/').collect();
    if pieces.len() != pattern.len() || pieces.iter().zip(pattern).any(|(v, p)| *p != "*" && v != p) {
        return Err(Error::Config("invalid Gemini resource name".into()));
    }
    pieces.iter().map(|v| segment(v)).collect::<Result<Vec<_>>>().map(|v| v.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifiers_cannot_escape_the_service() {
        for value in ["", ".", "..", "../models", "https://evil.invalid", "//evil.invalid", "x?key=y", "x#y", "x%2fy", "x\\y", "x\n"] {
            assert!(name(value, "models").is_err(), "{value:?}");
        }
        assert_eq!(model("models/future-model").unwrap(), "models/future-model");
        assert_eq!(model("future-model").unwrap(), "models/future-model");
        assert_eq!(segment("a b").unwrap(), "a%20b");
    }
    #[test]
    fn unknown_fields_and_tool_calls_round_trip() {
        let input = json!({"candidates":[{"content":{"role":"model","parts":[{"text":"internal", "thought":true},{"text":"hello"},{"functionCall":{"name":"lookup", "args":{"city":"Paris"},"future":true}, "thoughtSignature":"abc"}]},"newReason":"FUTURE"}],"futureResponse":42});
        let parsed: GenerateContentResponse = serde_json::from_value(input).unwrap();
        assert_eq!(parsed.text(), "hello");
        assert_eq!(parsed.function_calls().next().unwrap().name, "lookup");
        let output = serde_json::to_value(parsed).unwrap();
        assert_eq!(output["futureResponse"], 42);
        assert_eq!(output["candidates"][0]["newReason"], "FUTURE");
        assert_eq!(output["candidates"][0]["content"]["parts"][2]["thoughtSignature"], "abc");
    }
}
