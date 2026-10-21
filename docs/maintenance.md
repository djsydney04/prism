# Weekly cloud SDK maintenance

Prism calls the reusable [repo_maintainer](https://github.com/djsydney04/repo_maintainer)
workflow every Monday at **09:00 America/Los_Angeles**, including DST. It runs on
GitHub-hosted Linux while your computer is off. Schedules can be delayed.

The shared repository owns the Pi coding agent, provider adapters, checks,
proposal transfer, PR publishing and blocker notifications. Prism owns its
[weekly caller](../.github/workflows/sdk-maintenance.yml),
[configuration](../.github/maintainer.json), and
[maintenance instructions](../.github/maintenance/prompt.md). Both shared-workflow
references are pinned to the same commit, so changes in the shared repository
only take effect here after a reviewed pin update.

## Enable or switch providers

Add the selected provider's key in Prism's **Settings → Secrets and variables →
Actions → Secrets**. Keep the key out of code, workflow inputs, issues and chat.
Set the following repository **Variables** to choose inference:

| Provider variable `SDK_MAINTENANCE_PROVIDER` | Model variable `SDK_MAINTENANCE_MODEL` | Secret |
| --- | --- | --- |
| `groq` | `openai/gpt-oss-120b` (provider default) or `qwen/qwen3.8-27b` | `GROQ_API_KEY` |
| `google` | `gemini-3.8-flash` (provider default), `gemini-3.6-flash`, or `gemini-3.5-flash-lite` | `GEMINI_API_KEY` |

The workflow falls back to `google` when the provider variable is unset. Leave
the model variable unset to use the selected provider's default. A manual run
can override either setting. Change provider and model together.

To replace a key, update its named secret. Alternatively, set the optional
`SDK_MAINTENANCE_API_KEY` secret; it overrides the named key for the selected
provider. Delete that override to resume using provider-specific secrets.
No key has to be stored in the shared repository.

Use an **unbilled Google AI Studio project** or a **Groq free-plan organization**
for free-only inference. Pi does not change provider quotas. Another key in the
same Google project or Groq organization shares the existing quota. There is
no automatic key rotation, provider fallback or paid fallback.

During setup, Gemini 3.8 Flash returned a **20-request daily model limit**.
[Google's actual quotas depend on the project/model](https://ai.google.dev/gemini-api/docs/rate-limits).
[Groq's published free limits](https://console.groq.com/docs/rate-limits) for
GPT-OSS 120B are **1,000 requests/day, 8K tokens/minute and 200K tokens/day**;
account limits can differ. Groq therefore has more daily requests but a tighter
context budget. Neither free tier guarantees a complex maintenance task will fit.

Enable **Allow GitHub Actions to create and approve pull requests** in Prism's
Actions settings. The bot creates PRs; it never approves or merges them. Merge
the setup PR to activate the schedule, manually run **Weekly SDK maintenance**
once, and review its outcome. The duplicate local Codex schedule retires on its
next check after the cloud caller is merged.

## Behavior and review

- Read current official OpenAI, Gemini and Groq docs, source/guides, and existing
  maintenance PR feedback. Make focused changes only when evidence supports them.
- Use **Pi 1.3.0** with explicit read/edit/reference tools and fixed offline Rust
  checks. General shell commands, repository extensions and subagents are disabled.
  Provider keys are removed from the process environment before source tools run.
- Limit inference to 16 requests, including summaries, and a 20-minute deadline.
  Groq uses shorter excerpts, small file pages, compaction and 61-second request
  pacing. Google requests are paced 13 seconds apart. Oversized or incomplete
  work stops with a blocker; the partial patch is preserved.
- Validate the exact patch on a fresh runner: formatting, workspace tests and
  doctests, Clippy, rustdoc, Rust 1.88, package verification and dependency audit.
  A separate job gets write access only after validation succeeds.
- Open or update one PR on a `dylan/sdk-maintenance-…` branch, assigned to
  `djsydney04`, then explicitly dispatch cross-platform CI. No force pushes,
  merges, releases or live provider conformance tests.
- Limit edits to the configured SDK source, tests, examples and guides, at most
  25 regular files and 500 KB. Dependencies, automation and release files require
  human help.
- Keep completed/partial proposal artifacts for 30 days. New blockers create or
  update one assigned **SDK maintenance needs help** issue with an owner mention.
  Unchanged blockers and successful no-ops stay quiet. A later success closes the
  blocker issue. Enable GitHub assignment/mention notifications for delivery.

GitHub Actions runner and artifact usage follows Prism's GitHub plan. A provider
key from a billed account can incur charges; the workflow itself cannot enforce
that account's billing tier.

## Live diagnostic

Run **CI** manually with `maintenance_smoke` enabled, choose `google` or `groq`,
and optionally enter an allowed model ID. The diagnostic also calls the pinned
shared repository, using Prism's secrets. It asks Pi to repair a deliberately
failing disposable Rust fixture and run tests, then checks an independent
assertion. It cannot publish repository changes. It allows six model requests
and an eight-minute agent deadline. `gemini-3.5-flash-lite` can be chosen to test
Google without consuming the weekly 3.8 Flash quota.

Raw model logs are not uploaded. Failed diagnostics show a redacted excerpt.
Shared runner tests live in [repo_maintainer](https://github.com/djsydney04/repo_maintainer/actions).
