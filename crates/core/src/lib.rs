//! Shared transport for the unofficial Prism AI SDKs.
//!
//! REST credentials remain in headers; WebSocket protocols can opt into required
//! query authentication. Paths are checked before authentication, and automatic
//! retries are limited to idempotent HTTP requests.
#![forbid(unsafe_code)]

mod config;
mod error;
mod resource;
mod sse;
mod transport;
mod websocket;

pub use config::Config;
pub use error::{Error, Result};
pub use reqwest::{self, Method};
pub use resource::{encode_segment, Page, Resource};
pub use serde_json::{self, Value};
pub use sse::{EventStream, SseEvent};
pub use tokio_tungstenite::{self, tungstenite};
pub use transport::{Client, RawResponse};
pub use websocket::{WebSocketConnection, WebSocketReceiver, WebSocketSender, WebSocketSession};
