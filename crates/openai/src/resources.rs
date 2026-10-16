use serde::Serialize;
use serde_json::{json, Value};
use sydney_ai_core::{
    reqwest::multipart::{Form, Part},
    Client, Error, EventStream, Method, Resource, Result, WebSocketSession,
};

use crate::types::*;

fn streaming_body<B: Serialize + ?Sized>(body: &B, stream: bool) -> Result<Value> {
    let mut value = serde_json::to_value(body).map_err(Error::Serialization)?;
    value
        .as_object_mut()
        .ok_or_else(|| Error::Config("OpenAI streaming bodies must be JSON objects".into()))?
        .insert("stream".into(), Value::Bool(stream));
    Ok(value)
}

macro_rules! handle {
    ($name:ident, $path:literal) => {
        #[derive(Clone, Copy, Debug)]
        pub struct $name<'a> {
            client: &'a Client,
        }

        impl<'a> $name<'a> {
            pub(crate) fn new(client: &'a Client) -> Self {
                Self { client }
            }
            /// Generic access to all operations and newly added parameters.
            pub fn resource(&self) -> Resource {
                self.client.resource($path)
            }
        }
    };
}

handle!(Responses, "responses");
handle!(Chat, "chat/completions");
handle!(Embeddings, "embeddings");
handle!(Audio, "audio");
handle!(Images, "images");
handle!(Files, "files");
handle!(Moderations, "moderations");

impl Responses<'_> {
    /// Open a persistent Responses WebSocket. Send `response.create` events
    /// without the HTTP-only `stream` and `background` fields. Model events and
    /// new event types are available through the session's JSON methods.
    pub async fn connect(&self) -> Result<WebSocketSession> {
        self.client.websocket("responses", &[]).await
    }

    pub async fn create(&self, body: &ResponseRequest) -> Result<Response> {
        self.create_json(body).await
    }

    /// Use any serializable request shape, preserving all documented parameters.
    pub async fn create_json<B: Serialize + ?Sized>(&self, body: &B) -> Result<Response> {
        self.client
            .post("responses", &streaming_body(body, false)?)
            .await
    }

    /// Start server-sent events; forces `stream: true` regardless of extensions.
    pub async fn stream<B: Serialize + ?Sized>(&self, body: &B) -> Result<EventStream> {
        self.client
            .stream(
                Method::POST,
                "responses",
                &[],
                Some(&streaming_body(body, true)?),
            )
            .await
    }

    pub async fn retrieve(&self, id: &str) -> Result<Response> {
        self.resource().child(id).get(&[]).await
    }

    pub async fn retrieve_with_query(
        &self,
        id: &str,
        query: &[(String, String)],
    ) -> Result<Response> {
        self.resource().child(id).get(query).await
    }

    /// Resume/retrieve the event stream of a stored background response.
    pub async fn retrieve_stream(
        &self,
        id: &str,
        query: &[(String, String)],
    ) -> Result<EventStream> {
        let resource = self.resource().child(id);
        let mut query = query.to_vec();
        query.retain(|(key, _)| key != "stream");
        query.push(("stream".into(), "true".into()));
        self.client
            .stream(Method::GET, resource.path(), &query, None)
            .await
    }

    pub async fn delete(&self, id: &str) -> Result<Value> {
        self.resource().child(id).delete().await
    }
    pub async fn cancel(&self, id: &str) -> Result<Response> {
        self.resource()
            .child(id)
            .action("cancel")
            .create(&json!({}))
            .await
    }
    pub async fn compact<B: Serialize + ?Sized>(&self, body: &B) -> Result<Value> {
        self.resource().action("compact").create(body).await
    }
    pub async fn count_input_tokens<B: Serialize + ?Sized>(&self, body: &B) -> Result<Value> {
        self.resource().action("input_tokens").create(body).await
    }
    pub async fn input_items(&self, id: &str, query: &[(String, String)]) -> Result<List<Value>> {
        self.resource()
            .child(id)
            .child("input_items")
            .get(query)
            .await
    }
}

impl Chat<'_> {
    pub async fn create(&self, body: &ChatCompletionRequest) -> Result<ChatCompletion> {
        self.create_json(body).await
    }
    pub async fn create_json<B: Serialize + ?Sized>(&self, body: &B) -> Result<ChatCompletion> {
        self.client
            .post("chat/completions", &streaming_body(body, false)?)
            .await
    }
    pub async fn stream<B: Serialize + ?Sized>(&self, body: &B) -> Result<EventStream> {
        self.client
            .stream(
                Method::POST,
                "chat/completions",
                &[],
                Some(&streaming_body(body, true)?),
            )
            .await
    }
    pub async fn retrieve(&self, id: &str) -> Result<ChatCompletion> {
        self.resource().child(id).get(&[]).await
    }
    pub async fn list(&self, query: &[(String, String)]) -> Result<List<ChatCompletion>> {
        self.resource().get(query).await
    }
    pub async fn update<B: Serialize + ?Sized>(
        &self,
        id: &str,
        body: &B,
    ) -> Result<ChatCompletion> {
        self.resource().child(id).update(body).await
    }
    pub async fn delete(&self, id: &str) -> Result<Value> {
        self.resource().child(id).delete().await
    }
    pub async fn messages(&self, id: &str, query: &[(String, String)]) -> Result<List<Value>> {
        self.resource().child(id).child("messages").get(query).await
    }
}

impl Embeddings<'_> {
    pub async fn create(&self, body: &EmbeddingRequest) -> Result<EmbeddingResponse> {
        self.create_json(body).await
    }
    pub async fn create_json<B: Serialize + ?Sized>(&self, body: &B) -> Result<EmbeddingResponse> {
        self.client.post("embeddings", body).await
    }
}

impl Audio<'_> {
    /// Return speech bytes in the requested encoding.
    pub async fn speech(&self, body: &SpeechRequest) -> Result<Vec<u8>> {
        let body = serde_json::to_value(body).map_err(Error::Serialization)?;
        self.client
            .bytes(Method::POST, "audio/speech", &[], Some(&body))
            .await
    }

    pub async fn transcribe(
        &self,
        filename: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
        model: impl Into<String>,
    ) -> Result<Transcription> {
        self.transcribe_form(
            Form::new()
                .text("model", model.into())
                .part("file", Part::bytes(bytes.into()).file_name(filename.into())),
        )
        .await
    }

    /// For timestamps, languages, diarization, prompts, and other multipart fields.
    /// This method expects a JSON response; leave `response_format` unset or JSON.
    pub async fn transcribe_form(&self, form: Form) -> Result<Transcription> {
        self.client
            .multipart(Method::POST, "audio/transcriptions", &[], form)
            .await
    }

    /// Text, SRT, VTT, or other non-JSON transcription formats.
    pub async fn transcribe_bytes(&self, form: Form) -> Result<Vec<u8>> {
        self.client
            .multipart_bytes(Method::POST, "audio/transcriptions", &[], form)
            .await
    }

    /// The multipart form must include `stream=true` and a supported model.
    pub async fn transcribe_stream(&self, form: Form) -> Result<EventStream> {
        self.client
            .multipart_stream(Method::POST, "audio/transcriptions", &[], form)
            .await
    }

    pub async fn translate(
        &self,
        filename: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
        model: impl Into<String>,
    ) -> Result<Transcription> {
        self.translate_form(
            Form::new()
                .text("model", model.into())
                .part("file", Part::bytes(bytes.into()).file_name(filename.into())),
        )
        .await
    }

    /// Customize JSON translation requests with additional multipart fields.
    pub async fn translate_form(&self, form: Form) -> Result<Transcription> {
        self.client
            .multipart(Method::POST, "audio/translations", &[], form)
            .await
    }

    /// Text, SRT, VTT, or other non-JSON translation formats.
    pub async fn translate_bytes(&self, form: Form) -> Result<Vec<u8>> {
        self.client
            .multipart_bytes(Method::POST, "audio/translations", &[], form)
            .await
    }

    /// Custom voices, voice consents, and future audio resources.
    pub fn voices(&self) -> Resource {
        self.resource().child("voices")
    }
    pub fn voice_consents(&self) -> Resource {
        self.resource().child("voice_consents")
    }
}

impl Images<'_> {
    pub async fn generate(&self, body: &ImageRequest) -> Result<ImageResponse> {
        self.client
            .post("images/generations", &streaming_body(body, false)?)
            .await
    }
    pub async fn stream<B: Serialize + ?Sized>(&self, body: &B) -> Result<EventStream> {
        self.client
            .stream(
                Method::POST,
                "images/generations",
                &[],
                Some(&streaming_body(body, true)?),
            )
            .await
    }
    /// Image edit requests may use file IDs, URLs, or base64 data in JSON.
    pub async fn edit_json<B: Serialize + ?Sized>(&self, body: &B) -> Result<ImageResponse> {
        self.client
            .post("images/edits", &streaming_body(body, false)?)
            .await
    }
    /// File image edits with additional fields, optional mask, and multiple images.
    pub async fn edit(&self, form: Form) -> Result<ImageResponse> {
        self.client
            .multipart(Method::POST, "images/edits", &[], form)
            .await
    }
    /// The multipart form must include `stream=true` and a supported model.
    pub async fn edit_stream(&self, form: Form) -> Result<EventStream> {
        self.client
            .multipart_stream(Method::POST, "images/edits", &[], form)
            .await
    }
    pub async fn variation(&self, form: Form) -> Result<ImageResponse> {
        self.client
            .multipart(Method::POST, "images/variations", &[], form)
            .await
    }
}

impl Files<'_> {
    /// Upload bytes with a caller-provided filename and a documented purpose.
    pub async fn upload(
        &self,
        filename: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
        purpose: impl Into<String>,
    ) -> Result<File> {
        self.upload_form(
            Form::new()
                .text("purpose", purpose.into())
                .part("file", Part::bytes(bytes.into()).file_name(filename.into())),
        )
        .await
    }
    pub async fn upload_form(&self, form: Form) -> Result<File> {
        self.client
            .multipart(Method::POST, "files", &[], form)
            .await
    }
    pub async fn retrieve(&self, id: &str) -> Result<File> {
        self.resource().child(id).get(&[]).await
    }
    pub async fn list(&self, query: &[(String, String)]) -> Result<List<File>> {
        self.resource().get(query).await
    }
    pub async fn delete(&self, id: &str) -> Result<Value> {
        self.resource().child(id).delete().await
    }
    pub async fn content(&self, id: &str) -> Result<Vec<u8>> {
        let resource = self.resource().child(id).child("content");
        self.client
            .bytes(Method::GET, resource.path(), &[], None)
            .await
    }
}

impl Moderations<'_> {
    pub async fn create(&self, body: &ModerationRequest) -> Result<ModerationResponse> {
        self.client.post("moderations", body).await
    }
}
