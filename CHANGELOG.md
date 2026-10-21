# Changelog

## [1.0.0](https://github.com/djsydney04/prism/compare/v0.2.2...v1.0.0) (2026-10-21)


### ⚠ BREAKING CHANGES

* optimize required checks and update WebSocket TLS ([#5](https://github.com/djsydney04/prism/issues/5))

### Features

* configure weekly SDK maintenance with Groq GPT OSS 120B ([#4](https://github.com/djsydney04/prism/issues/4)) ([32a0721](https://github.com/djsydney04/prism/commit/32a0721da1b39d5b8b446ba45804765b09554a12))


### Bug Fixes

* **core:** yield SSE events incrementally to bound buffering ([#3](https://github.com/djsydney04/prism/issues/3)) ([b71577a](https://github.com/djsydney04/prism/commit/b71577a1a240a2237ab5a060253e88703d8689b0))


### Continuous Integration

* optimize required checks and update WebSocket TLS ([#5](https://github.com/djsydney04/prism/issues/5)) ([ce00181](https://github.com/djsydney04/prism/commit/ce001814f9226608036a068d22dcf750d4b37449))

## [0.2.2](https://github.com/djsydney04/prism/compare/v0.2.1...v0.2.2) (2026-10-16)


### Bug Fixes

* use Prism repository metadata and explicit synthetic webhook fixtures ([7052731](https://github.com/djsydney04/prism/commit/7052731a5e4a8378143e56c5c7616a4d3401ac3a))

## [0.2.1](https://github.com/djsydney04/prism/compare/v0.2.0...v0.2.1) (2026-10-16)


### Bug Fixes

* **ci:** automate synchronized releases and crate publication with Release Please ([b7d0aa4](https://github.com/djsydney04/prism/commit/b7d0aa47ad5470dd47779d5cd2b4e2a8ece626f2))

## 0.2.0 — 2026-10-15

- Rename the SDK family to Prism: `prism-openai`, `prism-gemini`, `prism-groq`,
  and `prism-ai-core`. Rust imports now use the `prism_` prefix.
- Update manifests, examples, documentation, release tooling, and the HTTP user
  agent consistently. SDK operations retain their existing behavior.
- The earlier `v0.1.0` GitHub tag and its original package names remain intact.

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
