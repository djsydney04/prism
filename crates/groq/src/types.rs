//! Wire models. Optional fields are omitted when absent; `extra` preserves new fields.

use crate::{reqwest, Error, Result, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Unknown JSON properties, kept for forwards compatibility.
pub type Extra = BTreeMap<String, Value>;

/// A chat message's text or multimodal content.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Content {
    /// Plain text.
    Text(String),
    /// Text, images, audio, or future content types.
    Parts(Vec<ContentPart>),
}
impl From<String> for Content {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for Content {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}
impl From<Vec<ContentPart>> for Content {
    fn from(value: Vec<ContentPart>) -> Self {
        Self::Parts(value)
    }
}

/// Extensible multimodal chat content part.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ContentPart {
    /// Wire content type, such as `text` or `image_url`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<ImageUrl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_audio: Option<InputAudio>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl ContentPart {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            kind: "text".into(),
            text: Some(text.into()),
            ..Self::default()
        }
    }
    /// Image URL or a data URI. The SDK does not fetch it locally.
    pub fn image(url: impl Into<String>) -> Self {
        Self {
            kind: "image_url".into(),
            image_url: Some(ImageUrl {
                url: url.into(),
                ..ImageUrl::default()
            }),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct InputAudio {
    /// Base64-encoded audio bytes.
    pub data: String,
    pub format: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Conversation message. String roles also permit future provider additions.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Message {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl Message {
    pub fn new(role: impl Into<String>, content: impl Into<Content>) -> Self {
        Self {
            role: role.into(),
            content: Some(content.into()),
            ..Self::default()
        }
    }
    pub fn user(content: impl Into<Content>) -> Self {
        Self::new("user", content)
    }
    pub fn system(content: impl Into<Content>) -> Self {
        Self::new("system", content)
    }
    pub fn assistant(content: impl Into<Content>) -> Self {
        Self::new("assistant", content)
    }
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<Content>) -> Self {
        Self {
            tool_call_id: Some(tool_call_id.into()),
            ..Self::new("tool", content)
        }
    }
}

/// Function invocation. Streaming chunks can omit the name and contain partial arguments.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct FunctionCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Returned tool call or streaming tool-call delta. Calls are never executed by this SDK.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionCall>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Function tool declaration with an arbitrary JSON Schema.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Function {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Tool {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<Function>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl Tool {
    pub fn function(name: impl Into<String>, parameters: Value) -> Self {
        Self {
            kind: "function".into(),
            function: Some(Function {
                name: name.into(),
                parameters: Some(parameters),
                ..Function::default()
            }),
            extra: Extra::new(),
        }
    }
}

/// Structured output configuration, supporting text, JSON mode, and JSON Schema.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<JsonSchema>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl ResponseFormat {
    pub fn json_object() -> Self {
        Self {
            kind: "json_object".into(),
            ..Self::default()
        }
    }
    pub fn json_schema(name: impl Into<String>, schema: Value, strict: bool) -> Self {
        Self {
            kind: "json_schema".into(),
            json_schema: Some(JsonSchema {
                name: name.into(),
                schema,
                strict: Some(strict),
                ..JsonSchema::default()
            }),
            extra: Extra::new(),
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct JsonSchema {
    pub name: String,
    pub schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Typed chat request covering every top-level field in Groq's API reference.
/// Groq itself validates model-specific parameter support.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation_options: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compound_custom: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_tool_validation: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documents: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions: Option<Vec<Function>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_reasoning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<BTreeMap<String, f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_settings: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChatCompletion {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: Option<Usage>,
    #[serde(default)]
    pub system_fingerprint: Option<String>,
    #[serde(default)]
    pub service_tier: Option<String>,
    #[serde(default)]
    pub x_groq: Option<Value>,
    #[serde(default)]
    pub usage_breakdown: Option<Value>,
    #[serde(default)]
    pub mcp_list_tools: Option<Vec<Value>>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: Message,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub logprobs: Option<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A streamed chat event. Usage-only chunks may have an empty `choices` array.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChatChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    #[serde(default)]
    pub choices: Vec<ChunkChoice>,
    #[serde(default)]
    pub usage: Option<Usage>,
    #[serde(default)]
    pub x_groq: Option<Value>,
    #[serde(default)]
    pub system_fingerprint: Option<String>,
    #[serde(default)]
    pub service_tier: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: Message,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub logprobs: Option<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: Option<u64>,
    #[serde(default)]
    pub completion_tokens: Option<u64>,
    #[serde(default)]
    pub total_tokens: Option<u64>,
    #[serde(default)]
    pub queue_time: Option<f64>,
    #[serde(default)]
    pub prompt_time: Option<f64>,
    #[serde(default)]
    pub completion_time: Option<f64>,
    #[serde(default)]
    pub total_time: Option<f64>,
    #[serde(default)]
    pub prompt_tokens_details: Option<Value>,
    #[serde(default)]
    pub completion_tokens_details: Option<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Responses beta request. Polymorphic inputs use JSON to retain every wire form.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResponseRequest {
    pub model: String,
    pub input: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl ResponseRequest {
    pub fn new(model: impl Into<String>, input: impl Into<Value>) -> Self {
        Self {
            model: model.into(),
            input: input.into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub object: String,
    pub model: String,
    pub status: String,
    pub created_at: u64,
    pub output: Vec<Value>,
    #[serde(default)]
    pub usage: Option<Value>,
    #[serde(default)]
    pub error: Option<Value>,
    #[serde(default)]
    pub incomplete_details: Option<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl Response {
    /// Join text content from output message items in server order.
    pub fn output_text(&self) -> String {
        self.output
            .iter()
            .filter_map(|item| item.get("content").and_then(Value::as_array))
            .flatten()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("output_text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect()
    }
}

/// Model information; no model-name enum restricts new releases.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub owned_by: String,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub max_completion_tokens: Option<u64>,
    #[serde(default)]
    pub public_apps: Option<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A provider list page; pagination metadata is retained.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct List<T> {
    pub object: String,
    pub data: Vec<T>,
    #[serde(default)]
    pub first_id: Option<String>,
    #[serde(default)]
    pub last_id: Option<String>,
    #[serde(default)]
    pub has_more: Option<bool>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, String>,
}
impl ListOptions {
    pub(crate) fn query(&self) -> Vec<(String, String)> {
        let mut query: Vec<_> = self
            .extra
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if let Some(after) = &self.after {
            query.push(("after".into(), after.clone()));
        }
        if let Some(limit) = self.limit {
            query.push(("limit".into(), limit.to_string()));
        }
        query
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Deletion {
    pub id: String,
    pub object: String,
    pub deleted: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BatchRequest {
    pub input_file_id: String,
    pub endpoint: String,
    pub completion_window: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl BatchRequest {
    pub fn new(input_file_id: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            input_file_id: input_file_id.into(),
            endpoint: endpoint.into(),
            completion_window: "24h".into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Batch {
    pub id: String,
    pub object: String,
    pub endpoint: String,
    pub input_file_id: String,
    pub completion_window: String,
    pub status: String,
    pub created_at: u64,
    #[serde(default)]
    pub output_file_id: Option<String>,
    #[serde(default)]
    pub error_file_id: Option<String>,
    #[serde(default)]
    pub errors: Option<Value>,
    #[serde(default)]
    pub request_counts: Option<RequestCounts>,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub cancelled_at: Option<u64>,
    #[serde(default)]
    pub cancelling_at: Option<u64>,
    #[serde(default)]
    pub completed_at: Option<u64>,
    #[serde(default)]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub expired_at: Option<u64>,
    #[serde(default)]
    pub failed_at: Option<u64>,
    #[serde(default)]
    pub finalizing_at: Option<u64>,
    #[serde(default)]
    pub in_progress_at: Option<u64>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RequestCounts {
    pub total: u64,
    pub completed: u64,
    pub failed: u64,
    #[serde(flatten)]
    pub extra: Extra,
}

/// One JSONL input record for the batch endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchItem<T = Value> {
    pub custom_id: String,
    pub method: String,
    pub url: String,
    pub body: T,
    #[serde(flatten)]
    pub extra: Extra,
}
impl<T: Serialize> BatchItem<T> {
    /// Serialize one record followed by a newline for a JSONL file.
    pub fn to_jsonl(&self) -> Result<String> {
        Ok(format!("{}\n", serde_json::to_string(self)?))
    }
}

/// Uploaded audio bytes or a URL passed to Groq.
#[derive(Clone, Debug)]
pub enum AudioSource {
    File {
        filename: String,
        bytes: Vec<u8>,
        mime_type: Option<String>,
    },
    Url(String),
}

/// Multipart speech recognition parameters. This consumes its bytes on send.
#[derive(Clone, Debug)]
pub struct AudioRequest {
    pub model: String,
    pub source: AudioSource,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub response_format: Option<String>,
    pub temperature: Option<f64>,
    pub timestamp_granularities: Vec<String>,
    /// Additional form fields, allowing repeated names.
    pub extra: Vec<(String, String)>,
}
impl AudioRequest {
    pub fn file(model: impl Into<String>, filename: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self::new(
            model,
            AudioSource::File {
                filename: filename.into(),
                bytes,
                mime_type: None,
            },
        )
    }
    pub fn url(model: impl Into<String>, url: impl Into<String>) -> Self {
        Self::new(model, AudioSource::Url(url.into()))
    }
    pub fn new(model: impl Into<String>, source: AudioSource) -> Self {
        Self {
            model: model.into(),
            source,
            language: None,
            prompt: None,
            response_format: None,
            temperature: None,
            timestamp_granularities: Vec::new(),
            extra: Vec::new(),
        }
    }
    pub(crate) fn into_form(self) -> Result<reqwest::multipart::Form> {
        let mut form = reqwest::multipart::Form::new().text("model", self.model);
        form = match self.source {
            AudioSource::Url(url) => form.text("url", url),
            AudioSource::File {
                filename,
                bytes,
                mime_type,
            } => {
                let mut part = reqwest::multipart::Part::bytes(bytes).file_name(filename);
                if let Some(mime) = mime_type {
                    part = part.mime_str(&mime).map_err(Error::from)?;
                }
                form.part("file", part)
            }
        };
        if let Some(value) = self.language {
            form = form.text("language", value);
        }
        if let Some(value) = self.prompt {
            form = form.text("prompt", value);
        }
        if let Some(value) = self.response_format {
            form = form.text("response_format", value);
        }
        if let Some(value) = self.temperature {
            form = form.text("temperature", value.to_string());
        }
        for value in self.timestamp_granularities {
            form = form.text("timestamp_granularities[]", value);
        }
        for (name, value) in self.extra {
            form = form.text(name, value);
        }
        Ok(form)
    }
}

/// Recognition response, including optional verbose segment and word timings.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Transcription {
    pub text: String,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub words: Option<Vec<Value>>,
    #[serde(default)]
    pub segments: Option<Vec<Value>>,
    #[serde(default)]
    pub x_groq: Option<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SpeechRequest {
    pub model: String,
    pub input: String,
    pub voice: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl SpeechRequest {
    pub fn new(
        model: impl Into<String>,
        input: impl Into<String>,
        voice: impl Into<String>,
    ) -> Self {
        Self {
            model: model.into(),
            input: input.into(),
            voice: voice.into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FineTuningRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FineTuning {
    pub id: String,
    pub name: String,
    pub base_model: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub input_file_id: String,
    pub created_at: u64,
    pub fine_tuned_model: String,
    #[serde(flatten)]
    pub extra: Extra,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FineTuningResponse {
    pub id: String,
    pub object: String,
    pub data: FineTuning,
    #[serde(flatten)]
    pub extra: Extra,
}
