# Changelog

## 0.1.0 — 2026-10-15

Initial independently written unofficial OpenAI, Gemini, and Groq Rust crates.

- Shared async HTTP transport, provider authentication, structured errors, bounded
  responses, multipart uploads, SSE, safe resource paths, and cursor page helpers.
- OpenAI Responses/Realtime/Live and Gemini Live WebSockets, with bounded messages,
  concurrent send/receive, redacted failures, and explicit session lifecycle.
- OpenAI signed webhook verification, secret URL redaction, IPv6 loopback tests,
  and media download redirects that drop credentials after leaving the API origin.
- Extensible typed generation requests and responses, with authenticated generic
  REST resource access for advanced and newly added provider parameters.
- OpenAI Responses/chat/embeddings/audio/images/moderations/files and advanced
  resource handles; Gemini content/embeddings/caches/files/batches/interactions/
  file-search/platform APIs; Groq's documented inference, batch, file, and fine-tuning routes.
- Mock contract tests, runnable examples, coverage matrices, CI, and versioned
  GitHub release and optional crates.io publishing workflows.

See provider coverage guides for typed coverage and transport limitations.
