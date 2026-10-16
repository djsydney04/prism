# OpenAI coverage

The `prism-openai` crate is an independent implementation based on the
[official OpenAI HTTP API reference](https://developers.openai.com/api/reference/overview).
The reference was reviewed on 2026-10-15. No third-party SDK code was imported.
Package version is `0.2.0` and minimum supported Rust is 1.88. <!-- x-release-please-version -->

“Typed” below means a named request/response Rust type and an ergonomic method.
“Resource” means an authenticated HTTP endpoint handle accepting any serializable
request and deserializable response, with explicit nested paths and HTTP verbs.
Resource support is intentionally broader than the typed schema. It is not a
claim that every schema field has a dedicated Rust type or every endpoint has
been exercised against a live OpenAI account.

| API surface | Access and supported operations | Schema level |
| --- | --- | --- |
| [Responses](https://developers.openai.com/api/reference/typescript/resources/responses) | `responses()`: create, retrieve, delete, cancel, compact, input items, input token counts, create/retrieve SSE, persistent WebSocket `connect()` | Typed common create/result; JSON for full unions, advanced requests, and WebSocket events |
| [Chat completions](https://developers.openai.com/api/reference/typescript/resources/chat) | `chat()`: create, retrieve, update, delete, list, messages, SSE including usage and tool deltas | Typed common request, messages, results and chunks; JSON extensions |
| [Conversations](https://developers.openai.com/api/reference/typescript/resources/conversations) | `conversations()`: CRUD; nested items CRUD/list with `child` | Resource |
| [Embeddings](https://developers.openai.com/api/reference/typescript/resources/embeddings) | `embeddings().create`: text and token input; float/base64 output; dimensions via field | Typed |
| [Images](https://developers.openai.com/api/reference/typescript/resources/images) | `images()`: generation, generation SSE, multipart/JSON edits, multipart edit SSE, variations | Typed common generation/results; JSON/multipart for advanced fields |
| [Audio](https://developers.openai.com/api/reference/typescript/resources/audio) | `audio()`: speech bytes; transcription/translation JSON or bytes; transcription SSE; voice/consent multipart and CRUD via resources | Typed speech/common transcription; arbitrary multipart fields |
| [Files](https://developers.openai.com/api/reference/typescript/resources/files) | `files()`: multipart uploads, list, retrieve, delete, exact content bytes | Typed common metadata and lists; custom forms |
| [Uploads](https://developers.openai.com/api/reference/typescript/resources/uploads) | `uploads()`: create, parts multipart, complete, cancel | Resource |
| [Models](https://developers.openai.com/api/reference/typescript/resources/models) | `models()`: list, retrieve, delete | Resource |
| [Batches](https://developers.openai.com/api/reference/typescript/resources/batches) | `batches()`: create, retrieve, list, cancel | Typed create input; resource results/actions |
| [Fine tuning](https://developers.openai.com/api/reference/typescript/resources/fine_tuning) | `fine_tuning_jobs()`, `fine_tuning_checkpoints()`, `graders()`: jobs CRUD/list, cancel/pause/resume, events, checkpoints, checkpoint permissions, graders run/validate | Typed common job-create input; resource operations |
| [Vector stores](https://developers.openai.com/api/reference/typescript/resources/vector_stores) | `vector_stores()`: CRUD, search, files CRUD/content, file batches create/retrieve/list/cancel | Resource |
| [Evals](https://developers.openai.com/api/reference/typescript/resources/evals) | `evals()`: CRUD/list, runs create/retrieve/list/delete/cancel, output item list/retrieve | Resource |
| [Containers](https://developers.openai.com/api/reference/typescript/resources/containers) | `containers()`: create/retrieve/list/delete, nested files upload/retrieve/list/delete and bytes | Resource + `http().bytes` for content |
| [Moderation](https://developers.openai.com/api/reference/typescript/resources/moderations) | `moderations().create`: text and multimodal inputs, open category maps | Typed common request/result |
| [Realtime](https://developers.openai.com/api/reference/typescript/resources/realtime) | `realtime()`: client secrets, translation client secrets, calls multipart/raw SDP, accept/hangup/refer/reject, legacy session REST; `realtime_connect(model)` for WebSocket JSON I/O | Resource + raw/multipart for negotiation; WebSocket session for events |
| [Live](https://developers.openai.com/api/reference/typescript/resources/live) | `live().child("sessions")`: create, accept, fork, hangup, refer, reject; recording bytes; `live_connect(&[])` primary WebSocket | Resource + raw bytes; WebSocket session for events |
| [Agents](https://developers.openai.com/api/reference/resources/beta/subresources/agents/methods/create) | `agents()`: CRUD/list; nested environments, templates, sessions, artifacts, events, items, turns, traces, subagents; `vaults()` for vaults and nested credentials | Resource; optional beta headers configured by caller |
| [Skills](https://developers.openai.com/api/reference/typescript/resources/skills) | `skills()`: multipart create, CRUD/list/content, versions create/retrieve/delete/list/content | Resource + multipart/bytes |
| [Webhooks](https://developers.openai.com/api/reference/typescript/resources/webhooks) | `webhooks()` maps to `webhook_endpoints`: CRUD/list, rotate secret, test; `webhook_event_types()` lists event kinds; `webhooks::WebhookVerifier` verifies raw incoming bodies | Resource administration; local HMAC verification and timestamp window |
| [Decisions](https://developers.openai.com/api/reference/typescript/resources/decisions) | `decisions().create` | Resource |
| [Safety](https://developers.openai.com/api/reference/typescript/resources/safety) | `safety()`: alerts and cases retrieval | Resource |
| [Content provenance](https://developers.openai.com/api/reference/typescript/resources/content_provenance_checks) | `content_provenance_checks().create` | Resource |
| [Video](https://developers.openai.com/api/reference/typescript/resources/videos) | `videos()`: multipart create, retrieve/list/delete, remix, edits, extensions, character create/retrieve; raw content download | Resource |
| [ChatKit](https://developers.openai.com/api/reference/resources/beta/subresources/chatkit/subresources/sessions/methods/create) | `chatkit()`: sessions create/cancel, threads retrieve/list/delete, items list | Resource |
| [Administration](https://developers.openai.com/api/reference/administration) | `organization()`: admin keys, audit logs, users/invites, groups/roles, projects and nested keys/certificates/retention/groups/permissions/rate limits/service accounts/spend limits/users, usage/costs | Resource; requires an appropriate Admin API key |
| Legacy HTTP APIs | `completions()`, `assistants()`, `threads()`, or `resource(path)` | Resource; provider availability and beta headers remain caller responsibilities |

## Resource examples

```rust,no_run
use prism_openai::{Client, Method, serde_json::{json, Value}};

# async fn example(client: &Client) -> prism_openai::Result<()> {
// Actions append one safely encoded path segment.
let cancelled: Value = client.batches().child("batch_123").action("cancel")
    .create(&json!({})).await?;

// Child resources safely encode IDs and support all documented JSON fields.
let items: Value = client.conversations().child("conv_123").child("items")
    .get(&[("limit".into(), "20".into())]).await?;

// General HTTP methods cover resources that require PUT/PATCH or extra queries.
let updated: Value = client.resource("a_documented_endpoint").request(
    Method::PATCH, &[], Some(&json!({"metadata":{"label":"example"}}))
).await?;
# Ok(())
# }
```

For content downloads use `http().bytes(Method::GET, handle.path(), &[], None)`.
For SDP negotiation use `http().raw` with the documented Content-Type and raw
body; response headers (including Location), status, and body remain accessible.
Multipart uploads use `reqwest::multipart::Form`, re-exported by the crate. JSON
decoders accept empty successful responses as null for endpoints returning 204.

## WebSocket protocol

[`responses().connect()`](https://developers.openai.com/api/docs/guides/websocket-mode)
uses `/v1/responses`. Send a `response.create` JSON event and handle the documented
response events, including completion and errors. HTTP-only `stream` and
`background` fields are omitted. JSON preserves `stream_id` for multiplexed lanes;
callers route interleaved events themselves.

[`realtime_connect(model)`](https://developers.openai.com/api/docs/guides/voice-websockets)
uses `/v1/realtime` with an encoded model query and configured header credentials.
[`live_connect(&[])`](https://developers.openai.com/api/reference/resources/live/primary-websocket)
uses `/v1/live/sessions` without query parameters; send `session.start` and wait
for `session.started` before audio. `websocket(path, query)` supports documented
sideband/fork paths. `send_json`/`recv_json` preserve arbitrary event fields;
`split()` supports independent sender and receiver tasks. Raw messages remain
accessible when applications need binary data or lower-level frame handling.

## Explicit boundaries

- This release supplies REST, SSE, and WebSocket transports. It does not
  implement a WebRTC peer connection, SIP media engine, audio device loop,
  automatic tool execution, or automatic model-event orchestration. Applications
  handle protocol events, audio encoding/playback, and reconnect policy.
- Incoming webhook verification checks HMAC signatures and a configurable
  timestamp window. Applications must deduplicate verified delivery IDs in
  durable storage to prevent repeat processing within that window.
- Resources accept any documented schema; advanced schemas do not have a
  distinct Rust type for every union member. Public request extension maps and
  `create_json`/`Resource` methods preserve new parameters without SDK changes.
- Cursors, polling, account permissions, model availability, request-size limits,
  tool authorization, and model-output validation are caller-controlled.
- This SDK does not neutralize prompt injection inside model inputs, retrieval
  results, or tool output. It sends data and returns data; it never executes
  generated code or tools. Applications must enforce their own trust boundaries.

## Test evidence

`cargo test -p prism-openai` runs offline contract tests against local HTTP mock
servers. They verify authenticated routing with a custom `/v1/` prefix, request
extensions, multimodal input, typed response preservation, cancellation and
empty delete responses, cursor queries, background response stream resumption,
named SSE events, tool-call deltas and usage chunks, float/base64 embeddings,
image requests, multipart file/audio uploads, non-JSON speech/transcription
formats, multipart SSE, dynamic-ID encoding, advanced resource nesting, admin
headers, provider error details, and raw SDP responses. Unit tests also verify
text extraction and malformed pagination detection.

Local WebSocket mock servers verify header authentication, the Responses socket
route and multiplexed event data, escaped Realtime model queries and tool events,
Live `session.start` exchange, generic sideband queries, and split sender/receiver
I/O. These tests require no provider credentials or Internet access.

No live credentialed test results are claimed by this matrix. Mock tests do not
prove provider-side schema acceptance or access to beta/organization endpoints.
Runnable examples opt into actual API requests through environment credentials.
