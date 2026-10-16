use serde_json::Value;

/// A result returned by an SDK operation.
pub type Result<T> = std::result::Result<T, Error>;

/// Transport, serialization, configuration, or provider failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("HTTP transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("JSON serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("API error ({status}): {message}")]
    Api {
        status: u16,
        message: String,
        request_id: Option<String>,
        body: Value,
    },
    #[error("protocol error: {0}")]
    Protocol(String),
}

impl Error {
    /// HTTP status for errors returned by the provider.
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Api { status, .. } => Some(*status),
            _ => None,
        }
    }
    /// Provider request ID, when supplied in response headers.
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::Api { request_id, .. } => request_id.as_deref(),
            _ => None,
        }
    }
    /// Whether an idempotent operation may succeed after a delay.
    pub fn is_retryable(&self) -> bool {
        matches!(self.status(), Some(408 | 409 | 429 | 500..=599))
            || matches!(self, Self::Transport(e) if e.is_connect() || e.is_timeout())
    }
}
