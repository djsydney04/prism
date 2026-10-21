#!/usr/bin/env python3
"""Select affected CI jobs and fail the required gate on missing or failed checks."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
PACKAGES = {
    "core": "prism-ai-core",
    "openai": "prism-openai",
    "gemini": "prism-gemini",
    "groq": "prism-groq",
}
FLAGS = ("rust", "packaging", "audit", "tooling")
JOBS = {
    "checks": "rust",
    "platform-tests": "rust",
    "msrv": "rust",
    "package": "packaging",
    "audit": "audit",
    "release-tooling": "tooling",
}
DOCS = {"README.md", "CHANGELOG.md", "RELEASE_NOTES.md", "RELEASING.md", "SECURITY.md"}


def plan(paths=None, scheduled=False):
    if scheduled:
        return {"rust": False, "packaging": False, "audit": True,
                "tooling": False, "packages": []}
    if paths is None:
        return {**dict.fromkeys(FLAGS, True), "packages": list(PACKAGES.values())}
    result = {**dict.fromkeys(FLAGS, False), "packages": []}
    packages = set()
    for path in paths:
        if (path.startswith(".github/")
                or path in {"scripts/ci.py", "scripts/tests/test_ci.py"}):
            return plan()
        if (path.startswith(".cargo/")
                or path in {"Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml"}):
            packages.update(PACKAGES.values())
            result["packaging"] = result["audit"] = True
            continue
        if path in DOCS or path.startswith("docs/"):
            continue
        if path in {"LICENSE-APACHE", "LICENSE-MIT"}:
            result["packaging"] = True
        elif path.startswith("crates/"):
            pieces = path.split("/")
            if len(pieces) < 3 or pieces[1] not in PACKAGES:
                return plan()
            packages.update(PACKAGES.values() if pieces[1] == "core" else [PACKAGES[pieces[1]]])
            result["packaging"] = True
            if pieces[-1] in {"Cargo.toml", "Cargo.lock"}:
                result["audit"] = True
        elif path in {"scripts/release.py", "scripts/tests/test_release.py"}:
            result["tooling"] = True
        elif path == "scripts/verify-packages.py":
            result["packaging"] = True
            result["tooling"] = True
        elif path in {"release-please-config.json", ".release-please-manifest.json", "version.txt"}:
            result["packaging"] = True
            result["tooling"] = True
        else:
            # New build inputs must not accidentally receive a documentation-only pass.
            return plan()
    result["rust"] = bool(packages)
    result["packages"] = [package for package in PACKAGES.values() if package in packages]
    return result


def changed_paths(event):
    pull_request = event["pull_request"]
    base, head = (pull_request[side]["sha"] for side in ("base", "head"))
    if not all(re.fullmatch(r"[0-9a-fA-F]{40}|[0-9a-fA-F]{64}", sha) for sha in (base, head)):
        raise ValueError("Pull request revisions must be full commit hashes")
    # No rename detection: both the old and new path must influence selection.
    output = subprocess.check_output(
        ["git", "diff", "--name-only", "--no-renames", "-z", f"{base}...{head}", "--"], cwd=ROOT)
    return [path.decode("utf-8", errors="surrogateescape") for path in output.split(b"\0") if path]


def event_plan(event_name, event_path, force_full=False):
    if force_full:
        return plan()
    if event_name == "schedule":
        return plan(scheduled=True)
    if event_name != "pull_request":
        # Manual runs, merge queues and reusable release validation run the full suite.
        return plan()
    try:
        return plan(changed_paths(json.loads(Path(event_path).read_text(encoding="utf-8"))))
    except (KeyError, TypeError, ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"Unable to determine changed paths; running all checks: {error}", file=sys.stderr)
        return plan()


def outputs_for(selected):
    return {**{key: str(selected[key]).lower() for key in FLAGS},
            "packages": " ".join(selected["packages"])}


def verify(needs):
    if needs.get("changes", {}).get("result") != "success":
        raise ValueError("CI job selection did not succeed")
    outputs = needs["changes"].get("outputs", {})
    if any(outputs.get(key) not in {"true", "false"} for key in FLAGS):
        raise ValueError("CI job selection returned missing or invalid outputs")
    packages = outputs.get("packages", "").split()
    if (len(packages) != len(set(packages)) or not set(packages).issubset(PACKAGES.values())
            or bool(packages) != (outputs["rust"] == "true")):
        raise ValueError("CI job selection returned invalid Rust packages")
    errors = []
    for job, flag in JOBS.items():
        result = needs.get(job, {}).get("result")
        expected = outputs[flag] == "true"
        if (expected and result != "success") or (not expected and result not in {"success", "skipped"}):
            errors.append(f"{job}: {result or 'missing'} (required={expected})")
    if errors:
        raise ValueError("CI checks did not pass: " + "; ".join(errors))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("plan", "verify"))
    args = parser.parse_args()
    if args.command == "verify":
        verify(json.loads(os.environ["CI_NEEDS_JSON"]))
        print("Every applicable CI check passed.")
        return
    selected = event_plan(os.environ.get("GITHUB_EVENT_NAME", ""),
                          os.environ.get("GITHUB_EVENT_PATH", ""),
                          os.environ.get("CI_FORCE_FULL") == "true")
    outputs = outputs_for(selected)
    print(json.dumps(selected, indent=2))
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as destination:
        for key, value in outputs.items():
            destination.write(f"{key}={value}\n")


if __name__ == "__main__":
    main()
