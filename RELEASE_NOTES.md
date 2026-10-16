Initial release of the independent unofficial Sydney Rust AI SDKs:
`sydney-openai`, `sydney-gemini`, `sydney-groq`, and `sydney-ai-core`.

Common APIs have extensible Rust types, and advanced REST APIs support generic
resource handles. Includes SSE and WebSocket streaming, uploads, structured errors,
OpenAI webhook verification, mock contract tests, opt-in live tests, examples, and
provider coverage guides. Requires Rust 1.88+ and Tokio.

Install from this Git tag using the examples in README. A GitHub release is
separate from crates.io publication; do not assume registry availability.

No live provider account was used for verification. API permissions, model
availability, and beta behavior remain provider-controlled. See docs/openai.md,
docs/gemini.md, and docs/groq.md for exact coverage and limitations.
