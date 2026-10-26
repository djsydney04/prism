# Weekly cloud SDK maintenance

Prism calls [repo_maintainer](https://github.com/djsydney04/repo_maintainer) every
Monday at **09:00 America/Los_Angeles**, including DST. It runs on GitHub Actions
while your computer is off. GitHub schedules may be delayed.

## Configure one YAML file

Edit [`.maintainer.yml`](../.maintainer.yml) to choose the model provider, docs
sources, instructions, editable files and Rust checks. The small
[caller workflow](../.github/workflows/sdk-maintenance.yml) owns the weekly schedule
and pins both the reusable workflow and implementation to the same exact commit.

| `agent.provider` | Default model | Actions secret |
| --- | --- | --- |
| `groq` | `openai/gpt-oss-120b` | `GROQ_API_KEY` |
| `google` | `gemini-3.8-flash` | `GEMINI_API_KEY` |
| `openai` | `gpt-5.4` | `OPENAI_API_KEY` |
| `anthropic` | `claude-sonnet-4-6` | `ANTHROPIC_API_KEY` |

Prism explicitly pins Google with `gemini-3.5-flash-lite` in its YAML
agent section. This pair completed the live OpenAI SDK assessment with passing
offline Rust contract tests:

```yaml
agent:
  provider: google
  model: gemini-3.5-flash-lite
  allow_paid: false
  max_requests: 16
```

Scheduled runs and manual runs without overrides use this exact pair and the
`GEMINI_API_KEY` secret. The diagnostic defaults to the same provider/model.
Selecting Groq with a blank diagnostic model uses `openai/gpt-oss-120b`.
Another provider or model requires an explicit configuration change or manual
override. The provider defaults in the table apply only when `agent.model` is
omitted; Prism keeps the verified model explicit.

To use OpenAI, change the YAML agent section:

```yaml
agent:
  provider: openai
  model: gpt-5.4       # Optional; omit to use the provider default.
  allow_paid: true
  max_requests: 16
```

For Anthropic, use `provider: anthropic`, for example with
`model: claude-sonnet-4-6`. Both paid providers require explicit `allow_paid: true`.
The maintainer uses native Rust adapters for OpenAI Responses and Anthropic
Messages; this does not add a new Rust SDK to Prism.

Add the key in **Settings → Secrets and variables → Actions → Secrets**. Keep
keys out of YAML, Dockerfiles, workflow inputs and chat. Replacing a named secret
changes the key. The optional `SDK_MAINTENANCE_API_KEY` overrides the selected
provider's named key. No copy is needed in the shared repository.

Manual workflow inputs may override provider/model for one run; paid inference
still needs explicit opt-in. YAML is the normal source of settings, and the
previous `SDK_MAINTENANCE_PROVIDER`/`SDK_MAINTENANCE_MODEL` variables are no longer
used. JSON configurations remain supported by the shared runner.

## Run locally with Docker

With `repo_maintainer` checked out beside Prism, run from Prism:

```sh
../repo_maintainer/maintainer check
../repo_maintainer/maintainer run
```

The wrapper builds the Docker image and handles your user ID and output paths.
`check` validates YAML without an API key or inference. `run` needs the selected
key in your shell or the shared checkout's ignored `.env` file. It reads committed
HEAD through a read-only mount and produces a reviewable patch/report under
`.maintainer-output/`. The cloud workflow publishes PRs after independent checks.
See the [Docker/Compose guide](https://github.com/djsydney04/repo_maintainer#quick-start-with-docker)
for direct container usage. Docker does not create a weekly schedule by itself.

## Activate, review and get notifications

Enable **Allow GitHub Actions to create and approve pull requests** in Prism's
Actions settings. The cloud schedule is enabled on the default branch.
Run **Weekly SDK maintenance** manually to
review the result. The duplicate local Codex schedule retires on its next check
after the cloud caller is merged.

Each pass completes one focused assessment or compatible fix, prioritizing
OpenAI. It names the area checked and any remaining work; a successful scoped
assessment does not claim that every SDK endpoint was audited. The 16-request
limit includes transcript summaries, and the agent sees its remaining budget.

The agent reads current official docs and existing maintenance PR feedback, then
uses explicit file/reference tools and bounded offline Rust checks. It loads no
repository plugins, general shell commands or subagents. Provider credentials
are removed from the environment before source tools and checks run.

A fresh runner validates formatting, workspace tests/doctests, Clippy, rustdoc,
Rust 1.88, crate packaging and dependency audit. A separate job gets write access
only after validation passes, opens/updates one assigned PR and dispatches
cross-platform CI. It never merges, force pushes, releases or runs credentialed
provider conformance tests. Changes are limited to configured source/guides,
25 regular files and 500 KB; dependencies and automation require human help.

New blockers update one assigned **SDK maintenance needs help** issue with an
owner mention. Repeated identical blockers and successful no-ops stay quiet;
recovery closes the issue. Enable GitHub assignment/mention notifications.
Completed or partial proposal artifacts remain available for 30 days.

Inference is capped at 16 requests including summaries, with a 20-minute agent
deadline. Groq uses smaller pages, compaction and 61-second pacing for its tighter
free token budget; Google uses 13-second pacing. There is no automatic provider,
key or paid fallback. An unbilled Google project or Groq free-plan organization
enforces free-only inference; billed accounts can incur charges. Another key in
the same project/organization shares its quota.

For a live repair diagnostic, manually run **Maintenance diagnostic**, choose a
provider/model, and explicitly enable paid usage if appropriate. The default
`repair` scope repairs a disposable failing Rust fixture, reruns tests and checks
an independent assertion. The `repository` scope reads the actual SDK and its
configured official documentation, and retains a report and proposed patch as
the `sdk-maintenance-diagnostic` artifact. Both scopes run with read-only GitHub
permissions and cannot publish source changes. Repository diagnostics verify
proposal generation; the weekly workflow independently validates changed patches
before publishing a PR. New OpenAI/Anthropic SDK routing is covered by
simulated transport tests; live paid tests require their corresponding keys.
