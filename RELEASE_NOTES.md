Prism v0.2.0 introduces the new name for these independent unofficial Rust AI SDKs:
`prism-openai`, `prism-gemini`, `prism-groq`, and `prism-ai-core`.

Common APIs have extensible Rust types, and advanced REST APIs support generic
resource handles. Includes SSE and WebSocket streaming, uploads, structured errors,
OpenAI webhook verification, mock contract tests, opt-in live tests, examples, and
provider coverage guides. Requires Rust 1.88+ and Tokio.

Install from this Git tag using the examples in README. A GitHub release is
separate from crates.io publication; do not assume registry availability.

Migrate from v0.1.0 by changing Cargo package names from `sydney-*` to `prism-*`
and Rust imports from `sydney_*` to `prism_*`. The SDK operations are unchanged.

No live provider account was used for verification. API permissions, model
availability, and beta behavior remain provider-controlled. See docs/openai.md,
docs/gemini.md, and docs/groq.md for exact coverage and limitations.
