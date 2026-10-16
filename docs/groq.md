# Groq API coverage

Independently implemented from the [official Groq REST API reference](https://console.groq.com/docs/api-reference)
and [batch guide](https://console.groq.com/docs/batch), reviewed 2026-10-15.
No third-party SDK source is included. Package: `sydney-groq`; Rust import:
`sydney_groq`.

| API operation | Method and route | SDK surface |
| --- | --- | --- |
| Chat completion | POST `/openai/v1/chat/completions` | `chat().create`, `chat().stream` |
| Response (beta) | POST `/openai/v1/responses` | `responses().create`, `responses().stream` |
| Transcription | POST `/openai/v1/audio/transcriptions` | `audio().transcribe`, `transcribe_bytes` |
| Translation | POST `/openai/v1/audio/translations` | `audio().translate`, `translate_bytes` |
| Speech generation | POST `/openai/v1/audio/speech` | `audio().speech` |
| List models | GET `/openai/v1/models` | `models().list` |
| Retrieve model | GET `/openai/v1/models/{model}` | `models().retrieve` |
| Create batch | POST `/openai/v1/batches` | `batches().create` |
| Retrieve batch | GET `/openai/v1/batches/{id}` | `batches().retrieve` |
| List batches | GET `/openai/v1/batches` | `batches().list` |
| Cancel batch | POST `/openai/v1/batches/{id}/cancel` | `batches().cancel` |
| Upload file | POST `/openai/v1/files` | `files().upload`, `upload_for` |
| List files | GET `/openai/v1/files` | `files().list`, `list_with` |
| Retrieve file | GET `/openai/v1/files/{id}` | `files().retrieve` |
| Delete file | DELETE `/openai/v1/files/{id}` | `files().delete` |
| Download file | GET `/openai/v1/files/{id}/content` | `files().content` |
| List fine tunings (closed beta) | GET `/v1/fine_tunings` | `fine_tunings().list` |
| Create fine tuning (closed beta) | POST `/v1/fine_tunings` | `fine_tunings().create` |
| Retrieve fine tuning (closed beta) | GET `/v1/fine_tunings/{id}` | `fine_tunings().retrieve` |
| Delete fine tuning (closed beta) | DELETE `/v1/fine_tunings/{id}` | `fine_tunings().delete` |

Chat parameters cover every top-level field documented at review time,
including multimodal messages, functions/tools, Compound settings, documents,
citations, reasoning, web-search settings, structured output, and streaming.
Model capabilities and parameter validation are controlled by Groq. Historical
or unsupported parameters remain representable for wire compatibility.

The batch guide documents audio transcription and translation JSONL inputs
alongside chat requests, while the API reference lists a narrower batch
`endpoint` value. The SDK preserves arbitrary endpoint strings so it can
represent either documented form without imposing a stale model catalog.

JSON structures expose typed common fields and flattened extension maps.
Polymorphic response output items, Compound configuration, schemas, and
provider-specific metadata use `serde_json::Value`. New paths and alternate
request/response types remain accessible through the shared transport and
generic resource API; generic access is separate from typed route coverage.

Every listed operation has mock-server coverage. Tests verify the native
fine-tuning prefix, namespaced model IDs, forward-compatible JSON, query
parameters, binary preservation, multipart fields, provider errors, and SSE
streaming. They validate the client/server contract locally. Production
compatibility still depends on account permissions and provider behavior; no
live Groq account was used for these tests.
