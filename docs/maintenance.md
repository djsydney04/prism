# Weekly cloud SDK maintenance

[Weekly SDK maintenance](../.github/workflows/sdk-maintenance.yml) runs on
GitHub-hosted Linux every Monday at **09:00 America/Los_Angeles**, including DST.
It can also be started manually from GitHub Actions. Your computer can be off.
GitHub schedules can be delayed; this is not a precise delivery-time guarantee.

## Enable it

1. Create a Gemini API key in [Google AI Studio](https://aistudio.google.com/api-keys)
   using a project **without billing enabled**. Check that the project has free
   quota for `gemini-3.8-flash` in its
   [active limits](https://ai.google.dev/gemini-api/docs/rate-limits).
2. Add the key as the repository Actions secret **`GEMINI_API_KEY`**. Never commit
   it or paste it in issues, PRs or workflow inputs.
3. In Settings → Actions → General → Workflow permissions, enable **Allow GitHub
   Actions to create and approve pull requests**. The workflow only creates PRs;
   it does not approve or merge them.
4. Merge the setup PR, then run **Weekly SDK maintenance** manually once and
   review its results before relying on the schedule. Once the cloud run works,
   pause the duplicate local Codex automation.

For a small live diagnostic, manually run **CI** with **maintenance_smoke**
enabled. This uses the secret to test an intentionally failing disposable Rust
fixture: the agent must call the check tool, edit the implementation and rerun
tests successfully. It cannot publish repository changes.
The diagnostic model defaults to 3.8 Flash; 3.6 Flash can be chosen explicitly
when the 3.8 free quota is unavailable. This does not change the weekly model.

The workflow uses the short-lived repository `GITHUB_TOKEN` for PRs and issues;
it needs no personal access token. The inference job has read access to GitHub.
The publish job gets write access only after independent validation succeeds.
Ordinary PR workflows do not start from `GITHUB_TOKEN` writes, so the publisher
explicitly dispatches the existing cross-platform CI workflow for the PR branch.

## Model and agent choice

The model is **Gemini 3.8 Flash**; the coding agent is **Gemini CLI 0.65.0**.
They are different components: the model reasons about changes, while the agent
reads and edits files, fetches references and iterates on check results. Codex is
not required. Pi or another provider-neutral coding agent could also be used,
but changing the agent alone does not establish better coding quality. This
setup uses Google's own maintained agent and its native API authentication.

Free API options reviewed **2026-10-20**:

| Provider | Free-tier evidence | Fit for this job |
| --- | --- | --- |
| Gemini | [3.8 Flash has free input/output](https://ai.google.dev/gemini-api/docs/pricing#gemini-3.8-flash), [1M input context and function calling](https://ai.google.dev/gemini-api/docs/models/gemini-3.8-flash); active quota is project specific | Selected for repository context, documented agentic coding support and a native coding agent |
| Groq | [GPT-OSS 120B: 30 RPM, 1,000 requests/day, 8,000 tokens/minute, 200,000 tokens/day](https://console.groq.com/docs/rate-limits) | Usable with another agent, but the token-per-minute budget constrains repository-sized prompts |
| Mistral | [Free API mode, no card required](https://docs.mistral.ai/getting-started/quickstarts/studio/activate-and-generate-api-key); model access and limits depend on the account | An alternative after checking the account's coding-model quota |
| Cerebras | [Current offer is a one-time $5 credit expiring after 30 days](https://www.cerebras.ai/pricing) | Not an ongoing free-tier choice for this schedule |

This is a practical starting choice, not a benchmark proving the strongest
coding model. Account quota, availability and terms can change. Google's free
tier [may use submitted content to improve its products](https://ai.google.dev/gemini-api/docs/pricing).
Only this public repository and public provider references are needed here.

## Run behavior

- Read the latest official references/changelogs for OpenAI, Gemini and Groq,
  current source and guides, and any existing maintenance PR's review/CI feedback.
- Use a maximum of 16 agent turns and a 20-minute agent deadline. The agent can
  read/edit files and fetch specific official references. Its `check_rust` tool
  runs bounded offline format/test/Clippy commands so it can repair failures.
  Arbitrary shell commands, paid search grounding and subagents are disabled.
- Recheck the exported patch in a separate job, then run Linux CI: formatting,
  workspace tests and doctests, Clippy, rustdoc, Rust 1.88, package verification
  and dependency audit. Failed or incomplete work cannot reach the publisher.
- Open one maintenance PR on a `dylan/sdk-maintenance-…` branch, assigned to
  `djsydney04`, or update the existing one. Never force push, merge or release.
- Only SDK Rust/JSON files, crate Markdown, root README and provider Markdown
  guides can change automatically. Dependencies, release files and automation
  require human help. Each proposal is limited to 25 files and 500 KB.
- Preserve partial work in the run's proposal artifact for 30 days. A new blocker
  creates or updates one assigned **SDK maintenance needs help** issue with an
  `@djsydney04` mention; an unchanged blocker produces no repeated mention. A
  later successful run closes the issue. Enable GitHub assignment/mention
  notifications for delivery. Successful runs without changes stay quiet.

There is **no automatic paid model fallback**. Keeping the AI Studio project
unbilled is what enforces free-only inference; a key from a billed project can
incur charges even for this same model. Rate limits produce a blocked run rather
than switching providers. GitHub Actions runner/storage usage follows the
repository's own GitHub plan and applicable limits.

For authentication, quota or failed checks, inspect the linked run, repair the
configuration or proposal, and rerun the workflow. Raw model logs are not
uploaded as artifacts; failed diagnostics print a redacted excerpt.
Live credentialed provider conformance tests are never run by this job.

The local orchestration tests require no model key:

```sh
python3 -m unittest discover -s scripts/tests -v
```

References: [Gemini CLI headless mode](https://geminicli.com/docs/cli/headless/),
[Gemini CLI configuration](https://geminicli.com/docs/reference/configuration/),
[GitHub schedules](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#schedule).
