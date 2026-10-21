# CI selection and required checks

Pull requests always run `Select CI jobs` and `CI required`. The latter is the
branch-protection check: it requires every selected job to succeed and rejects
failed, cancelled, unexpectedly skipped, or missing results. Intentionally
unselected jobs can be skipped without leaving a required workflow pending.
Changes to CI itself run every job. Unknown paths or a failed change comparison
also run the full suite.

| Changed input | Work required |
| --- | --- |
| Root Markdown guides and `docs/` | Selection and required gate only |
| One provider crate, including its compiled README/examples/tests | That crate's tests on Linux/macOS/Windows, Linux lint/docs, Rust 1.88, and archive verification |
| Shared core | The same checks for all four crates |
| Cargo manifests/lockfiles, toolchain or Cargo config | All affected Rust checks, archive verification, and dependency audit |
| Release Python scripts/tests | Python tooling tests |
| Package-verification script or release metadata | Archive verification and Python tooling tests |
| Root license texts | Archive verification |
| Workflows, CI helper/tests, or unknown inputs | Full suite |

Crate-local README changes still build: some crates include their README in Rust
documentation. A core change includes all provider dependents. PR comparisons use
the complete merge-base diff with NUL-delimited paths and rename detection
disabled, so deletions and both sides of a move are checked without an API file
count limit.

Manual runs, merge-queue runs, and reusable release validation run the full
suite. Release validation still checks the caller's exact `ref`; CI selection,
Python tooling tests, and the gate use the calling workflow's tooling revision.
The normal `push` to `main` does not repeat a PR's build. Release Please retains
its explicit CI dispatch for release PRs created with `GITHUB_TOKEN`.

Rust dependency build artifacts and Cargo downloads are cached per job and
toolchain. Formatting, Clippy, documentation tests, and documentation generation
run once on Linux; macOS and Windows retain native tests. Archive verification
keeps its fresh temporary build/registry identity. `cargo-audit` is cached as a
versioned executable, but advisory results are never cached. The audit runs on
dependency/CI changes, full validation, and once a week to detect new advisories
against an unchanged lockfile. Superseded PR runs are cancelled; release
validation is not cancelled by the CI concurrency policy.

## Repository setting

Protect `main` with the GitHub Actions `CI required` check (app ID `15368`),
including administrators. Requiring each conditional job separately would leave
documentation-only PRs waiting for matrix jobs that intentionally did not run.
The gate enforces all applicable jobs instead. Requiring an up-to-date branch is
optional; it is not needed to require the latest PR commit's checks and can
increase rebuilds.

The repository setting is applied through GitHub separately from the workflow
change. After introducing the gate, update older open PR branches to include
the workflow before merging them.

Local validation:

```sh
python3 -m unittest discover -s scripts/tests -v
actionlint .github/workflows/*.yml
```

GitHub references: [required check behavior](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks),
[workflow concurrency](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency),
and [branch protection](https://docs.github.com/en/rest/branches/branch-protection#update-branch-protection).
