# Prism Rust AI SDKs

Independent, unofficial async Rust crates for OpenAI, Google's Gemini Developer
API, and Groq. Written here from provider documentation rather than copied from
another SDK. Not affiliated with or endorsed by these providers.

| Crate | Rust import | API reference |
|---|---|---|
| `prism-openai` | `prism_openai` | [Coverage and examples](docs/openai.md) |
| `prism-gemini` | `prism_gemini` | [Coverage and examples](docs/gemini.md) |
| `prism-groq` | `prism_groq` | [Coverage and examples](docs/groq.md) |
| `prism-ai-core` | `prism_ai_core` | Shared transport, errors, SSE, and resource operations |

Requires Rust 1.88+ and a Tokio runtime. Every client is cloneable and shares its
connection pool. The crates use Rustls; system OpenSSL is not required.
OpenAI Responses/Realtime/Live and Gemini Live WebSocket sessions support concurrent
send/receive, JSON and binary frames, and configurable message limits.

## Install

Add the crates you need from crates.io:

```toml
[dependencies]
prism-openai = "0.2.2" # x-release-please-version
prism-gemini = "0.2.2" # x-release-please-version
prism-groq = "0.2.2" # x-release-please-version
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Registry publication runs through GitHub Actions; see
[release instructions](RELEASING.md).

[Weekly cloud maintenance](docs/maintenance.md) checks provider docs and proposes
tested SDK updates through PRs using Groq's GPT OSS 120B model
(`openai/gpt-oss-120b`).

## Use

Set `OPENAI_API_KEY`, `GEMINI_API_KEY`, or `GROQ_API_KEY` in your application's
environment. Keys are never compiled into the libraries. Examples accept model
names from environment variables so you control model availability and pricing.

```rust,no_run
use prism_openai::{Client, ResponseRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model = std::env::var("OPENAI_MODEL")?;
    let response = Client::from_env()?.responses()
        .create(&ResponseRequest::new(model, "Hello from Rust")).await?;
    println!("{}", response.output_text());
    Ok(())
}
```

For Gemini and Groq quickstarts, streaming, tools, and upload examples, see the
provider guides above and each crate's `examples/` directory.

## Coverage and API evolution

Common generation APIs have typed request/response models. Extension maps preserve
unknown fields and permit new provider parameters without a crate upgrade. Less
common REST resources use an authenticated `Resource` handle and arbitrary
serializable Rust types or `serde_json::Value`. The provider coverage guides
distinguish typed methods, generic resource access, and transport limitations.
Resource access is an escape hatch, not evidence that every server-side workflow
has been exercised. Model IDs are unrestricted strings.

```rust,no_run
# async fn example(client: prism_openai::Client) -> prism_openai::Result<()> {
let page: prism_openai::serde_json::Value = client.models().list(&[]).await?;
println!("{page}");
# Ok(()) }
```

## Reliability and security

- Bearer or `x-goog-api-key` REST headers, redacted credential debug output, HTTPS
  enforced outside loopback test servers, and same-origin resumable uploads.
  Gemini Live requires its key or ephemeral token in a WebSocket query parameter;
  SDK diagnostics redact it, but your proxies should also avoid logging URLs.
  Custom endpoints explicitly receive your configured key.
- Redirects are disabled for ordinary requests. Explicit media downloads follow
  at most five redirects and permanently strip credentials and custom headers
  after leaving the configured origin.
- Structured API errors preserve status, request IDs, and original error bodies.
  Avoid logging provider responses if your requests contain private information.
- Bounded buffered responses (64 MiB default) and SSE events (8 MiB default).
  Configure larger limits for large audio/file downloads.
- Only GET/HEAD/OPTIONS are automatically retried on transient failures (two
  retries by default). Writes, uploads, and streams never replay automatically.
  `Retry-After` is honored up to 60 seconds per retry. Default total request
  timeout is 120 seconds, including SSE streams; configure it for long generations.
  WebSocket handshakes use that timeout; applications set their own session idle
  deadlines and reconnect policy.
- Tools and model output are returned as data. Libraries never execute function
  calls, shell commands, downloaded code, or instructions embedded in responses.
  Applications still need their own prompt-injection defenses and tool permissions.
- No credential discovery beyond the documented environment variables, no dotenv
  loading, no telemetry, no generated SDK dependency, and `unsafe` forbidden in our code.
- OpenAI webhook verification checks the raw request signature and timestamp with
  constant-time HMAC verification. Applications must still deduplicate delivery IDs.

## Verify

```sh
cargo fmt --all --check
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo doc --workspace --no-deps --locked
python3 scripts/verify-packages.py
cargo audit --deny warnings
```

Tests use local mock HTTP servers and protocol fixtures; provider credentials
are unnecessary. They check request paths, verbs, headers, bodies, binary uploads,
SSE framing, extensions, error handling, retry behavior, and URL security. Live
provider examples and ignored live tests are opt-in and may incur charges.
After configuring a provider key and model name, run e.g.
`cargo test -p prism-openai --test live -- --ignored` to test that provider.
Passing mock tests does not
establish live service availability or access to beta features.

Licensed under MIT OR Apache-2.0.
