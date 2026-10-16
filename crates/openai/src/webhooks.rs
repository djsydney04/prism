//! Verify incoming OpenAI webhook signatures before processing their raw body.
//!
//! Implements the HMAC-SHA256 signing scheme referenced by the
//! [official webhook guide](https://developers.openai.com/api/docs/guides/webhooks).
//! The signed bytes are `webhook-id.webhook-timestamp.` followed by the exact
//! HTTP request body. Do not parse, trim, or reserialize the body first.
//!
//! Verification checks a five-minute timestamp window by default. Applications
//! must also deduplicate successfully verified webhook IDs in durable storage:
//! the timestamp window alone does not prevent repeated delivery in that window.
//!
//! ```no_run
//! use prism_openai::webhooks::{WebhookHeaders, WebhookVerifier};
//! # fn handle(body: &[u8], id: &str, timestamp: &str, signature: &str)
//! # -> Result<(), Box<dyn std::error::Error>> {
//! let secret = std::env::var("OPENAI_WEBHOOK_SECRET")?;
//! let verifier = WebhookVerifier::new(&secret)?;
//! let headers = WebhookHeaders { id, timestamp, signature };
//! let verified = verifier.verify(body, headers)?;
//! let event: serde_json::Value = verified.json()?;
//! // Deduplicate verified.id() before processing event.
//! # let _ = event;
//! # Ok(()) }
//! ```

use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
use serde::de::DeserializeOwned;
use sha2::Sha256;
use std::{
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Result for local webhook verification, independent of HTTP client errors.
pub type WebhookResult<T> = std::result::Result<T, WebhookError>;

/// Rejected signature, timestamp, configuration, header, or verified JSON.
/// Errors never include the signing secret or supplied signature.
#[derive(Debug)]
#[non_exhaustive]
pub enum WebhookError {
    InvalidSecret,
    MissingHeader(&'static str),
    InvalidHeader(&'static str),
    InvalidTimestamp,
    TimestampTooOld,
    TimestampInFuture,
    InvalidSignature,
    ClockBeforeUnixEpoch,
    Json(serde_json::Error),
}

impl fmt::Display for WebhookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSecret => f.write_str("invalid webhook signing secret"),
            Self::MissingHeader(name) => write!(f, "missing {name} header"),
            Self::InvalidHeader(name) => write!(f, "invalid or duplicate {name} header"),
            Self::InvalidTimestamp => f.write_str("invalid webhook timestamp"),
            Self::TimestampTooOld => f.write_str("webhook timestamp is too old"),
            Self::TimestampInFuture => f.write_str("webhook timestamp is too far in the future"),
            Self::InvalidSignature => f.write_str("webhook signature verification failed"),
            Self::ClockBeforeUnixEpoch => f.write_str("system clock precedes the Unix epoch"),
            Self::Json(_) => {
                f.write_str("verified webhook body is not valid for the requested JSON type")
            }
        }
    }
}
impl std::error::Error for WebhookError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

/// Required request header values. Preserve them exactly as received.
#[derive(Clone, Copy)]
pub struct WebhookHeaders<'a> {
    pub id: &'a str,
    pub timestamp: &'a str,
    /// Space-separated `v1,<base64>` signatures; any valid signature is accepted.
    pub signature: &'a str,
}

impl<'a> WebhookHeaders<'a> {
    /// Read case-insensitive header names from a reqwest/http HeaderMap.
    /// Duplicate physical headers are rejected to avoid ambiguous inputs.
    pub fn from_headers(headers: &'a crate::reqwest::header::HeaderMap) -> WebhookResult<Self> {
        fn one<'a>(
            headers: &'a crate::reqwest::header::HeaderMap,
            name: &'static str,
        ) -> WebhookResult<&'a str> {
            let mut values = headers.get_all(name).iter();
            let value = values.next().ok_or(WebhookError::MissingHeader(name))?;
            if values.next().is_some() {
                return Err(WebhookError::InvalidHeader(name));
            }
            value
                .to_str()
                .map_err(|_| WebhookError::InvalidHeader(name))
        }
        Ok(Self {
            id: one(headers, "webhook-id")?,
            timestamp: one(headers, "webhook-timestamp")?,
            signature: one(headers, "webhook-signature")?,
        })
    }
}
impl fmt::Debug for WebhookHeaders<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebhookHeaders")
            .field("id", &self.id)
            .field("timestamp", &self.timestamp)
            .field("signature", &"[REDACTED]")
            .finish()
    }
}

/// A verified raw webhook body and authenticated delivery metadata.
/// This value can only be constructed by successful verification.
pub struct VerifiedWebhook<'a> {
    body: &'a [u8],
    id: &'a str,
    timestamp: u64,
}
impl<'a> VerifiedWebhook<'a> {
    pub fn body(&self) -> &'a [u8] {
        self.body
    }
    pub fn id(&self) -> &'a str {
        self.id
    }
    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }
    /// Deserialize only after successful signature and timestamp verification.
    pub fn json<T: DeserializeOwned>(&self) -> WebhookResult<T> {
        serde_json::from_slice(self.body).map_err(WebhookError::Json)
    }
}
impl fmt::Debug for VerifiedWebhook<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerifiedWebhook")
            .field("id", &self.id)
            .field("timestamp", &self.timestamp)
            .field("body_length", &self.body.len())
            .finish()
    }
}

/// Reusable verifier for one endpoint signing secret. Cloning preserves its
/// configuration; Debug redacts the cryptographic key state.
#[derive(Clone)]
pub struct WebhookVerifier {
    mac: Hmac<Sha256>,
    tolerance: Duration,
}
impl fmt::Debug for WebhookVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebhookVerifier")
            .field("secret", &"[REDACTED]")
            .field("tolerance", &self.tolerance)
            .finish()
    }
}
impl WebhookVerifier {
    /// Decode a `whsec_` signing secret using standard Base64. An absent prefix,
    /// empty signing key, or malformed encoding is rejected without echoing it.
    pub fn new(secret: &str) -> WebhookResult<Self> {
        let encoded = secret
            .strip_prefix("whsec_")
            .ok_or(WebhookError::InvalidSecret)?;
        let key = STANDARD
            .decode(encoded)
            .map_err(|_| WebhookError::InvalidSecret)?;
        if key.is_empty() {
            return Err(WebhookError::InvalidSecret);
        }
        let mac = Hmac::<Sha256>::new_from_slice(&key).map_err(|_| WebhookError::InvalidSecret)?;
        Ok(Self {
            mac,
            tolerance: Duration::from_secs(300),
        })
    }
    /// Set the maximum age and permitted future clock skew, in whole seconds.
    /// A smaller window reduces replay exposure but requires synchronized clocks.
    /// This does not replace delivery-ID deduplication.
    pub fn with_tolerance(mut self, tolerance: Duration) -> Self {
        self.tolerance = tolerance;
        self
    }
    /// Verify the original body using the system's current Unix timestamp.
    pub fn verify<'a>(
        &self,
        body: &'a [u8],
        headers: WebhookHeaders<'a>,
    ) -> WebhookResult<VerifiedWebhook<'a>> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| WebhookError::ClockBeforeUnixEpoch)?
            .as_secs();
        self.verify_at(body, headers, now)
    }
    /// Verify using an explicit current Unix timestamp for deterministic servers
    /// and tests. The original timestamp string participates in the signature.
    pub fn verify_at<'a>(
        &self,
        body: &'a [u8],
        headers: WebhookHeaders<'a>,
        now: u64,
    ) -> WebhookResult<VerifiedWebhook<'a>> {
        if headers.id.is_empty()
            || headers.id.len() > 1024
            || !headers.id.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(WebhookError::InvalidHeader("webhook-id"));
        }
        if headers.timestamp.is_empty()
            || headers.timestamp.len() > 20
            || !headers.timestamp.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(WebhookError::InvalidTimestamp);
        }
        let timestamp = headers
            .timestamp
            .parse::<u64>()
            .map_err(|_| WebhookError::InvalidTimestamp)?;
        if timestamp < now && now - timestamp > self.tolerance.as_secs() {
            return Err(WebhookError::TimestampTooOld);
        }
        if timestamp > now && timestamp - now > self.tolerance.as_secs() {
            return Err(WebhookError::TimestampInFuture);
        }
        if headers.signature.is_empty()
            || headers.signature.len() > 16384
            || !headers
                .signature
                .bytes()
                .all(|b| b == b' ' || b.is_ascii_graphic())
        {
            return Err(WebhookError::InvalidHeader("webhook-signature"));
        }
        let mut mac = self.mac.clone();
        mac.update(headers.id.as_bytes());
        mac.update(b".");
        mac.update(headers.timestamp.as_bytes());
        mac.update(b".");
        mac.update(body);
        for token in headers
            .signature
            .split(' ')
            .filter(|token| !token.is_empty())
        {
            let Some(encoded) = token.strip_prefix("v1,") else {
                continue;
            };
            let Ok(signature) = STANDARD.decode(encoded) else {
                continue;
            };
            // RustCrypto's full-tag comparison is constant-time for equal-length tags.
            if mac.clone().verify_slice(&signature).is_ok() {
                return Ok(VerifiedWebhook {
                    body,
                    id: headers.id,
                    timestamp,
                });
            }
        }
        Err(WebhookError::InvalidSignature)
    }
    /// Verify first, then deserialize the authenticated JSON body.
    pub fn unwrap<T: DeserializeOwned>(
        &self,
        body: &[u8],
        headers: WebhookHeaders<'_>,
    ) -> WebhookResult<T> {
        self.verify(body, headers)?.json()
    }
    /// Verify and deserialize using a supplied current Unix timestamp.
    pub fn unwrap_at<T: DeserializeOwned>(
        &self,
        body: &[u8],
        headers: WebhookHeaders<'_>,
        now: u64,
    ) -> WebhookResult<T> {
        self.verify_at(body, headers, now)?.json()
    }
}
