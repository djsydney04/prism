# prism-ai-core

Shared infrastructure for `prism-openai`, `prism-gemini`, and `prism-groq`.
Independent unofficial Rust SDKs. Async Tokio/reqwest with Rustls, bounded response
bodies, SSE, multipart uploads, same-origin URL validation, redacted credentials,
and structured API errors. See the repository README for usage and coverage.

## WebSocket TLS

Normal SDK WebSocket methods configure Rustls with the current bundled Mozilla
WebPKI roots, certificate-chain and hostname verification, and TLS 1.2/1.3.
Plain WS remains limited to loopback by the SDK configuration checks.

The low-level `prism_ai_core::tokio_tungstenite` reexport enables Rustls without
upstream root-loading features, which still depend on an obsolete certificate
package. **Raw WSS callers must now supply a TLS connector.** The raw
`connect_async` / `connect_async_with_config` defaults have no trust roots; use
`connect_async_tls_with_config` with `Some(Connector::Rustls(...))` instead.
`client_async_tls_with_config` likewise requires a connector when upgrading a
caller-provided stream to TLS. This does not change `Client::websocket`, provider
session method signatures, `WebSocketConnection`, or sockets obtained through
`into_inner()`.

SDK connections now consistently use the bundled Mozilla roots and Rustls.
Enabling `native-tls` or `rustls-tls-native-roots` on another dependency no longer
selects a different backend or adds OS trust roots to SDK sessions. Applications
that relied on this Cargo feature unification to trust a custom origin solely
through the OS store must migrate to a raw connection with their own TLS
connector. Default SDK clients using Mozilla-trusted servers retain their
existing behavior.

For direct raw connections, add these dependencies to your application:

```toml
rustls = { version = "0.23", default-features = false, features = ["ring", "std", "tls12"] }
webpki-roots = "1.0.10"
```

Then build the connector and pass it explicitly:

```rust,no_run
use prism_ai_core::tokio_tungstenite::{connect_async_tls_with_config, Connector};
use std::sync::Arc;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
let tls = rustls::ClientConfig::builder_with_provider(
    Arc::new(rustls::crypto::ring::default_provider()),
)
.with_safe_default_protocol_versions()?
.with_root_certificates(roots)
.with_no_client_auth();
let (_socket, _response) = connect_async_tls_with_config(
    "wss://example.com/socket", None, false, Some(Connector::Rustls(Arc::new(tls))),
).await?;
# Ok(())
# }
```

Raw connections bypass the SDK's authentication, timeout, bounds, and diagnostic
redaction. Do not disable certificate verification to work around TLS failures.
