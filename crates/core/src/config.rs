use crate::{Error, Result};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION},
    Url,
};
use std::{fmt, time::Duration};

/// Transport settings. Debug output never includes headers or credentials.
#[derive(Clone)]
pub struct Config {
    pub(crate) base: Url,
    pub(crate) key: String,
    pub(crate) key_header: Option<HeaderName>,
    pub(crate) headers: HeaderMap,
    pub(crate) timeout: Duration,
    pub(crate) retries: u32,
    pub(crate) max_response_bytes: usize,
    pub(crate) max_event_bytes: usize,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut safe_base = self.base.clone();
        let _ = safe_base.set_username("");
        let _ = safe_base.set_password(None);
        safe_base.set_query(None);
        safe_base.set_fragment(None);
        f.debug_struct("Config")
            .field("base_url", &safe_base.as_str())
            .field("credentials", &"[REDACTED]")
            .field("timeout", &self.timeout)
            .field("max_retries", &self.retries)
            .finish_non_exhaustive()
    }
}

impl Config {
    /// Configure a provider base URL and key. Validation happens before any request.
    pub fn new(base_url: impl AsRef<str>, api_key: impl Into<String>) -> Self {
        // Invalid URLs are retained as an invalid scheme so Client::new reports an error.
        let base = Url::parse(base_url.as_ref())
            .unwrap_or_else(|_| Url::parse("invalid://configuration").expect("constant URL"));
        Self {
            base,
            key: api_key.into(),
            key_header: None,
            headers: HeaderMap::new(),
            timeout: Duration::from_secs(120),
            retries: 2,
            max_response_bytes: 64 * 1024 * 1024,
            max_event_bytes: 8 * 1024 * 1024,
        }
    }
    /// Read a key from the named environment variable. No implicit dotenv loading.
    pub fn from_env(base_url: impl AsRef<str>, env_var: &str) -> Result<Self> {
        let key =
            std::env::var(env_var).map_err(|_| Error::Config(format!("{env_var} is not set")))?;
        Ok(Self::new(base_url, key))
    }
    /// Replace the base URL. HTTPS is required except for loopback test servers.
    pub fn base_url(mut self, base_url: impl AsRef<str>) -> Result<Self> {
        self.base =
            Url::parse(base_url.as_ref()).map_err(|_| Error::Config("invalid base URL".into()))?;
        Ok(self)
    }
    /// Authenticate using a key header instead of Bearer authentication.
    pub fn with_api_key_header(mut self, name: &str) -> Result<Self> {
        self.key_header = Some(
            HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| Error::Config("invalid API key header".into()))?,
        );
        Ok(self)
    }
    /// Authenticate with a caller-managed Bearer token instead of an API-key header.
    /// Token refresh is the calling application's responsibility.
    pub fn with_bearer_authentication(mut self) -> Self {
        self.key_header = None;
        self
    }
    /// Add a default header such as OpenAI-Project. Authentication cannot be overridden.
    pub fn header(mut self, name: &str, value: &str) -> Result<Self> {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| Error::Config("invalid header name".into()))?;
        if name == AUTHORIZATION || name == "host" || name == "x-goog-api-key" {
            return Err(Error::Config(
                "use the API key configuration for authentication headers".into(),
            ));
        }
        let mut value = HeaderValue::from_str(value)
            .map_err(|_| Error::Config("invalid header value".into()))?;
        value.set_sensitive(true);
        self.headers.insert(name, value);
        Ok(self)
    }
    /// Set the total request timeout (including streams).
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    /// Maximum retries after the initial attempt, only for GET/HEAD/OPTIONS.
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.retries = retries.min(10);
        self
    }
    /// Bound buffered response bodies (default 64 MiB).
    pub fn max_response_bytes(mut self, limit: usize) -> Self {
        self.max_response_bytes = limit;
        self
    }
    /// Bound individual SSE events (default 8 MiB).
    pub fn max_event_bytes(mut self, limit: usize) -> Self {
        self.max_event_bytes = limit;
        self
    }
    pub(crate) fn validate(&mut self) -> Result<()> {
        let loopback = is_loopback(&self.base);
        if self.base.scheme() != "https" && !(self.base.scheme() == "http" && loopback) {
            return Err(Error::Config(
                "base URL must use HTTPS (HTTP allowed only on loopback)".into(),
            ));
        }
        if self.base.host_str().is_none()
            || !self.base.username().is_empty()
            || self.base.password().is_some()
            || self.base.query().is_some()
            || self.base.fragment().is_some()
        {
            return Err(Error::Config(
                "base URL must have a host and no credentials, query, or fragment".into(),
            ));
        }
        if self.key.trim().is_empty() {
            return Err(Error::Config("API key must not be empty".into()));
        }
        if self.timeout.is_zero() || self.max_response_bytes == 0 || self.max_event_bytes == 0 {
            return Err(Error::Config(
                "timeouts and size limits must be positive".into(),
            ));
        }
        if !self.base.path().ends_with('/') {
            let path = format!("{}/", self.base.path());
            self.base.set_path(&path);
        }
        Ok(())
    }
}

pub(crate) fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(h)) => h == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}
