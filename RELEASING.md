# Releasing

The four crates use a synchronized workspace version. Make and review API changes
on GitHub; run the full CI checks, independent review, and package verification
before cutting a release. Update `CHANGELOG.md` and the installation tag in README.

## GitHub release

1. Update `[workspace.package].version` and the three path dependency versions of
   `sydney-ai-core`. Rebuild the lockfile and run all checks.
2. Commit and push changes. Wait for GitHub Actions to pass for that exact commit.
3. Create an annotated `vX.Y.Z` tag and push it. The release workflow verifies the
   tag against every package version and creates a GitHub release with crate archives.

Git dependencies with `tag = "vX.Y.Z"` are immediately usable. Registry publication
requires a crates.io account and package ownership, independently of GitHub access.

## crates.io publication

Authenticate locally using `cargo login`, or add a scoped `CARGO_REGISTRY_TOKEN`
GitHub Actions secret. Never commit a token or paste one into an issue or chat.
The manual `Publish crates` workflow runs only after release checks and uses the
`crates-io` GitHub environment, which can enforce repository approval policies.
It publishes the core first, then all three SDKs after the core is indexed.

For manual publication:

```sh
cargo publish -p sydney-ai-core --locked
# Wait for the core version to appear in the registry, then:
cargo publish -p sydney-openai --locked
cargo publish -p sydney-gemini --locked
cargo publish -p sydney-groq --locked
```

Use `cargo package --workspace --locked` on current stable Cargo to package and
verify the workspace together before its dependencies are published. Cargo uses
the packaged sibling crates during this verification. Individual `cargo publish`
operations still require publishing core first so registry clients can resolve it.

Prefer `python3 scripts/verify-packages.py` when verifying repeatedly. It assigns
each run a fresh temporary target directory, avoiding Cargo's local-registry cache
of an earlier unpublished core with the same version. It saves the four verified
archives and `SHA256SUMS` to `target/package/`. CI uses this wrapper too.

Treat published versions as immutable. Breaking changes during 0.x require a new
minor version; compatible fixes use a patch version. Never force-move release tags.
