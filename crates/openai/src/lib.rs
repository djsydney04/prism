//! An independently implemented, unofficial async OpenAI Rust SDK.
//!
//! Common generation operations have extensible typed requests and responses.
//! Every REST surface is reachable through [`Resource`] or [`Client::http`],
//! including new endpoint parameters and response fields. No model names are
//! hard-coded. WebRTC and WebSocket sessions require a separate transport.
//!
//! ```no_run
//! use sydney_openai::{Client, ResponseRequest};
//!
//! # async fn example() -> sydney_openai::Result<()> {
//! let client = Client::from_env()?;
//! let model = std::env::var("OPENAI_MODEL").expect("set OPENAI_MODEL");
//! let response = client.responses().create(&ResponseRequest::new(model, "Hello")).await?;
//! println!("{}", response.output_text());
//! # Ok(())
//! # }
//! ```
#![forbid(unsafe_code)]

mod resources;
pub mod types;

pub use resources::{Audio, Chat, Embeddings, Files, Images, Moderations, Responses};
pub use sydney_ai_core::{
    reqwest, serde_json, Config, Error, EventStream, Method, Resource, Result,
};
pub use types::*;

/// Default OpenAI REST root. Custom roots support gateways and local mock tests.
pub const BASE_URL: &str = "https://api.openai.com/v1/";

/// Authenticated OpenAI REST client backed by the shared secure HTTP transport.
#[derive(Clone, Debug)]
pub struct Client {
    http: sydney_ai_core::Client,
}

macro_rules! resource_accessors {
    ($($name:ident => $path:literal),+ $(,)?) => {$(
        #[doc = concat!("Access the `", $path, "` REST resource. Use child() for IDs and nested resources.")]
        pub fn $name(&self) -> Resource { self.resource($path) }
    )+};
}

impl Client {
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Self::with_config(Config::new(BASE_URL, api_key))
    }

    /// Read `OPENAI_API_KEY`; a missing or invalid credential is an error.
    pub fn from_env() -> Result<Self> {
        Self::with_config(Config::from_env(BASE_URL, "OPENAI_API_KEY")?)
    }

    /// Supply timeouts, headers, retry policy, and a custom API root via Config.
    pub fn with_config(config: Config) -> Result<Self> {
        Ok(Self {
            http: sydney_ai_core::Client::new(config)?,
        })
    }

    /// Low-level JSON, binary, multipart, raw HTTP, and streaming requests.
    pub fn http(&self) -> &sydney_ai_core::Client {
        &self.http
    }

    /// Access any relative REST path. Encode dynamic IDs using Resource::child.
    ///
    /// Relative paths preserve the configured API prefix. A leading slash is
    /// reserved for an explicitly origin-root-relative path.
    pub fn resource(&self, path: &str) -> Resource {
        self.http.resource(path)
    }

    pub fn responses(&self) -> Responses<'_> {
        Responses::new(&self.http)
    }
    pub fn chat(&self) -> Chat<'_> {
        Chat::new(&self.http)
    }
    pub fn embeddings(&self) -> Embeddings<'_> {
        Embeddings::new(&self.http)
    }
    pub fn audio(&self) -> Audio<'_> {
        Audio::new(&self.http)
    }
    pub fn images(&self) -> Images<'_> {
        Images::new(&self.http)
    }
    pub fn files(&self) -> Files<'_> {
        Files::new(&self.http)
    }
    pub fn moderations(&self) -> Moderations<'_> {
        Moderations::new(&self.http)
    }

    resource_accessors! {
        models => "models",
        conversations => "conversations",
        batches => "batches",
        fine_tuning => "fine_tuning",
        fine_tuning_jobs => "fine_tuning/jobs",
        fine_tuning_checkpoints => "fine_tuning/checkpoints",
        graders => "fine_tuning/alpha/graders",
        vector_stores => "vector_stores",
        evals => "evals",
        containers => "containers",
        uploads => "uploads",
        realtime => "realtime",
        organization => "organization",
        agents => "agents",
        skills => "skills",
        live => "live",
        webhooks => "webhook_endpoints",
        webhook_event_types => "webhook_event_types",
        decisions => "decisions",
        videos => "videos",
        safety => "safety",
        content_provenance_checks => "content_provenance_checks",
        chatkit => "chatkit",
        completions => "completions",
        assistants => "assistants",
        threads => "threads"
    }
}
