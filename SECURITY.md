# Security

Do not post credentials or private prompts in public issues. Report vulnerabilities
using GitHub's private vulnerability reporting when available, or contact the
repository owner privately through their GitHub profile.

This project implements API clients. It never executes model tool calls. Treat
model output, retrieved files, web content, and tool arguments as untrusted input
in applications built on these clients. Independently implementing an SDK cannot
eliminate prompt injection in an application that consumes model output.

Custom base URLs must use HTTPS except on loopback; they receive the API key you
configure. Redirects and cross-origin resumable uploads are rejected. Request and
response JSON may contain sensitive information; do not log them indiscriminately.

Dependencies are pinned in the repository lockfile and checked in CI. Applications
also resolve transitive dependencies through their own Cargo.lock. Our crates
forbid unsafe code, but that does not make a claim about every dependency.
