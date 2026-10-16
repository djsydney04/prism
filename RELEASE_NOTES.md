# Prism Rust AI SDKs

Independent, unofficial async Rust crates for OpenAI, Google's Gemini Developer
API, and Groq: `prism-openai`, `prism-gemini`, `prism-groq`, and the shared
`prism-ai-core`. Install provider crates from crates.io with the normal version
dependencies in [README](README.md). Requires Rust 1.88+ and Tokio.

Common generation APIs expose extensible typed requests and responses; advanced
REST APIs use authenticated generic resource handles. Shared capabilities include
SSE streaming, multipart uploads, structured errors, bounded responses, and
credential redaction. OpenAI Responses/Realtime/Live and Gemini Live support
WebSocket sessions. OpenAI webhook verification authenticates raw payloads and
checks timestamps; applications handle delivery deduplication and tool execution.

Mock contract tests and CI verify local protocol behavior. Runnable examples and
opt-in live tests support checking a configured provider account. Live service
availability, account permissions, model capabilities, and beta behavior remain
provider-controlled. See the [OpenAI](docs/openai.md), [Gemini](docs/gemini.md),
and [Groq](docs/groq.md) coverage guides for supported methods and limitations.
