//! Shared transport for the unofficial Sydney AI SDKs.
//!
//! Credentials remain in headers. Redirects are disabled, paths are checked before
//! authentication, and automatic retries are limited to idempotent requests.
#![forbid(unsafe_code)]

mod config;
mod error;
mod resource;
mod sse;
mod transport;

pub use config::Config;
pub use error::{Error, Result};
pub use reqwest::{self, Method};
pub use resource::{encode_segment, Page, Resource};
pub use serde_json::{self, Value};
pub use sse::{EventStream, SseEvent};
pub use transport::{Client, RawResponse};
