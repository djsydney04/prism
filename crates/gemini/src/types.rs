//! Common wire types. Unknown provider fields survive deserialization in `extra`.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Additional request or response fields supported by the provider.
pub type Extra = Map<String, Value>;

/// A conversation turn containing text, media, or tool parts.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default)]
    pub parts: Vec<Part>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

impl Content {
    /// Make a user turn with a single text part.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Some("user".into()),
            parts: vec![Part::text(text)],
            ..Self::default()
        }
    }

    /// Make a model turn with a single text part.
    pub fn model(text: impl Into<String>) -> Self {
        Self {
            role: Some("model".into()),
            parts: vec![Part::text(text)],
            ..Self::default()
        }
    }
}

/// One content part. The API generally expects one payload field per part.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<Blob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<FileData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_response: Option<FunctionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

impl Part {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            ..Self::default()
        }
    }
    /// Inline media; `data` must already be base64 encoded.
    pub fn inline(mime_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            inline_data: Some(Blob {
                mime_type: mime_type.into(),
                data: data.into(),
                extra: Extra::new(),
            }),
            ..Self::default()
        }
    }
    /// Media previously uploaded through the Files API.
    pub fn file(mime_type: impl Into<String>, file_uri: impl Into<String>) -> Self {
        Self {
            file_data: Some(FileData {
                mime_type: mime_type.into(),
                file_uri: file_uri.into(),
                extra: Extra::new(),
            }),
            ..Self::default()
        }
    }
    /// Supply a tool result. Calling the tool itself is the application's job.
    pub fn function_response(name: impl Into<String>, response: Value) -> Self {
        Self {
            function_response: Some(FunctionResponse {
                name: name.into(),
                response,
                ..FunctionResponse::default()
            }),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Blob {
    pub mime_type: String,
    pub data: String,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileData {
    pub mime_type: String,
    pub file_uri: String,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub args: Value,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FunctionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub response: Value,
    #[serde(default, flatten)]
    pub extra: Extra,
}

/// Generation parameters; use `extra` for new or model-specific controls.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_count: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_json_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_modalities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speech_config: Option<Value>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetySetting {
    pub category: String,
    pub threshold: String,
    #[serde(default, flatten)]
    pub extra: Extra,
}

/// A `generateContent` request. `model` belongs in the method argument.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentRequest {
    pub contents: Vec<Content>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_config: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safety_settings: Vec<SafetySetting>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_content: Option<String>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

impl GenerateContentRequest {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            contents: vec![Content::user(text)],
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentResponse {
    #[serde(default)]
    pub candidates: Vec<Candidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_feedback: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<UsageMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

impl GenerateContentResponse {
    /// Text from the first candidate, excluding explicitly marked thought parts.
    pub fn text(&self) -> String {
        self.candidates
            .first()
            .and_then(|c| c.content.as_ref())
            .map(|c| {
                c.parts
                    .iter()
                    .filter(|p| p.thought != Some(true))
                    .filter_map(|p| p.text.as_deref())
                    .collect()
            })
            .unwrap_or_default()
    }
    /// Function calls from the first candidate. The SDK never executes them.
    pub fn function_calls(&self) -> impl Iterator<Item = &FunctionCall> {
        self.candidates
            .first()
            .and_then(|c| c.content.as_ref())
            .into_iter()
            .flat_map(|c| c.parts.iter())
            .filter_map(|p| p.function_call.as_ref())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<i64>,
    #[serde(default)]
    pub safety_ratings: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation_metadata: Option<Value>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_content_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thoughts_token_count: Option<i64>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedContentRequest {
    pub content: Content,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dimensionality: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed_content_config: Option<Value>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Embedding {
    #[serde(default)]
    pub values: Vec<f64>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EmbedContentResponse {
    pub embedding: Embedding,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BatchEmbedContentsResponse {
    #[serde(default)]
    pub embeddings: Vec<Embedding>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountTokensResponse {
    pub total_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_content_token_count: Option<i64>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_token_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_token_limit: Option<i64>,
    #[serde(default)]
    pub supported_generation_methods: Vec<String>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsResponse {
    #[serde(default)]
    pub models: Vec<Model>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesResponse {
    #[serde(default)]
    pub files: Vec<File>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UploadFileResponse {
    pub file: File,
    #[serde(default, flatten)]
    pub extra: Extra,
}

/// Long-running operation. Poll its `name` until `done` is true.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Operation {
    pub name: String,
    #[serde(default)]
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

/// Batch resource returned by update methods. Lifecycle polling returns `Operation`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_config: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_stats: Option<Value>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

pub type GenerateContentBatch = Batch;
pub type EmbedContentBatch = Batch;

/// Initial Gemini Live session configuration. Send through `Client::live_connect`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSetup {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<Content>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realtime_input_config: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_resumption: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window_compression: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_audio_transcription: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_audio_transcription: Option<Value>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

/// A Gemini Live server event. New event shapes survive in `extra`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveServerMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_complete: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_content: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_cancellation: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub go_away: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_resumption_update: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<Value>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

/// Interactions use snake_case fields, unlike the older model API.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InteractionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub input: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_interaction_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Interaction {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub steps: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    #[serde(default, flatten)]
    pub extra: Extra,
}

impl Interaction {
    /// Concatenate text in model-output steps, preserving other steps in `steps`.
    pub fn text(&self) -> String {
        self.steps
            .iter()
            .filter(|s| s["type"] == "model_output")
            .filter_map(|s| s["content"].as_array())
            .flatten()
            .filter(|p| p["type"] == "text")
            .filter_map(|p| p["text"].as_str())
            .collect()
    }
}
