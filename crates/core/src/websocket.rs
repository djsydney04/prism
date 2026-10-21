//! Bounded, authenticated WebSocket sessions. No automatic reconnect or replay.
use crate::{Client, Error, Result};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{fmt, sync::Arc};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async_tls_with_config,
    tungstenite::{
        self,
        client::IntoClientRequest,
        http::header::{HeaderValue, AUTHORIZATION},
        protocol::WebSocketConfig,
        Message,
    },
    Connector, MaybeTlsStream, WebSocketStream,
};

/// Raw connection, for applications requiring the underlying stream/sink traits.
/// Raw tungstenite diagnostics may contain payloads; avoid logging sensitive data.
pub type WebSocketConnection = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Authenticated WebSocket session with bounded frames and redacted diagnostics.
pub struct WebSocketSession {
    socket: WebSocketConnection,
    max_message_bytes: usize,
}
impl fmt::Debug for WebSocketSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebSocketSession")
            .field("max_message_bytes", &self.max_message_bytes)
            .finish_non_exhaustive()
    }
}

/// Sending half for concurrent input and output.
pub struct WebSocketSender {
    sink: SplitSink<WebSocketConnection, Message>,
    max_message_bytes: usize,
}
impl fmt::Debug for WebSocketSender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebSocketSender")
            .field("max_message_bytes", &self.max_message_bytes)
            .finish_non_exhaustive()
    }
}

/// Receiving half for concurrent input and output.
pub struct WebSocketReceiver {
    stream: SplitStream<WebSocketConnection>,
}
impl fmt::Debug for WebSocketReceiver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WebSocketReceiver { .. }")
    }
}

fn ws_error(error: tungstenite::Error) -> Error {
    // Do not interpolate the error: it can include credential-bearing URLs,
    // HTTP response bodies, invalid text, or complete rejected write buffers.
    match error {
        tungstenite::Error::Http(response) => Error::Api {
            status: response.status().as_u16(),
            message: "WebSocket handshake rejected".into(),
            request_id: response
                .headers()
                .get("x-request-id")
                .or_else(|| response.headers().get("x-goog-request-id"))
                .and_then(|h| h.to_str().ok())
                .map(str::to_owned),
            body: serde_json::Value::Null,
        },
        tungstenite::Error::Capacity(_) => {
            Error::Protocol("WebSocket message exceeded configured size limit".into())
        }
        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed => {
            Error::Protocol("WebSocket connection is closed".into())
        }
        tungstenite::Error::Io(_) => Error::Protocol("WebSocket I/O failed".into()),
        tungstenite::Error::Tls(_) => Error::Protocol("WebSocket TLS handshake failed".into()),
        _ => Error::Protocol("WebSocket protocol error".into()),
    }
}

fn json_message<B: Serialize + ?Sized>(body: &B) -> Result<Message> {
    serde_json::to_string(body)
        .map(|s| Message::Text(s.into()))
        .map_err(|_| Error::Protocol("WebSocket JSON serialization failed".into()))
}
fn decode_json<T: DeserializeOwned>(message: Message) -> Result<Option<T>> {
    match message {
        Message::Text(text) => serde_json::from_slice(text.as_bytes()).map(Some),
        Message::Binary(bytes) => serde_json::from_slice(&bytes).map(Some),
        _ => return Ok(None),
    }
    .map_err(|_| Error::Protocol("received invalid JSON over WebSocket".into()))
}
fn check_message(message: &Message, limit: usize) -> Result<()> {
    if message.len() > limit {
        Err(Error::Protocol(
            "WebSocket message exceeded configured size limit".into(),
        ))
    } else {
        Ok(())
    }
}

fn tls_connector(roots: rustls::RootCertStore) -> Result<Connector> {
    // Match reqwest: honor an installed provider, otherwise use Ring. Avoid
    // relying on feature unification to choose or install a global provider.
    let provider = rustls::crypto::CryptoProvider::get_default()
        .cloned()
        .unwrap_or_else(|| Arc::new(rustls::crypto::ring::default_provider()));
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| Error::Config("invalid WebSocket TLS configuration".into()))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Connector::Rustls(Arc::new(config)))
}

impl Client {
    /// Connect using the configured authentication header. HTTPS becomes WSS;
    /// loopback HTTP becomes WS. Cross-origin and redirect handshakes are rejected.
    pub async fn websocket(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<WebSocketSession> {
        self.connect_websocket(path, query, None).await
    }

    /// Connect using an API-key query parameter where required by the provider.
    /// In this mode the key is omitted from authentication headers. Avoid logging
    /// URLs at proxies: unlike normal REST requests, this protocol requires a query key.
    pub async fn websocket_with_key_query(
        &self,
        path: &str,
        query: &[(String, String)],
        key_parameter: &str,
    ) -> Result<WebSocketSession> {
        if key_parameter.is_empty()
            || !key_parameter
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(Error::Config("invalid WebSocket key parameter".into()));
        }
        if query
            .iter()
            .any(|(k, _)| k == key_parameter || k == "key" || k == "access_token")
        {
            return Err(Error::Config(
                "WebSocket authentication query cannot be overridden".into(),
            ));
        }
        self.connect_websocket(path, query, Some(key_parameter))
            .await
    }

    async fn connect_websocket(
        &self,
        path: &str,
        query: &[(String, String)],
        key_parameter: Option<&str>,
    ) -> Result<WebSocketSession> {
        let mut url = self.url(path, false)?;
        if !query.is_empty() || key_parameter.is_some() {
            let mut pairs = url.query_pairs_mut();
            pairs.extend_pairs(query.iter().map(|(k, v)| (k, v)));
            if let Some(parameter) = key_parameter {
                pairs.append_pair(parameter, &self.config.key);
            }
        }
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        url.set_scheme(scheme)
            .map_err(|_| Error::Config("invalid WebSocket scheme".into()))?;
        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|_| Error::Config("invalid WebSocket request".into()))?;
        for (name, value) in &self.config.headers {
            request.headers_mut().insert(name.clone(), value.clone());
        }
        if key_parameter.is_none() {
            let (header, key) = match &self.config.key_header {
                Some(header) => (header.clone(), self.config.key.clone()),
                None => (AUTHORIZATION, format!("Bearer {}", self.config.key)),
            };
            let mut value = HeaderValue::from_str(&key)
                .map_err(|_| Error::Config("invalid WebSocket authentication header".into()))?;
            value.set_sensitive(true);
            request.headers_mut().insert(header, value);
        }
        let limit = self.config.max_event_bytes;
        let config = WebSocketConfig::default()
            .max_message_size(Some(limit))
            .max_frame_size(Some(limit))
            .write_buffer_size(0)
            .max_write_buffer_size(limit.saturating_add(1024));
        let connector = if scheme == "wss" {
            Some(tls_connector(rustls::RootCertStore::from_iter(
                webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
            ))?)
        } else {
            None
        };
        let (socket, _) = tokio::time::timeout(
            self.config.timeout,
            connect_async_tls_with_config(request, Some(config), false, connector),
        )
        .await
        .map_err(|_| Error::Protocol("WebSocket handshake timed out".into()))?
        .map_err(ws_error)?;
        Ok(WebSocketSession {
            socket,
            max_message_bytes: limit,
        })
    }
}

impl WebSocketSession {
    pub async fn send_json<B: Serialize + ?Sized>(&mut self, body: &B) -> Result<()> {
        self.send_message(json_message(body)?).await
    }
    pub async fn send_message(&mut self, message: Message) -> Result<()> {
        check_message(&message, self.max_message_bytes)?;
        self.socket.send(message).await.map_err(ws_error)
    }
    pub async fn send_text(&mut self, text: impl Into<String>) -> Result<()> {
        self.send_message(Message::Text(text.into().into())).await
    }
    pub async fn send_binary(&mut self, bytes: impl Into<Vec<u8>>) -> Result<()> {
        self.send_message(Message::Binary(bytes.into().into()))
            .await
    }
    /// Receive text, binary, or control messages. A close frame is returned once.
    pub async fn recv_message(&mut self) -> Result<Option<Message>> {
        self.socket.next().await.transpose().map_err(ws_error)
    }
    /// Decode text or binary JSON, skipping ping/pong controls. `None` means closed.
    pub async fn recv_json<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
        while let Some(message) = self.recv_message().await? {
            if message.is_close() {
                return Ok(None);
            }
            if let Some(value) = decode_json(message)? {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }
    pub async fn close(&mut self) -> Result<()> {
        self.socket.close(None).await.map_err(ws_error)
    }
    pub fn split(self) -> (WebSocketSender, WebSocketReceiver) {
        let (sink, stream) = self.socket.split();
        (
            WebSocketSender {
                sink,
                max_message_bytes: self.max_message_bytes,
            },
            WebSocketReceiver { stream },
        )
    }
    /// Access the raw socket; low-level errors are no longer redacted by this SDK.
    pub fn into_inner(self) -> WebSocketConnection {
        self.socket
    }
}

impl WebSocketSender {
    pub async fn send_json<B: Serialize + ?Sized>(&mut self, body: &B) -> Result<()> {
        self.send_message(json_message(body)?).await
    }
    pub async fn send_message(&mut self, message: Message) -> Result<()> {
        check_message(&message, self.max_message_bytes)?;
        self.sink.send(message).await.map_err(ws_error)
    }
    pub async fn close(&mut self) -> Result<()> {
        self.sink.close().await.map_err(ws_error)
    }
}
impl WebSocketReceiver {
    pub async fn recv_message(&mut self) -> Result<Option<Message>> {
        self.stream.next().await.transpose().map_err(ws_error)
    }
    pub async fn recv_json<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
        while let Some(message) = self.recv_message().await? {
            if message.is_close() {
                return Ok(None);
            }
            if let Some(value) = decode_json(message)? {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use rcgen::{CertificateParams, KeyPair};
    use rustls::{
        pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName},
        CertificateError, ClientConnection, RootCertStore, ServerConfig, ServerConnection,
    };
    use std::time::Duration;

    fn identity(expired: bool) -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
        let key = KeyPair::generate().unwrap();
        let mut params =
            CertificateParams::new(vec!["localhost".into(), "127.0.0.1".into()]).unwrap();
        if expired {
            params.not_before = rcgen::date_time_ymd(1990, 1, 1);
            params.not_after = rcgen::date_time_ymd(2000, 1, 1);
        }
        let cert = params.self_signed(&key).unwrap();
        (
            cert.der().clone(),
            PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
        )
    }

    fn server_config(
        cert: CertificateDer<'static>,
        key: PrivateKeyDer<'static>,
        version: &'static rustls::SupportedProtocolVersion,
    ) -> ServerConfig {
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[version])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap()
    }

    // Exercise the actual connector's verifier without public-network servers
    // or a test-only trust override in the SDK's connection path.
    fn handshake(
        connector: Connector,
        server: ServerConfig,
        hostname: &'static str,
    ) -> std::result::Result<(), rustls::Error> {
        let Connector::Rustls(config) = connector else {
            panic!("expected a Rustls connector");
        };
        let mut client =
            ClientConnection::new(config, ServerName::try_from(hostname).unwrap()).unwrap();
        let mut server = ServerConnection::new(Arc::new(server)).unwrap();
        for _ in 0..16 {
            let mut bytes = Vec::new();
            client.write_tls(&mut bytes).unwrap();
            if !bytes.is_empty() {
                server.read_tls(&mut bytes.as_slice()).unwrap();
                server.process_new_packets()?;
            }
            bytes.clear();
            server.write_tls(&mut bytes).unwrap();
            if !bytes.is_empty() {
                client.read_tls(&mut bytes.as_slice()).unwrap();
                client.process_new_packets()?;
            }
            if !client.is_handshaking() && !server.is_handshaking() {
                return Ok(());
            }
        }
        panic!("TLS handshake did not complete");
    }

    #[test]
    fn trusted_certificate_supports_tls12_and_tls13() {
        for version in [&rustls::version::TLS12, &rustls::version::TLS13] {
            let (cert, key) = identity(false);
            let mut roots = RootCertStore::empty();
            roots.add(cert.clone()).unwrap();
            handshake(
                tls_connector(roots).unwrap(),
                server_config(cert, key, version),
                "localhost",
            )
            .unwrap();
        }
    }

    #[test]
    fn trusted_certificate_with_wrong_hostname_is_rejected() {
        let (cert, key) = identity(false);
        let mut roots = RootCertStore::empty();
        roots.add(cert.clone()).unwrap();
        let error = handshake(
            tls_connector(roots).unwrap(),
            server_config(cert, key, &rustls::version::TLS13),
            "wrong.example",
        )
        .unwrap_err();
        assert!(matches!(
            error,
            rustls::Error::InvalidCertificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. }
            )
        ));
    }

    #[test]
    fn trusted_but_expired_certificate_is_rejected() {
        let (cert, key) = identity(true);
        let mut roots = RootCertStore::empty();
        roots.add(cert.clone()).unwrap();
        let error = handshake(
            tls_connector(roots).unwrap(),
            server_config(cert, key, &rustls::version::TLS13),
            "localhost",
        )
        .unwrap_err();
        assert!(matches!(
            error,
            rustls::Error::InvalidCertificate(
                CertificateError::Expired | CertificateError::ExpiredContext { .. }
            )
        ));
    }

    #[tokio::test]
    async fn sdk_rejects_untrusted_tls_before_header_or_query_authentication() {
        for query_auth in [false, true] {
            let (cert, key) = identity(false);
            let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config(
                cert,
                key,
                &rustls::version::TLS13,
            )));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin = format!("https://{}/", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(5), async move {
                    let (tcp, _) = listener.accept().await.unwrap();
                    // TLS must fail before the server can receive an HTTP
                    // upgrade request containing either form of credentials.
                    assert!(acceptor.accept(tcp).await.is_err());
                })
                .await
                .unwrap();
            });
            let client = Client::new(Config::new(origin, "tls-test-secret")).unwrap();
            let result = if query_auth {
                client.websocket_with_key_query("/ws", &[], "key").await
            } else {
                client.websocket("/ws", &[]).await
            };
            let error = result.unwrap_err();
            assert!(matches!(error, Error::Protocol(_)));
            assert_eq!(error.to_string(), "protocol error: WebSocket I/O failed");
            assert!(!format!("{error:?}").contains("tls-test-secret"));
            server.await.unwrap();
        }
    }
}
