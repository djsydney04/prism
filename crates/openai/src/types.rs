//! Extensible types for common OpenAI operations.
//!
//! Model identifiers and protocol unions stay open so a provider addition does
//! not require an SDK update. `extra` fields round-trip new API properties.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Additional provider-defined request or response properties.
pub type Extra = BTreeMap<String, Value>;

macro_rules! extra_builder {
    ($($name:ident),+ $(,)?) => {$(
        impl $name {
            /// Add a documented property not represented by a dedicated field.
            ///
            /// Use dedicated fields for known keys to avoid duplicate JSON keys.
            pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
                self.extra.insert(key.into(), value.into());
                self
            }
        }
    )+};
}

/// Input to `POST /responses`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResponseRequest {
    pub model: String,
    pub input: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl ResponseRequest {
    /// Accepts a plain text input or any documented input item array.
    pub fn new(model: impl Into<String>, input: impl Into<Value>) -> Self {
        Self {
            model: model.into(),
            input: input.into(),
            instructions: None,
            max_output_tokens: None,
            previous_response_id: None,
            tools: None,
            tool_choice: None,
            store: None,
            extra: Extra::new(),
        }
    }
}

/// Responses API result. Output items retain all tool and modality variants.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub object: String,
    pub model: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub output: Vec<Value>,
    #[serde(default)]
    pub usage: Option<Value>,
    #[serde(default)]
    pub error: Option<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Response {
    /// Concatenate text output in wire order, excluding tool calls and refusals.
    pub fn output_text(&self) -> String {
        let mut text = String::new();
        for item in &self.output {
            if item.get("type").and_then(Value::as_str) != Some("message") {
                continue;
            }
            if let Some(parts) = item.get("content").and_then(Value::as_array) {
                for part in parts {
                    if part.get("type").and_then(Value::as_str) == Some("output_text") {
                        if let Some(fragment) = part.get("text").and_then(Value::as_str) {
                            text.push_str(fragment);
                        }
                    }
                }
            }
        }
        text
    }
}

/// A chat message with support for text, multimodal input, and tool calls.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<Value>>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl ChatMessage {
    pub fn new(role: impl Into<String>, content: impl Into<Value>) -> Self {
        Self {
            role: role.into(),
            content: Some(content.into()),
            name: None,
            tool_call_id: None,
            tool_calls: None,
            extra: Extra::new(),
        }
    }

    pub fn user(content: impl Into<Value>) -> Self {
        Self::new("user", content)
    }
    pub fn system(content: impl Into<Value>) -> Self {
        Self::new("system", content)
    }
    pub fn developer(content: impl Into<Value>) -> Self {
        Self::new("developer", content)
    }
    pub fn assistant(content: impl Into<Value>) -> Self {
        Self::new("assistant", content)
    }
    pub fn tool(call_id: impl Into<String>, content: impl Into<Value>) -> Self {
        let mut message = Self::new("tool", content);
        message.tool_call_id = Some(call_id.into());
        message
    }
}

/// Input to `POST /chat/completions`. Streaming is chosen by the method.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl ChatCompletionRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: None,
            max_completion_tokens: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            extra: Extra::new(),
        }
    }
}

/// A non-streaming chat completion.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatCompletion {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    #[serde(default)]
    pub usage: Option<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: u64,
    pub message: ChatMessage,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A streaming chat completion chunk. `delta` retains new tool and audio data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    #[serde(default)]
    pub choices: Vec<ChatChunkChoice>,
    #[serde(default)]
    pub usage: Option<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatChunkChoice {
    pub index: u64,
    pub delta: Value,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Input to embeddings. `input` may be strings or token arrays.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u64>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl EmbeddingRequest {
    pub fn new(model: impl Into<String>, input: impl Into<Value>) -> Self {
        Self {
            model: model.into(),
            input: input.into(),
            encoding_format: None,
            dimensions: None,
            extra: Extra::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub object: String,
    pub model: String,
    pub data: Vec<Embedding>,
    pub usage: Value,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Embedding {
    pub object: String,
    pub index: u64,
    pub embedding: EmbeddingVector,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingVector {
    Float(Vec<f64>),
    Base64(String),
}

/// Text-to-speech input. Voice and format names are deliberately open strings.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpeechRequest {
    pub model: String,
    pub input: String,
    pub voice: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl SpeechRequest {
    pub fn new(
        model: impl Into<String>,
        input: impl Into<String>,
        voice: impl Into<Value>,
    ) -> Self {
        Self {
            model: model.into(),
            input: input.into(),
            voice: voice.into(),
            response_format: None,
            instructions: None,
            speed: None,
            extra: Extra::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transcription {
    pub text: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageRequest {
    pub model: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl ImageRequest {
    pub fn new(model: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            prompt: prompt.into(),
            n: None,
            size: None,
            quality: None,
            extra: Extra::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageResponse {
    pub created: u64,
    #[serde(default)]
    pub data: Vec<ImageData>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageData {
    #[serde(default)]
    pub b64_json: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub revised_prompt: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct File {
    pub id: String,
    pub object: String,
    pub bytes: u64,
    pub created_at: u64,
    pub filename: String,
    pub purpose: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Cursor pages retain extra pagination fields and unknown item properties.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct List<T> {
    pub data: Vec<T>,
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default)]
    pub has_more: Option<bool>,
    #[serde(default)]
    pub first_id: Option<String>,
    #[serde(default)]
    pub last_id: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl<T> List<T> {
    /// Cursor for the next page, or an error for a malformed paginated response.
    pub fn next_after(&self) -> sydney_ai_core::Result<Option<&str>> {
        if self.has_more != Some(true) {
            return Ok(None);
        }
        self.last_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .map(Some)
            .ok_or_else(|| {
                sydney_ai_core::Error::Protocol("page has_more=true without last_id".into())
            })
    }
}

/// Input to `POST /batches`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchRequest {
    pub input_file_id: String,
    pub endpoint: String,
    pub completion_window: String,
    #[serde(flatten)]
    pub extra: Extra,
}

impl BatchRequest {
    pub fn new(input_file_id: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            input_file_id: input_file_id.into(),
            endpoint: endpoint.into(),
            completion_window: "24h".into(),
            extra: Extra::new(),
        }
    }
}

/// Input to fine-tuning jobs; methods and hyperparameters use `extra`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FineTuningRequest {
    pub model: String,
    pub training_file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl FineTuningRequest {
    pub fn new(model: impl Into<String>, training_file: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            training_file: training_file.into(),
            validation_file: None,
            suffix: None,
            extra: Extra::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModerationRequest {
    pub input: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl ModerationRequest {
    pub fn new(input: impl Into<Value>) -> Self {
        Self {
            input: input.into(),
            model: None,
            extra: Extra::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModerationResponse {
    pub id: String,
    pub model: String,
    pub results: Vec<ModerationResult>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModerationResult {
    pub flagged: bool,
    pub categories: BTreeMap<String, bool>,
    pub category_scores: BTreeMap<String, f64>,
    #[serde(flatten)]
    pub extra: Extra,
}

extra_builder!(
    ResponseRequest,
    ChatMessage,
    ChatCompletionRequest,
    EmbeddingRequest,
    SpeechRequest,
    ImageRequest,
    BatchRequest,
    FineTuningRequest,
    ModerationRequest
);
