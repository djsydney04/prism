use crate::{Config, Error, EventStream, Method, Resource, Result, Value};
use futures_util::StreamExt;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER},
    Url,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fmt,
    sync::Arc,
    time::{Duration, SystemTime},
};

/// Shared, connection-pooled, cloneable HTTP transport.
#[derive(Clone)]
pub struct Client {
    pub(crate) inner: reqwest::Client,
    pub(crate) config: Arc<Config>,
}
impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}
/// Successful unparsed response for binary and resumable upload APIs.
#[derive(Debug)]
pub struct RawResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}
impl RawResponse {
    pub fn json<T: DeserializeOwned>(&self) -> Result<T> {
        decode(&self.body)
    }
}
fn decode<T: DeserializeOwned>(body: &[u8]) -> Result<T> {
    Ok(serde_json::from_slice(if body.is_empty() {
        b"null"
    } else {
        body
    })?)
}

impl Client {
    pub fn new(mut config: Config) -> Result<Self> {
        config.validate()?;
        let mut headers = config.headers.clone();
        let (name, value) = match &config.key_header {
            Some(name) => (name.clone(), config.key.clone()),
            None => (AUTHORIZATION, format!("Bearer {}", config.key)),
        };
        let mut value = HeaderValue::from_str(&value)
            .map_err(|_| Error::Config("API key is not a valid header value".into()))?;
        value.set_sensitive(true);
        headers.insert(name, value);
        let inner = reqwest::Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(config.timeout)
            .connect_timeout(Duration::from_secs(15))
            .user_agent(concat!("sydney-ai-sdk/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            inner,
            config: Arc::new(config),
        })
    }
    pub fn resource(&self, path: impl Into<String>) -> Resource {
        Resource {
            client: self.clone(),
            path: path.into(),
        }
    }
    pub(crate) fn url(&self, path: &str, absolute: bool) -> Result<Url> {
        if path.contains(['\\', '?', '#'])
            || path.chars().any(char::is_control)
            || path.starts_with("//")
            || (!path.contains("://")
                && path
                    .split('/')
                    .next()
                    .is_some_and(|part| part.contains(':')))
        {
            return Err(Error::Config(
                "path must not contain a query, fragment, control, or backslash".into(),
            ));
        }
        // Upload URLs may include opaque queries; handle absolute URLs separately in raw().
        if path.contains("://") {
            if !absolute {
                return Err(Error::Config("use a relative API path".into()));
            }
            let url = Url::parse(path).map_err(|_| Error::Config("invalid upload URL".into()))?;
            return self.same_origin(url);
        }
        for segment in path.split('/') {
            let decoded = percent_encoding::percent_decode_str(segment)
                .decode_utf8()
                .map_err(|_| Error::Config("path contains invalid UTF-8 encoding".into()))?;
            if decoded.split('/').any(|part| part == "." || part == "..")
                || decoded.contains('\\')
                || decoded.chars().any(char::is_control)
            {
                return Err(Error::Config(
                    "path contains unsafe traversal or encoded separator".into(),
                ));
            }
        }
        self.same_origin(
            self.config
                .base
                .join(path)
                .map_err(|_| Error::Config("invalid API path".into()))?,
        )
    }
    fn same_origin(&self, url: Url) -> Result<Url> {
        if url.origin() != self.config.base.origin()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::Config("request URL must use the configured origin without embedded credentials or fragments".into()));
        }
        Ok(url)
    }
    async fn send(
        &self,
        request: reqwest::RequestBuilder,
        retry: bool,
    ) -> Result<reqwest::Response> {
        let mut attempt = 0;
        loop {
            let req = request
                .try_clone()
                .ok_or_else(|| Error::Protocol("request body cannot be replayed".into()))?;
            let result = req.send().await;
            if retry && attempt < self.config.retries {
                let retryable = match &result {
                    Ok(r) => matches!(r.status().as_u16(), 408 | 409 | 429 | 500..=599),
                    Err(e) => e.is_connect() || e.is_timeout(),
                };
                if retryable {
                    let delay = result
                        .as_ref()
                        .ok()
                        .and_then(|r| r.headers().get(RETRY_AFTER))
                        .and_then(|h| h.to_str().ok())
                        .and_then(parse_retry_after)
                        .unwrap_or_else(|| Duration::from_millis(250 * (1u64 << attempt)))
                        .min(Duration::from_secs(60));
                    attempt += 1;
                    drop(result);
                    tokio::time::sleep(delay).await;
                    continue;
                }
            }
            return Ok(result?);
        }
    }
    async fn buffered(&self, response: reqwest::Response) -> Result<RawResponse> {
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        if response
            .content_length()
            .is_some_and(|len| len > self.config.max_response_bytes as u64)
        {
            return Err(Error::Protocol(
                "response exceeded configured size limit".into(),
            ));
        }
        let mut chunks = response.bytes_stream();
        let mut body = Vec::new();
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk?;
            if chunk.len() > self.config.max_response_bytes.saturating_sub(body.len()) {
                return Err(Error::Protocol(
                    "response exceeded configured size limit".into(),
                ));
            }
            body.extend_from_slice(&chunk);
        }
        if !(200..300).contains(&status) {
            let parsed: Value = serde_json::from_slice(&body)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&body).into_owned()));
            let message = parsed
                .pointer("/error/message")
                .or_else(|| parsed.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("provider returned HTTP {status}"));
            let request_id = headers
                .get("x-request-id")
                .or_else(|| headers.get("request-id"))
                .or_else(|| headers.get("x-goog-request-id"))
                .and_then(|h| h.to_str().ok())
                .map(str::to_owned);
            return Err(Error::Api {
                status,
                message,
                request_id,
                body: parsed,
            });
        }
        Ok(RawResponse {
            status,
            headers,
            body,
        })
    }
    pub async fn json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        body: Option<&B>,
    ) -> Result<T> {
        let retry = matches!(method, Method::GET | Method::HEAD | Method::OPTIONS);
        let mut request = self
            .inner
            .request(method, self.url(path, false)?)
            .query(query);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = self.send(request, retry).await?;
        self.buffered(response).await?.json()
    }
    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<T> {
        self.json::<Value, T>(Method::GET, path, query, None).await
    }
    pub async fn post<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.json(Method::POST, path, &[], Some(body)).await
    }
    pub async fn patch<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.json(Method::PATCH, path, &[], Some(body)).await
    }
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.json::<Value, T>(Method::DELETE, path, &[], None).await
    }
    pub async fn bytes(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<Vec<u8>> {
        let retry = matches!(method, Method::GET | Method::HEAD | Method::OPTIONS);
        let mut request = self
            .inner
            .request(method, self.url(path, false)?)
            .query(query);
        if let Some(body) = body {
            request = request.json(body);
        }
        Ok(self.buffered(self.send(request, retry).await?).await?.body)
    }
    pub async fn stream(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<EventStream> {
        let mut request = self
            .inner
            .request(method, self.url(path, false)?)
            .query(query)
            .header("accept", "text/event-stream");
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await?;
        if !response.status().is_success() {
            self.buffered(response).await?;
            unreachable!("non-success is an error");
        }
        let is_sse = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.split(';')
                    .next()
                    .is_some_and(|v| v.trim().eq_ignore_ascii_case("text/event-stream"))
            });
        if !is_sse {
            return Err(Error::Protocol(
                "stream response must have text/event-stream content type".into(),
            ));
        }
        Ok(crate::sse::response_stream(
            response,
            self.config.max_event_bytes,
        ))
    }
    pub async fn multipart<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        form: reqwest::multipart::Form,
    ) -> Result<T> {
        decode(&self.multipart_bytes(method, path, query, form).await?)
    }
    pub async fn multipart_bytes(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        form: reqwest::multipart::Form,
    ) -> Result<Vec<u8>> {
        let response = self
            .inner
            .request(method, self.url(path, false)?)
            .query(query)
            .multipart(form)
            .send()
            .await?;
        Ok(self.buffered(response).await?.body)
    }
    /// Multipart request with an SSE response, without retries or reconnection.
    pub async fn multipart_stream(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        form: reqwest::multipart::Form,
    ) -> Result<EventStream> {
        let response = self
            .inner
            .request(method, self.url(path, false)?)
            .query(query)
            .header("accept", "text/event-stream")
            .multipart(form)
            .send()
            .await?;
        if !response.status().is_success() {
            self.buffered(response).await?;
            unreachable!("non-success is an error");
        }
        let is_sse = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.split(';')
                    .next()
                    .is_some_and(|v| v.trim().eq_ignore_ascii_case("text/event-stream"))
            });
        if !is_sse {
            return Err(Error::Protocol(
                "stream response must have text/event-stream content type".into(),
            ));
        }
        Ok(crate::sse::response_stream(
            response,
            self.config.max_event_bytes,
        ))
    }
    /// Send bytes to an API path or a same-origin resumable upload URL.
    /// Cross-origin upload URLs are rejected before credentials are attached.
    pub async fn raw(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        headers: &[(String, String)],
        body: Vec<u8>,
    ) -> Result<RawResponse> {
        let url = if path.contains("://") {
            self.same_origin(
                Url::parse(path).map_err(|_| Error::Config("invalid upload URL".into()))?,
            )?
        } else {
            self.url(path, false)?
        };
        let mut request = self.inner.request(method, url).query(query);
        for (name, value) in headers {
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| Error::Config("invalid header name".into()))?;
            if name == AUTHORIZATION
                || name == "host"
                || name == "x-goog-api-key"
                || self.config.key_header.as_ref() == Some(&name)
            {
                return Err(Error::Config(
                    "raw request cannot override authentication or Host".into(),
                ));
            }
            request = request.header(
                name,
                HeaderValue::from_str(value)
                    .map_err(|_| Error::Config("invalid header value".into()))?,
            );
        }
        self.buffered(request.body(body).send().await?).await
    }

    /// Download media with at most five redirects. The initial URL must be on the
    /// configured origin. All custom/authentication headers are permanently
    /// removed once a redirect leaves that origin. Redirect URLs require HTTPS,
    /// except between loopback test servers. Bodies use the configured size limit.
    pub async fn download(&self, path: &str) -> Result<Vec<u8>> {
        let mut url = if path.contains("://") {
            self.same_origin(
                Url::parse(path).map_err(|_| Error::Config("invalid download URL".into()))?,
            )?
        } else {
            self.url(path, false)?
        };
        let anonymous = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(self.config.timeout)
            .connect_timeout(Duration::from_secs(15))
            .build()?;
        let mut authenticated = true;
        for hop in 0..=5 {
            let client = if authenticated {
                &self.inner
            } else {
                &anonymous
            };
            let response = client.get(url.clone()).send().await?;
            if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                return Ok(self.buffered(response).await?.body);
            }
            if hop == 5 {
                return Err(Error::Protocol("download exceeded five redirects".into()));
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| {
                    Error::Protocol("download redirect missing a valid Location".into())
                })?;
            let next = url
                .join(location)
                .map_err(|_| Error::Protocol("invalid download redirect URL".into()))?;
            let test_http =
                crate::config::is_loopback(&self.config.base) && crate::config::is_loopback(&next);
            if (next.scheme() != "https" && !(next.scheme() == "http" && test_http))
                || !next.username().is_empty()
                || next.password().is_some()
                || next.fragment().is_some()
            {
                return Err(Error::Protocol(
                    "download redirect requires HTTPS and no embedded credentials or fragment"
                        .into(),
                ));
            }
            if next.origin() != self.config.base.origin() {
                authenticated = false;
            }
            url = next;
        }
        unreachable!("bounded redirect loop always returns")
    }
}
fn parse_retry_after(value: &str) -> Option<Duration> {
    value
        .parse::<u64>()
        .map(Duration::from_secs)
        .ok()
        .or_else(|| {
            httpdate::parse_http_date(value).ok().map(|date| {
                date.duration_since(SystemTime::now())
                    .unwrap_or(Duration::ZERO)
            })
        })
}
