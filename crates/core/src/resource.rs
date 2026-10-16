use crate::{Client, EventStream, Method, Result, Value};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Encode one opaque path segment; IDs cannot inject query strings or subresources.
pub fn encode_segment(id: &str) -> String {
    const SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    percent_encoding::utf8_percent_encode(id, SEGMENT).to_string()
}

/// Owned, cloneable endpoint handle. Every operation uses the configured authenticated origin.
#[derive(Clone, Debug)]
pub struct Resource {
    pub(crate) client: Client,
    pub(crate) path: String,
}
impl Resource {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn child(&self, id: &str) -> Self {
        Self {
            client: self.client.clone(),
            path: format!("{}/{}", self.path.trim_end_matches('/'), encode_segment(id)),
        }
    }
    pub fn action(&self, name: &str) -> Self {
        self.child(name)
    }
    pub async fn get<T: DeserializeOwned>(&self, query: &[(String, String)]) -> Result<T> {
        self.client.get(&self.path, query).await
    }
    pub async fn list<T: DeserializeOwned>(&self, query: &[(String, String)]) -> Result<T> {
        self.get(query).await
    }
    pub async fn create<B: Serialize + ?Sized, T: DeserializeOwned>(&self, body: &B) -> Result<T> {
        self.client.post(&self.path, body).await
    }
    pub async fn update<B: Serialize + ?Sized, T: DeserializeOwned>(&self, body: &B) -> Result<T> {
        self.create(body).await
    }
    pub async fn patch<B: Serialize + ?Sized, T: DeserializeOwned>(&self, body: &B) -> Result<T> {
        self.client.patch(&self.path, body).await
    }
    pub async fn delete<T: DeserializeOwned>(&self) -> Result<T> {
        self.client.delete(&self.path).await
    }
    pub async fn request<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        method: Method,
        query: &[(String, String)],
        body: Option<&B>,
    ) -> Result<T> {
        self.client.json(method, &self.path, query, body).await
    }
    pub async fn stream(&self, body: &Value) -> Result<EventStream> {
        self.client
            .stream(Method::POST, &self.path, &[], Some(body))
            .await
    }
    pub async fn multipart<T: DeserializeOwned>(
        &self,
        form: reqwest::multipart::Form,
    ) -> Result<T> {
        self.client
            .multipart(Method::POST, &self.path, &[], form)
            .await
    }
}

/// OpenAI-style cursor page. Callers control iteration and can persist their cursor.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page<T> {
    pub data: Vec<T>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub first_id: Option<String>,
    #[serde(default)]
    pub last_id: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}
impl<T> Page<T> {
    /// Return the next cursor, rejecting malformed pages to avoid endless loops.
    pub fn next_after(&self) -> Result<Option<&str>> {
        if !self.has_more {
            return Ok(None);
        }
        self.last_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .map(Some)
            .ok_or_else(|| {
                crate::Error::Protocol("page has_more=true without a last_id cursor".into())
            })
    }
}
