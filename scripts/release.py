#!/usr/bin/env python3
"""Validate synchronized release versions and publish crates in dependency order."""
import argparse
import json
import os
import re
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CRATES = ("prism-ai-core", "prism-openai", "prism-gemini", "prism-groq")


def version():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"], cwd=ROOT))
    core = next((p for p in metadata["packages"] if p["name"] == "prism-ai-core"), None)
    if core is None:
        raise SystemExit("Workspace must contain prism-ai-core")
    return core["version"]


def check(tag):
    expected = version()
    version_file = ROOT / "version.txt"
    if version_file.exists() and version_file.read_text().strip() != expected:
        raise SystemExit("version.txt must match the workspace version")
    if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", tag) or tag != f"v{expected}":
        raise SystemExit(f"Tag must match workspace version v{expected}")
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"], cwd=ROOT))
    packages = {p["name"]: p for p in metadata["packages"]}
    if set(packages) != set(CRATES):
        raise SystemExit("Unexpected workspace packages")
    for name in CRATES:
        package = packages[name]
        if package["version"] != expected:
            raise SystemExit(f"Version mismatch in {name}")
        dependencies = package["dependencies"]
        if name != "prism-ai-core" and not any(d["name"] == "prism-ai-core" for d in dependencies):
            raise SystemExit(f"Missing prism-ai-core dependency in {name}")
        for dependency in dependencies:
            if dependency["name"] == "prism-ai-core" and dependency["req"] != f"^{expected}":
                raise SystemExit(f"Update the prism-ai-core dependency in {name}")
    print(f"Validated {tag}: {', '.join(CRATES)}")


def resolve(tag, expected_sha=""):
    if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", tag):
        raise SystemExit("Invalid release tag")
    if expected_sha and not re.fullmatch(r"[0-9a-fA-F]{40}", expected_sha):
        raise SystemExit("Expected SHA must be a full commit SHA")
    sha = subprocess.check_output(["git", "rev-parse", f"refs/tags/{tag}^{{commit}}"], cwd=ROOT, text=True).strip()
    if expected_sha and sha != expected_sha:
        raise SystemExit("Release tag does not match the expected commit")
    subprocess.run(["git", "merge-base", "--is-ancestor", sha, "origin/main"], cwd=ROOT, check=True)
    release = json.loads(subprocess.check_output(
        ["gh", "release", "view", tag, "--json", "tagName,isDraft"], cwd=ROOT))
    if release["isDraft"] or release["tagName"] != tag:
        raise SystemExit("Release must already be published on GitHub")
    print(f"Resolved {tag} at {sha}")
    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with open(output, "a", encoding="utf-8") as destination:
            destination.write(f"tag={tag}\nsha={sha}\n")


def published(name, expected):
    # Cargo uses the authenticated registry/index configuration; no tokens in URLs.
    # Run outside the workspace: cargo info otherwise resolves local packages and
    # can incorrectly report an unpublished workspace crate as registry-available.
    with tempfile.TemporaryDirectory(prefix="prism-registry-check-") as registry_dir:
        result = subprocess.run(["cargo", "info", f"{name}@{expected}", "--registry", "crates-io"],
                                cwd=registry_dir, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return result.returncode == 0


def publish():
    if not os.environ.get("CARGO_REGISTRY_TOKEN"):
        raise SystemExit("Configure CARGO_REGISTRY_TOKEN in the crates-io environment before publishing")
    expected = version()
    for name in CRATES:
        if published(name, expected):
            print(f"{name}@{expected} already published; skipping")
            continue
        subprocess.run(["cargo", "publish", "-p", name, "--locked"], cwd=ROOT, check=True)
        for _ in range(30):
            if published(name, expected):
                break
            time.sleep(10)
        else:
            raise SystemExit(f"Waiting for {name}@{expected} exceeded five minutes; retry the workflow")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT, help="Released SDK checkout (publisher code stays on main)")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("version")
    validate = commands.add_parser("check")
    validate.add_argument("--tag", required=True)
    resolver = commands.add_parser("resolve")
    resolver.add_argument("--tag", required=True)
    resolver.add_argument("--expected-sha", default="")
    commands.add_parser("publish")
    args = parser.parse_args()
    ROOT = args.root.resolve()
    if args.command == "version":
        print(version())
    elif args.command == "check":
        check(args.tag)
    elif args.command == "resolve":
        resolve(args.tag, args.expected_sha)
    else:
        publish()
