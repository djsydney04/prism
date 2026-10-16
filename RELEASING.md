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

`cargo package` for an unpublished SDK dependency cannot resolve core from the
registry until core is published. The release workflow uses a temporary local
registry patch only to verify downstream source packages before publication.
It does not modify the published manifests or advertise a registry release.

Treat published versions as immutable. Breaking changes during 0.x require a new
minor version; compatible fixes use a patch version. Never force-move release tags.
