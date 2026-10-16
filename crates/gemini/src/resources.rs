use crate::*;
use serde::Serialize;

/// Model inference, embeddings, token counting, batch submission, and prediction.
pub struct Models<'a>(pub(crate) &'a Client);

impl Models<'_> {
    pub async fn list(&self, query: &[(String, String)]) -> Result<ModelsResponse> {
        self.0.inner.get("/v1beta/models", query).await
    }
    pub async fn get(&self, id: &str) -> Result<Model> {
        self.0.inner.get(&format!("/v1beta/{}", model(id)?), &[]).await
    }
    pub async fn generate_content<B: Serialize + ?Sized>(&self, id: &str, body: &B) -> Result<GenerateContentResponse> {
        self.0.inner.post(&format!("/v1beta/{}:generateContent", model(id)?), body).await
    }
    /// SSE chunks decode as `GenerateContentResponse` using `SseEvent::json`.
    pub async fn stream_generate_content<B: Serialize + ?Sized>(&self, id: &str, body: &B) -> Result<EventStream> {
        let body = serde_json::to_value(body)?;
        self.0.inner.stream(Method::POST, &format!("/v1beta/{}:streamGenerateContent", model(id)?), &[("alt".into(), "sse".into())], Some(&body)).await
    }
    pub async fn count_tokens<B: Serialize + ?Sized>(&self, id: &str, body: &B) -> Result<CountTokensResponse> {
        self.0.inner.post(&format!("/v1beta/{}:countTokens", model(id)?), body).await
    }
    pub async fn embed_content<B: Serialize + ?Sized>(&self, id: &str, body: &B) -> Result<EmbedContentResponse> {
        self.0.inner.post(&format!("/v1beta/{}:embedContent", model(id)?), body).await
    }
    pub async fn batch_embed_contents<B: Serialize + ?Sized>(&self, id: &str, body: &B) -> Result<BatchEmbedContentsResponse> {
        self.0.inner.post(&format!("/v1beta/{}:batchEmbedContents", model(id)?), body).await
    }
    pub async fn batch_generate_content<B: Serialize + ?Sized>(&self, id: &str, body: &B) -> Result<Operation> {
        self.0.inner.post(&format!("/v1beta/{}:batchGenerateContent", model(id)?), body).await
    }
    pub async fn async_batch_embed_content<B: Serialize + ?Sized>(&self, id: &str, body: &B) -> Result<Operation> {
        self.0.inner.post(&format!("/v1beta/{}:asyncBatchEmbedContent", model(id)?), body).await
    }
    /// Specialized models, for example image generation.
    pub async fn predict<B: Serialize + ?Sized>(&self, id: &str, body: &B) -> Result<Value> {
        self.0.inner.post(&format!("/v1beta/{}:predict", model(id)?), body).await
    }
    /// Specialized asynchronous generation, for example video generation.
    pub async fn predict_long_running<B: Serialize + ?Sized>(&self, id: &str, body: &B) -> Result<Operation> {
        self.0.inner.post(&format!("/v1beta/{}:predictLongRunning", model(id)?), body).await
    }
}

/// Server-side cached content. Supply `updateMask` when updating expiration.
pub struct CachedContents<'a>(pub(crate) &'a Client);
impl CachedContents<'_> {
    pub async fn create<B: Serialize + ?Sized>(&self, body: &B) -> Result<Value> {
        self.0.inner.post("/v1beta/cachedContents", body).await
    }
    pub async fn list(&self, query: &[(String, String)]) -> Result<Value> {
        self.0.inner.get("/v1beta/cachedContents", query).await
    }
    pub async fn get(&self, id: &str) -> Result<Value> {
        self.0.inner.get(&format!("/v1beta/{}", name(id, "cachedContents")?), &[]).await
    }
    pub async fn delete(&self, id: &str) -> Result<Value> {
        self.0.inner.delete(&format!("/v1beta/{}", name(id, "cachedContents")?)).await
    }
    pub async fn update<B: Serialize + ?Sized>(&self, id: &str, body: &B, query: &[(String, String)]) -> Result<Value> {
        self.0.inner.json(Method::PATCH, &format!("/v1beta/{}", name(id, "cachedContents")?), query, Some(body)).await
    }
}

/// Asynchronous batch lifecycle. Submission is on `Models`.
pub struct Batches<'a>(pub(crate) &'a Client);
impl Batches<'_> {
    pub async fn list(&self, query: &[(String, String)]) -> Result<Value> {
        self.0.inner.get("/v1beta/batches", query).await
    }
    pub async fn get(&self, id: &str) -> Result<Operation> {
        self.0.inner.get(&format!("/v1beta/{}", name(id, "batches")?), &[]).await
    }
    pub async fn delete(&self, id: &str) -> Result<Value> {
        self.0.inner.delete(&format!("/v1beta/{}", name(id, "batches")?)).await
    }
    pub async fn cancel(&self, id: &str) -> Result<Value> {
        self.0.inner.post(&format!("/v1beta/{}:cancel", name(id, "batches")?), &json!({})).await
    }
    pub async fn update_generate_content<B: Serialize + ?Sized>(&self, id: &str, body: &B, query: &[(String, String)]) -> Result<Operation> {
        self.0.inner.json(Method::PATCH, &format!("/v1beta/{}:updateGenerateContentBatch", name(id, "batches")?), query, Some(body)).await
    }
    pub async fn update_embed_content<B: Serialize + ?Sized>(&self, id: &str, body: &B, query: &[(String, String)]) -> Result<Operation> {
        self.0.inner.json(Method::PATCH, &format!("/v1beta/{}:updateEmbedContentBatch", name(id, "batches")?), query, Some(body)).await
    }
}

/// Poll operation names returned by generation, batches, and file search uploads.
pub struct Operations<'a>(pub(crate) &'a Client);
impl Operations<'_> {
    pub async fn get(&self, operation_name: &str) -> Result<Operation> {
        let valid = [
            &["batches", "*"][..],
            &["models", "*", "operations", "*"][..],
            &["fileSearchStores", "*", "operations", "*"][..],
            &["fileSearchStores", "*", "upload", "operations", "*"][..],
            &["generatedFiles", "*", "operations", "*"][..],
            &["operations", "*"][..],
        ].iter().find_map(|pattern| nested_name(operation_name, pattern).ok())
            .ok_or_else(|| Error::Config("invalid Gemini operation name".into()))?;
        self.0.inner.get(&format!("/v1beta/{valid}"), &[]).await
    }
}

/// A resumable session URL. Treat it as a secret. Debug output omits the URL.
#[derive(Clone)]
pub struct UploadSession { url: String }
impl std::fmt::Debug for UploadSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("UploadSession { url: [REDACTED] }") }
}
impl UploadSession {
    /// Persist only in secure storage if resuming after a process restart.
    pub fn url(&self) -> &str { &self.url }
    /// Restored URLs are validated against the configured origin on every request.
    pub fn from_url(url: impl Into<String>) -> Self { Self { url: url.into() } }
}

async fn start_upload(client: &Client, path: &str, metadata: &Value, mime_type: &str, length: u64) -> Result<UploadSession> {
    let response = client.inner.raw(Method::POST, path, &[], &[
        ("content-type".into(), "application/json".into()),
        ("x-goog-upload-protocol".into(), "resumable".into()),
        ("x-goog-upload-command".into(), "start".into()),
        ("x-goog-upload-header-content-length".into(), length.to_string()),
        ("x-goog-upload-header-content-type".into(), mime_type.into()),
    ], serde_json::to_vec(metadata)?).await?;
    let url = response.headers.get("x-goog-upload-url").and_then(|h| h.to_str().ok())
        .ok_or_else(|| Error::Protocol("Gemini did not return a resumable upload URL".into()))?;
    Ok(UploadSession::from_url(url))
}

async fn upload_chunk(client: &Client, session: &UploadSession, offset: u64, bytes: Vec<u8>, finalize: bool) -> Result<RawResponse> {
    client.inner.raw(Method::POST, &session.url, &[], &[
        ("x-goog-upload-offset".into(), offset.to_string()),
        ("x-goog-upload-command".into(), if finalize { "upload, finalize" } else { "upload" }.into()),
    ], bytes).await
}

/// Files, resumable media upload, generated-media download, and GCS registration.
pub struct Files<'a>(pub(crate) &'a Client);
impl Files<'_> {
    pub async fn list(&self, query: &[(String, String)]) -> Result<FilesResponse> {
        self.0.inner.get("/v1beta/files", query).await
    }
    pub async fn get(&self, id: &str) -> Result<File> {
        self.0.inner.get(&format!("/v1beta/{}", name(id, "files")?), &[]).await
    }
    pub async fn delete(&self, id: &str) -> Result<Value> {
        self.0.inner.delete(&format!("/v1beta/{}", name(id, "files")?)).await
    }
    pub async fn register<B: Serialize + ?Sized>(&self, body: &B) -> Result<FilesResponse> {
        self.0.inner.post("/v1beta/files:register", body).await
    }
    /// `metadata` is the outer object, for example `{"file":{"displayName":"Notes"}}`.
    pub async fn start_upload(&self, metadata: &Value, mime_type: &str, length: u64) -> Result<UploadSession> {
        start_upload(self.0, "/upload/v1beta/files", metadata, mime_type, length).await
    }
    /// Send a chunk. Finalized responses decode as `UploadFileResponse`.
    pub async fn upload_chunk(&self, session: &UploadSession, offset: u64, bytes: Vec<u8>, finalize: bool) -> Result<RawResponse> {
        upload_chunk(self.0, session, offset, bytes, finalize).await
    }
    /// Query committed offset after an interrupted transfer; inspect response headers.
    pub async fn query_upload(&self, session: &UploadSession) -> Result<RawResponse> {
        self.0.inner.raw(Method::POST, &session.url, &[], &[("x-goog-upload-command".into(), "query".into())], Vec::new()).await
    }
    pub async fn cancel_upload(&self, session: &UploadSession) -> Result<RawResponse> {
        self.0.inner.raw(Method::POST, &session.url, &[], &[("x-goog-upload-command".into(), "cancel".into())], Vec::new()).await
    }
    /// Upload in one resumable transfer. Bytes stay in memory; chunk large files manually.
    pub async fn upload(&self, metadata: &Value, mime_type: &str, bytes: Vec<u8>) -> Result<UploadFileResponse> {
        let session = self.start_upload(metadata, mime_type, bytes.len() as u64).await?;
        self.upload_chunk(&session, 0, bytes, true).await?.json()
    }
    /// Download a provider-returned media URI. The transport rejects foreign origins.
    /// User-uploaded files are generally not downloadable; generated media may be.
    pub async fn download(&self, download_uri: &str) -> Result<Vec<u8>> {
        Ok(self.0.inner.raw(Method::GET, download_uri, &[("alt".into(), "media".into())], &[], Vec::new()).await?.body)
    }
}

/// File search stores, documents, imports, and resumable document upload.
pub struct FileSearchStores<'a>(pub(crate) &'a Client);
impl FileSearchStores<'_> {
    pub async fn create<B: Serialize + ?Sized>(&self, body: &B) -> Result<Value> {
        self.0.inner.post("/v1beta/fileSearchStores", body).await
    }
    pub async fn list(&self, query: &[(String, String)]) -> Result<Value> {
        self.0.inner.get("/v1beta/fileSearchStores", query).await
    }
    pub async fn get(&self, id: &str) -> Result<Value> {
        self.0.inner.get(&format!("/v1beta/{}", name(id, "fileSearchStores")?), &[]).await
    }
    pub async fn delete(&self, id: &str, force: bool) -> Result<Value> {
        self.0.inner.json::<Value, Value>(Method::DELETE, &format!("/v1beta/{}", name(id, "fileSearchStores")?), &[("force".into(), force.to_string())], None).await
    }
    pub async fn import_file<B: Serialize + ?Sized>(&self, id: &str, body: &B) -> Result<Operation> {
        self.0.inner.post(&format!("/v1beta/{}:importFile", name(id, "fileSearchStores")?), body).await
    }
    pub async fn list_documents(&self, store: &str, query: &[(String, String)]) -> Result<Value> {
        self.0.inner.get(&format!("/v1beta/{}/documents", name(store, "fileSearchStores")?), query).await
    }
    pub async fn get_document(&self, document_name: &str) -> Result<Value> {
        let path = nested_name(document_name, &["fileSearchStores", "*", "documents", "*"])?;
        self.0.inner.get(&format!("/v1beta/{path}"), &[]).await
    }
    pub async fn delete_document(&self, document_name: &str, force: bool) -> Result<Value> {
        let path = nested_name(document_name, &["fileSearchStores", "*", "documents", "*"])?;
        self.0.inner.json::<Value, Value>(Method::DELETE, &format!("/v1beta/{path}"), &[("force".into(), force.to_string())], None).await
    }
    pub async fn start_upload(&self, store: &str, metadata: &Value, mime_type: &str, length: u64) -> Result<UploadSession> {
        start_upload(self.0, &format!("/upload/v1beta/{}:uploadToFileSearchStore", name(store, "fileSearchStores")?), metadata, mime_type, length).await
    }
    pub async fn upload_chunk(&self, session: &UploadSession, offset: u64, bytes: Vec<u8>, finalize: bool) -> Result<RawResponse> {
        upload_chunk(self.0, session, offset, bytes, finalize).await
    }
    pub async fn upload(&self, store: &str, metadata: &Value, mime_type: &str, bytes: Vec<u8>) -> Result<Operation> {
        let session = self.start_upload(store, metadata, mime_type, bytes.len() as u64).await?;
        self.upload_chunk(&session, 0, bytes, true).await?.json()
    }
}

/// Stateful interactions, background work, and SSE creation/resumption.
pub struct Interactions<'a>(pub(crate) &'a Client);
impl Interactions<'_> {
    pub async fn create<B: Serialize + ?Sized>(&self, body: &B) -> Result<Interaction> {
        self.0.inner.post("/v1beta/interactions", body).await
    }
    pub async fn get(&self, id: &str, query: &[(String, String)]) -> Result<Interaction> {
        self.0.inner.get(&format!("/v1beta/{}", name(id, "interactions")?), query).await
    }
    pub async fn delete(&self, id: &str) -> Result<Value> {
        self.0.inner.delete(&format!("/v1beta/{}", name(id, "interactions")?)).await
    }
    pub async fn cancel(&self, id: &str) -> Result<Interaction> {
        self.0.inner.post(&format!("/v1beta/{}/cancel", name(id, "interactions")?), &json!({})).await
    }
    /// Adds `stream: true` to an object body and yields the provider's SSE events.
    pub async fn stream<B: Serialize + ?Sized>(&self, body: &B) -> Result<EventStream> {
        let mut value = serde_json::to_value(body)?;
        let object = value.as_object_mut().ok_or_else(|| Error::Config("interaction body must be an object".into()))?;
        object.insert("stream".into(), Value::Bool(true));
        self.0.inner.stream(Method::POST, "/v1beta/interactions", &[], Some(&value)).await
    }
    /// Resume an interaction's event stream. Supply `last_event_id` in `query` if needed.
    pub async fn resume_stream(&self, id: &str, query: &[(String, String)]) -> Result<EventStream> {
        let mut query = query.to_vec();
        query.retain(|(k, _)| k != "stream");
        query.push(("stream".into(), "true".into()));
        self.0.inner.stream(Method::GET, &format!("/v1beta/{}", name(id, "interactions")?), &query, None).await
    }
}

macro_rules! collection {
    ($service:ident, $collection:literal, $doc:literal) => {
        #[doc = $doc]
        pub struct $service<'a>(pub(crate) &'a Client);
        impl $service<'_> {
            pub async fn create<B: Serialize + ?Sized>(&self, body: &B) -> Result<Value> {
                self.0.inner.post(concat!("/v1beta/", $collection), body).await
            }
            pub async fn list(&self, query: &[(String, String)]) -> Result<Value> {
                self.0.inner.get(concat!("/v1beta/", $collection), query).await
            }
            pub async fn get(&self, id: &str) -> Result<Value> {
                self.0.inner.get(&format!("/v1beta/{}", name(id, $collection)?), &[]).await
            }
            pub async fn delete(&self, id: &str) -> Result<Value> {
                self.0.inner.delete(&format!("/v1beta/{}", name(id, $collection)?)).await
            }
        }
    };
}
collection!(Agents, "agents", "Managed agent resources.");
collection!(Webhooks, "webhooks", "Webhook registration and lifecycle. No signature verification helper is included.");
collection!(Triggers, "triggers", "Scheduled agent triggers and executions.");
collection!(Environments, "environments", "Managed agent environment resources and workspace files.");
collection!(Voices, "voices", "Custom and prebuilt voice resources.");
collection!(Credentials, "credentials", "Provider-managed credential resources.");

macro_rules! patch_collection {
    ($service:ident, $collection:literal) => {
        impl $service<'_> {
            pub async fn update<B: Serialize + ?Sized>(&self, id: &str, body: &B, query: &[(String, String)]) -> Result<Value> {
                self.0.inner.json(Method::PATCH, &format!("/v1beta/{}", name(id, $collection)?), query, Some(body)).await
            }
        }
    };
}
patch_collection!(Webhooks, "webhooks");
patch_collection!(Triggers, "triggers");
patch_collection!(Credentials, "credentials");

impl Webhooks<'_> {
    pub async fn ping(&self, id: &str) -> Result<Value> {
        self.0.inner.post(&format!("/v1beta/{}:ping", name(id, "webhooks")?), &json!({})).await
    }
    pub async fn rotate_signing_secret(&self, id: &str) -> Result<Value> {
        self.0.inner.post(&format!("/v1beta/{}:rotateSigningSecret", name(id, "webhooks")?), &json!({})).await
    }
}
impl Triggers<'_> {
    pub async fn run<B: Serialize + ?Sized>(&self, id: &str, body: &B) -> Result<Value> {
        self.0.inner.post(&format!("/v1beta/{}/executions", name(id, "triggers")?), body).await
    }
    pub async fn executions(&self, id: &str, query: &[(String, String)]) -> Result<Value> {
        self.0.inner.get(&format!("/v1beta/{}/executions", name(id, "triggers")?), query).await
    }
}
impl Environments<'_> {
    pub async fn files(&self, environment: &str, file_path: &str, query: &[(String, String)]) -> Result<Value> {
        let path = file_path.split('/').map(segment).collect::<Result<Vec<_>>>()?.join("/");
        self.0.inner.get(&format!("/v1beta/{}/files/{path}", name(environment, "environments")?), query).await
    }
    /// Start a resumable workspace-file upload. Use `files().upload_chunk` to transfer.
    pub async fn start_file_upload(&self, environment: &str, file_path: &str, mime_type: &str, length: u64) -> Result<UploadSession> {
        let path = file_path.split('/').map(segment).collect::<Result<Vec<_>>>()?.join("/");
        let response = self.0.inner.raw(Method::PUT,
            &format!("/upload/v1beta/{}/files/{path}", name(environment, "environments")?),
            &[], &[("x-goog-upload-protocol".into(), "resumable".into()), ("x-goog-upload-command".into(), "start".into()),
                ("x-goog-upload-header-content-length".into(), length.to_string()), ("x-goog-upload-header-content-type".into(), mime_type.into())], Vec::new()).await?;
        let url = response.headers.get("x-goog-upload-url").and_then(|v| v.to_str().ok())
            .ok_or_else(|| Error::Protocol("Gemini did not return a resumable upload URL".into()))?;
        Ok(UploadSession::from_url(url))
    }
}
