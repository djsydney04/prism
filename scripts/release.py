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
    return next(p["version"] for p in metadata["packages"] if p["name"] == "prism-ai-core")


def check(tag):
    expected = version()
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
        for dependency in package["dependencies"]:
            if dependency["name"] == "prism-ai-core" and dependency["req"] != f"^{expected}":
                raise SystemExit(f"Update the prism-ai-core dependency in {name}")
    print(f"Validated {tag}: {', '.join(CRATES)}")


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
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("version")
    validate = commands.add_parser("check")
    validate.add_argument("--tag", required=True)
    commands.add_parser("publish")
    args = parser.parse_args()
    if args.command == "version":
        print(version())
    elif args.command == "check":
        check(args.tag)
    else:
        publish()
