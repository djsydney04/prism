#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod resources;
pub mod types;

pub use resources::*;
pub use serde_json::{self, json, Value};
pub use sydney_ai_core::{
    tungstenite, Config, Error, EventStream, Method, RawResponse, Resource, Result,
    WebSocketReceiver, WebSocketSender, WebSocketSession,
};
pub use types::*;

/// Default service origin. Endpoints use explicit `/v1beta/` paths.
pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Authentication {
    ApiKey,
    OAuth,
}

/// A cheaply cloneable Gemini client. REST keys use `x-goog-api-key` headers;
/// Gemini Live requires query authentication for its WebSocket protocol.
#[derive(Clone, Debug)]
pub struct Client {
    inner: sydney_ai_core::Client,
    authentication: Authentication,
}

impl Client {
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Self::with_config(Config::new(DEFAULT_BASE_URL, api_key))
    }

    /// Read `GEMINI_API_KEY`. REST authentication uses a header; Live uses a query key.
    pub fn from_env() -> Result<Self> {
        Self::with_config(Config::from_env(DEFAULT_BASE_URL, "GEMINI_API_KEY")?)
    }

    /// Override transport settings and the service origin (for example, a mock server).
    pub fn with_config(config: Config) -> Result<Self> {
        Ok(Self {
            inner: sydney_ai_core::Client::new(config.with_api_key_header("x-goog-api-key")?)?,
            authentication: Authentication::ApiKey,
        })
    }

    /// Use a caller-managed OAuth access token for REST requests. The application
    /// obtains and refreshes tokens and supplies any required quota-project header.
    /// This constructor does not enable Vertex AI or Live query-token authentication.
    pub fn with_oauth_token(access_token: impl Into<String>) -> Result<Self> {
        Self::with_oauth_config(Config::new(DEFAULT_BASE_URL, access_token))
    }

    /// Configure caller-managed OAuth bearer authentication, overriding key-header
    /// selection. Use `Config::header("x-goog-user-project", project)` if required.
    pub fn with_oauth_config(config: Config) -> Result<Self> {
        Ok(Self {
            inner: sydney_ai_core::Client::new(config.with_bearer_authentication())?,
            authentication: Authentication::OAuth,
        })
    }

    pub fn models(&self) -> Models<'_> {
        Models(self)
    }
    pub fn files(&self) -> Files<'_> {
        Files(self)
    }
    pub fn cached_contents(&self) -> CachedContents<'_> {
        CachedContents(self)
    }
    pub fn batches(&self) -> Batches<'_> {
        Batches(self)
    }
    pub fn file_search_stores(&self) -> FileSearchStores<'_> {
        FileSearchStores(self)
    }
    pub fn interactions(&self) -> Interactions<'_> {
        Interactions(self)
    }
    pub fn operations(&self) -> Operations<'_> {
        Operations(self)
    }
    pub fn agents(&self) -> Agents<'_> {
        Agents(self)
    }
    pub fn webhooks(&self) -> Webhooks<'_> {
        Webhooks(self)
    }
    pub fn triggers(&self) -> Triggers<'_> {
        Triggers(self)
    }
    pub fn environments(&self) -> Environments<'_> {
        Environments(self)
    }
    pub fn voices(&self) -> Voices<'_> {
        Voices(self)
    }
    pub fn credentials(&self) -> Credentials<'_> {
        Credentials(self)
    }

    /// Create a short-lived token for a constrained Live WebSocket session.
    pub async fn create_auth_token<B: serde::Serialize + ?Sized>(&self, body: &B) -> Result<Value> {
        self.inner.post("/v1beta/auth_tokens", body).await
    }

    /// Open a Gemini Live socket and send `{"setup": setup}` as its first message.
    /// Wait for `setupComplete` before sending client content or realtime input.
    /// Gemini's WebSocket protocol requires the configured API key in the URL query.
    pub async fn live_connect<B: serde::Serialize + ?Sized>(
        &self,
        setup: &B,
    ) -> Result<WebSocketSession> {
        self.connect_live(setup, false).await
    }

    /// Use a short-lived token as the configured credential and connect to the
    /// constrained Live endpoint with `access_token` query authentication.
    pub async fn live_connect_ephemeral<B: serde::Serialize + ?Sized>(
        &self,
        setup: &B,
    ) -> Result<WebSocketSession> {
        self.connect_live(setup, true).await
    }

    async fn connect_live<B: serde::Serialize + ?Sized>(
        &self,
        setup: &B,
        ephemeral: bool,
    ) -> Result<WebSocketSession> {
        if self.authentication == Authentication::OAuth {
            return Err(Error::Config("Gemini Live requires an API-key or ephemeral-token client; OAuth clients support REST only".into()));
        }
        let setup = serde_json::to_value(setup)?;
        if !setup.is_object() {
            return Err(Error::Config("Live setup must be an object".into()));
        }
        let (path, parameter) = if ephemeral {
            ("/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContentConstrained", "access_token")
        } else {
            (
                "/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent",
                "key",
            )
        };
        let mut session = self
            .inner
            .websocket_with_key_query(path, &[], parameter)
            .await?;
        session.send_json(&json!({"setup": setup})).await?;
        Ok(session)
    }

    /// Escape hatch for new/legacy REST resources; the transport validates paths.
    /// Example: `client.resource("/v1beta/tunedModels")`.
    pub fn resource(&self, path: &str) -> Resource {
        self.inner.resource(path)
    }

    /// Access the shared transport for custom JSON, SSE, and byte requests.
    pub fn transport(&self) -> &sydney_ai_core::Client {
        &self.inner
    }
}

fn segment(value: &str) -> Result<String> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value
            .chars()
            .any(|c| c.is_control() || "/\\?#:%".contains(c))
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
    let id = value
        .strip_prefix(collection)
        .and_then(|v| v.strip_prefix('/'))
        .unwrap_or(value);
    Ok(format!("{collection}/{}", segment(id)?))
}

fn model(value: &str) -> Result<String> {
    if value.starts_with("tunedModels/") {
        name(value, "tunedModels")
    } else {
        name(value, "models")
    }
}

fn nested_name(value: &str, pattern: &[&str]) -> Result<String> {
    let pieces: Vec<_> = value.split('/').collect();
    if pieces.len() != pattern.len() || pieces.iter().zip(pattern).any(|(v, p)| *p != "*" && v != p)
    {
        return Err(Error::Config("invalid Gemini resource name".into()));
    }
    pieces
        .iter()
        .map(|v| segment(v))
        .collect::<Result<Vec<_>>>()
        .map(|v| v.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifiers_cannot_escape_the_service() {
        for value in [
            "",
            ".",
            "..",
            "../models",
            "https://evil.invalid",
            "//evil.invalid",
            "x?key=y",
            "x#y",
            "x%2fy",
            "x\\y",
            "x\n",
        ] {
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
        assert_eq!(
            output["candidates"][0]["content"]["parts"][2]["thoughtSignature"],
            "abc"
        );
    }
}
