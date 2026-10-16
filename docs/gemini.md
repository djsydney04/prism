# Gemini Developer API coverage

Package: `sydney-gemini`; import: `sydney_gemini`. Independent implementation, unofficial and unaffiliated with Google. The default origin is `https://generativelanguage.googleapis.com`; named methods use `/v1beta/`. Based on the official reference reviewed October 15, 2026.

## Coverage matrix

| API area | SDK entry point and named operations | Representation |
| --- | --- | --- |
| Model metadata | `models().list`, `get` | Typed models and page tokens |
| Content generation | `models().generate_content`, `stream_generate_content` | Typed content, media parts, tool calls, candidates, usage; SSE |
| Token counting | `models().count_tokens` | Typed token count; arbitrary serialized request |
| Embeddings | `models().embed_content`, `batch_embed_contents` | Typed vectors; arbitrary serialized request |
| Async batches | `models().batch_generate_content`, `async_batch_embed_content` | Typed operation |
| Batch lifecycle | `batches().list`, `get`, `cancel`, `delete`, `update_generate_content`, `update_embed_content` | Polling returns operations; updates return typed batch resources; query support |
| Specialized image/video prediction | `models().predict`, `predict_long_running` | JSON predictions or operation; model-specific input accepted |
| Operation polling | `operations().get` | Model, batch, file search and generated-file operation names |
| Cached content | `cached_contents().create`, `list`, `get`, `update`, `delete` | JSON; update mask query support |
| Files metadata | `files().list`, `get`, `delete`, `register` | Typed file, state, URI, page token, extra fields |
| Files upload | `files().start_upload`, `upload_chunk`, `query_upload`, `cancel_upload`, `upload` | Resumable protocol, binary chunks, offset headers |
| Generated media download | `files().download` | Raw bytes from same-origin initial URI; up to five redirects, auth permanently stripped across origins |
| File search stores | `file_search_stores().create`, `list`, `get`, `delete`, `import_file` | JSON and operations; explicit force deletion |
| File search documents | `file_search_stores().list_documents`, `get_document`, `delete_document` | JSON; explicit force deletion |
| File search upload | `file_search_stores().start_upload`, `upload_chunk`, `upload` | Resumable upload, operation result |
| Stateful interactions | `interactions().create`, `get`, `cancel`, `delete`, `stream`, `resume_stream` | Typed common interaction fields, JSON steps, SSE |
| Live authentication tokens | `create_auth_token` | JSON request and response |
| Gemini Live `BidiGenerateContent` | `live_connect`, `live_connect_ephemeral` | Bounded WebSocket, typed setup/server events, arbitrary JSON messages, raw frames, concurrent split halves |
| Managed agents | `agents().create`, `list`, `get`, `delete` | JSON |
| Webhooks | `webhooks().create`, `list`, `get`, `update`, `delete`, `ping`, `rotate_signing_secret` | JSON |
| Scheduled triggers | `triggers().create`, `list`, `get`, `update`, `delete`, `run`, `executions` | JSON |
| Environments | `environments().create`, `list`, `get`, `delete`, `files`, `start_file_upload` | JSON metadata; resumable file session |
| Voices | `voices().create`, `list`, `get`, `delete` | JSON |
| Credentials | `credentials().create`, `list`, `get`, `update`, `delete` | JSON |
| New or legacy REST resources | `resource(path)` and `transport()` | Generic JSON, HTTP methods, query, bytes, SSE and multipart |
| Vertex AI | Not included | Different endpoint/authentication contract |
| Realtime music-specific helpers | Not included | Generic transport is available for additional protocol integrations |
| Caller-managed OAuth access tokens | `with_oauth_token`, `with_oauth_config` | Bearer auth for REST; quota-project header configurable; no token acquisition/refresh |
| Automatic pagination/polling | Not included | Application controls iteration |
| Webhook signature verification | Not included | Receiving application must verify incoming webhook signatures |

## Schema and routing policy

Common generation, embedding, model, file, operation and interaction schemas are typed. Nested specialized tool, grounding, safety-result, image/video, agent and management schemas use JSON values. Every request accepts an arbitrary serializable type. `extra` maps preserve future JSON fields; string identifiers and status values remain extensible. This is broad REST method coverage, not an exhaustive typed reproduction of Google's entire schema.

Resource arguments accept an ID or its collection-qualified name. The SDK validates collection structure for nested document/operation names, rejects traversal, URL syntax, query/fragment injection, encoded separators and control characters, and encodes permitted opaque identifiers. Applications may override the origin through `Config` for trusted proxies or local tests. Initial requests and resumable URLs must match the configured origin. REST keys or caller-managed OAuth bearer tokens travel in headers; Google's Live protocol requires `key`/`access_token` query authentication. OAuth REST clients reject the Live query-key flow. Debug output redacts configuration credentials and upload session URLs; WebSocket diagnostics omit URLs and payloads.

Uploads can be split into application-managed chunks and resumed by querying the server's committed offset. The convenience upload buffers supplied bytes. Downloads buffer within the configured response limit and follow at most five redirects, permanently dropping API keys and custom headers after crossing origins. Outside loopback test servers, redirected media must use HTTPS.

Live connections send the initial setup and expose every documented text/audio/video/tool/resumption message through JSON. Applications wait for `setupComplete`, perform device capture/base64 encoding/playback, execute tools, control reconnects and preserve session handles. Message/frame limits use `Config::max_event_bytes`; handshake deadlines use `Config::timeout`. `split()` supports simultaneous input/output. No automatic reconnect or replay occurs.

The deprecated `corpora`, `tunedModels`, permissions and generated-file REST families can be reached through the generic transport where still enabled by Google. There are no named legacy wrappers. Stable `/v1/` endpoints can likewise be called through `resource`/`transport`; the named API selects `/v1beta/` explicitly.

## Testing scope

Offline contract tests assert named routes and HTTP methods, Google key-header authentication, exact representative request bodies, streamed chunks and resumed event IDs, query parameters, file-search force flags, resumable upload negotiation and binary content, pagination tokens, response extension retention, provider error diagnostics, and rejection before any network access for malicious names/foreign URLs. Live mock servers assert key versus ephemeral-token query authentication, endpoint selection, setup-first ordering, multimodal/tool input, binary JSON output and extension retention. Shared transport tests cover WebSocket message bounds, redacted errors, redirects, splitting and deadlines. Unit tests cover mixed text/tool/thought parts and schema extension round trips.

Examples and Rust documentation are compiled. Live provider calls need an API key and enabled/billable account, so mocks do not verify live access, evolving model-specific requirements, quotas, or server behavior. Do not describe this suite as live conformance certification.

## Official reference sources

- [API overview and authentication](https://ai.google.dev/api)
- [All REST methods](https://ai.google.dev/api/all-methods)
- [Content generation](https://ai.google.dev/api/generate-content)
- [Model metadata and prediction](https://ai.google.dev/api/models)
- [Interactions](https://ai.google.dev/api/interactions-api)
- [Batch API](https://ai.google.dev/api/batch-api)
- [Cached content](https://ai.google.dev/api/caching)
- [Files and resumable upload](https://ai.google.dev/api/files)
- [Files guide and download limitations](https://ai.google.dev/gemini-api/docs/files)
- [File search stores](https://ai.google.dev/api/file-search/file-search-stores)
- [Environments](https://ai.google.dev/api/environments)
- [Live WebSocket reference](https://ai.google.dev/api/live)
- [Live WebSocket authentication](https://ai.google.dev/gemini-api/docs/live-api/get-started-websocket)
- [Caller-managed OAuth authentication](https://ai.google.dev/gemini-api/docs/oauth)
