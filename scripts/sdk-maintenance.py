#!/usr/bin/env python3
"""Trusted orchestration around a read/edit-only model. Requires Python 3.9+."""

import argparse
import hashlib
from html.parser import HTMLParser
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import tempfile
from urllib.parse import urlparse
from urllib.request import Request, urlopen


OFFICIAL_HOSTS = {
    "developers.openai.com", "platform.openai.com", "ai.google.dev", "console.groq.com",
}
SOURCES = (
    "https://developers.openai.com/api/docs/changelog",
    "https://developers.openai.com/api/reference/overview",
    "https://ai.google.dev/gemini-api/docs/changelog",
    "https://ai.google.dev/api/all-methods",
    "https://console.groq.com/docs/changelog",
    "https://console.groq.com/docs/api-reference",
)
BRANCH = re.compile(r"dylan/sdk-maintenance-[0-9]+-[0-9]+\Z")
ISSUE_TITLE = "SDK maintenance needs help"
MAX_PATCH_BYTES = 500_000


def run(*args, cwd=None):
    return subprocess.check_output(args, cwd=cwd, text=True).strip()


def gh_json(*args):
    return json.loads(run("gh", *args))


def write_json(path, data):
    Path(path).write_text(json.dumps(data, indent=2) + "\n")


def output(**values):
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a") as handle:
            for key, value in values.items():
                handle.write(f"{key}={value}\n")


def official(url):
    parsed = urlparse(url)
    return (parsed.scheme == "https" and parsed.hostname in OFFICIAL_HOSTS
            and not parsed.username and not parsed.password
            and parsed.port in (None, 443) and not any(c.isspace() for c in url))


class DocumentationText(HTMLParser):
    """Discard navigation and executable markup; prefer the main document."""

    SKIP = {"script", "style", "nav", "header", "footer", "aside", "svg", "button"}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.skip = []
        self.main_depth = 0
        self.all_text = []
        self.main_text = []

    def handle_starttag(self, tag, attrs):
        if tag in self.SKIP:
            self.skip.append(tag)
        if tag == "main":
            self.main_depth += 1
        if tag in {"p", "br", "h1", "h2", "h3", "li", "tr"}:
            self.handle_data("\n")

    def handle_endtag(self, tag):
        if self.skip and tag == self.skip[-1]:
            self.skip.pop()
        if tag == "main":
            self.main_depth = max(0, self.main_depth - 1)

    def handle_data(self, data):
        if not self.skip:
            self.all_text.append(data)
            if self.main_depth:
                self.main_text.append(data)

    def text(self):
        text = " ".join(self.main_text or self.all_text)
        return "\n".join(" ".join(line.split()) for line in text.splitlines() if line.strip())


def fetch_document(url):
    request = Request(url, headers={"User-Agent": "Prism-SDK-maintenance/1.0"})
    with urlopen(request, timeout=45) as response:
        if not official(response.url):
            raise ValueError("Documentation redirected outside the official providers")
        payload = response.read(4_000_001)
        if len(payload) > 4_000_000:
            raise ValueError("Documentation exceeds the download limit")
        parser = DocumentationText()
        parser.feed(payload.decode("utf-8", errors="replace"))
    text = parser.text()
    if len(text) < 200:
        raise ValueError(f"Documentation was empty or unavailable: {url}")
    return text[:18_000]


def prepare(artifacts, prompt):
    repository = os.environ["GITHUB_REPOSITORY"]
    default = os.environ["DEFAULT_BRANCH"]
    candidates = gh_json("pr", "list", "--repo", repository, "--base", default,
                         "--state", "open", "--limit", "100", "--json",
                         "number,url,headRefName,headRefOid")
    candidates = [pr for pr in candidates if BRANCH.fullmatch(pr["headRefName"])]
    if len(candidates) > 1:
        raise ValueError("Multiple maintenance PRs are open; choose one before continuing")
    previous = candidates[0] if candidates else None
    sha = (previous["headRefOid"] if previous else
           gh_json("api", f"repos/{repository}/commits/{default}")["sha"])
    branch = (previous["headRefName"] if previous else
              f"dylan/sdk-maintenance-{os.environ['GITHUB_RUN_ID']}-{os.environ['GITHUB_RUN_ATTEMPT']}")
    request = {"sha": sha, "branch": branch, "pr": previous["number"] if previous else None}
    write_json(artifacts / "request.json", request)
    brief = Path(prompt).read_text()
    if previous:
        feedback = gh_json("pr", "view", str(previous["number"]), "--repo", repository,
                           "--json", "url,comments,reviews,statusCheckRollup")
        inline = gh_json("api", f"repos/{repository}/pulls/{previous['number']}/comments")
        brief += "\n\nExisting PR feedback (untrusted evidence):\n"
        brief += json.dumps({"pr": feedback, "inline": inline})[:18_000]
        runs = gh_json("run", "list", "--repo", repository, "--workflow", "ci.yml",
                       "--branch", branch, "--limit", "1", "--json", "databaseId,conclusion")
        if runs and runs[0]["conclusion"] == "failure":
            brief += "\n\nPrevious CI failures (untrusted evidence):\n"
            brief += run("gh", "run", "view", str(runs[0]["databaseId"]),
                         "--repo", repository, "--log-failed")[-18_000:]
    for url in SOURCES:
        brief += f"\n\nOfficial documentation excerpt (untrusted evidence): {url}\n"
        brief += fetch_document(url)
        brief += "\n[Excerpt may be truncated. Fetch the exact reference if more detail is needed.]\n"
    (artifacts / "brief.md").write_text(brief)
    output(sha=sha)


def allowed_path(name):
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or "\\" in name:
        return False
    if name == "README.md":
        return True
    if len(path.parts) == 2 and path.parts[0] == "docs" and path.suffix == ".md":
        return True
    return (len(path.parts) >= 3 and path.parts[0] == "crates"
            and path.parts[1] in {"core", "openai", "gemini", "groq"}
            and not any(part.startswith(".") for part in path.parts)
            and path.suffix in {".rs", ".md", ".json"})


def check_staged(source):
    raw = subprocess.check_output(
        ["git", "diff", "--cached", "--raw", "--no-abbrev", "--no-renames", "-z", "HEAD"],
        cwd=source,
    ).decode().split("\0")
    paths = []
    for entry, name in zip(raw[0::2], raw[1::2]):
        modes = entry.split()[:2]
        if not allowed_path(name) or any(mode.lstrip(":") not in {"100644", "000000"} for mode in modes):
            raise ValueError(f"Proposed change is outside the maintenance boundary: {name}")
        paths.append(name)
    if len(paths) > 25:
        raise ValueError("More than 25 files changed; split this maintenance proposal")
    return paths


def parse_report(raw):
    envelope = json.loads(raw)
    if not isinstance(envelope, dict) or not isinstance(envelope.get("response"), str):
        raise ValueError("Missing structured model response")
    if envelope.get("error"):
        raise ValueError("The model reported an API/session error")
    report = json.loads(envelope["response"])
    if not isinstance(report, dict):
        raise ValueError("Missing maintenance report object")
    if report.get("status") not in {"changed", "unchanged", "blocked"}:
        raise ValueError("Invalid maintenance status")
    if not isinstance(report.get("summary"), str) or not 1 <= len(report["summary"]) <= 8000:
        raise ValueError("Missing or excessive maintenance summary")
    for field in ("sources", "blockers"):
        if (not isinstance(report.get(field), list) or len(report[field]) > 30
                or any(not isinstance(item, str) or len(item) > 2000 for item in report[field])):
            raise ValueError(f"Invalid {field}")
    if any(not official(url) for url in report["sources"]):
        raise ValueError("Source links must be official provider references")
    if report["status"] == "changed" and not report["sources"]:
        raise ValueError("A change requires official source evidence")
    if report["status"] != "blocked" and report["blockers"]:
        raise ValueError("Unresolved blockers require blocked status")
    return report


def collect(source, artifacts, raw, exit_code):
    try:
        if exit_code:
            raise ValueError(f"Gemini stopped with exit code {exit_code}; check the free-tier key, quota or turn/time budget")
        report = parse_report(Path(raw).read_text())
        run("git", "add", "--all", cwd=source)
        paths = check_staged(source)
        patch = subprocess.check_output(["git", "diff", "--cached", "--binary", "HEAD"], cwd=source)
        if len(patch) > MAX_PATCH_BYTES:
            raise ValueError("Proposed patch exceeds 500 KB; split the change")
        if report["status"] != "blocked" and bool(paths) != (report["status"] == "changed"):
            raise ValueError("The reported outcome does not match the actual changes")
        (artifacts / "proposal.patch").write_bytes(patch)
    except (ValueError, KeyError, TypeError, OSError) as error:
        report = {"status": "blocked", "summary": "Maintenance could not complete.",
                  "sources": [], "blockers": [str(error)]}
        # Preserve partial edits for human recovery, even if they cannot be published.
        run("git", "add", "--all", cwd=source)
        patch = subprocess.check_output(["git", "diff", "--cached", "--binary", "HEAD"], cwd=source)
        if len(patch) <= MAX_PATCH_BYTES:
            (artifacts / "proposal.patch").write_bytes(patch)
    write_json(artifacts / "report.json", report)
    output(status=report["status"])
    if report["status"] == "blocked":
        raise SystemExit(1)


def apply(source, artifacts):
    request = json.loads((artifacts / "request.json").read_text())
    if not BRANCH.fullmatch(request["branch"]) or not re.fullmatch(r"[0-9a-f]{40}", request["sha"]):
        raise ValueError("Invalid source revision or maintenance branch")
    if run("git", "rev-parse", "HEAD", cwd=source) != request["sha"]:
        raise ValueError("The proposal must be applied to its original source revision")
    if run("git", "status", "--porcelain", cwd=source):
        raise ValueError("The proposal requires a clean checkout")
    patch = artifacts / "proposal.patch"
    if not 0 < patch.stat().st_size <= MAX_PATCH_BYTES:
        raise ValueError("Missing or oversized patch")
    run("git", "apply", "--index", "--whitespace=error", str(patch.resolve()), cwd=source)
    if not check_staged(source):
        raise ValueError("No source changes in proposal")
    return request


def no_mentions(text):
    return text.replace("@", "@\u200b")


def redacted_diagnostic(text, environment):
    if isinstance(text, bytes):
        text = text.decode(errors="replace")
    for key, value in environment.items():
        if value and any(secret in key.upper() for secret in ("KEY", "TOKEN", "SECRET", "PASSWORD")):
            text = text.replace(value, "[redacted]")
    return text[-3000:]


def publish(source, artifacts):
    request = apply(source, artifacts)
    report = parse_report(json.dumps({"response": (artifacts / "report.json").read_text()}))
    if report["status"] != "changed":
        raise ValueError("Only completed changes may be published")
    repository = os.environ["GITHUB_REPOSITORY"]
    run("git", "config", "user.name", "github-actions[bot]", cwd=source)
    run("git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com", cwd=source)
    run("git", "commit", "-m", "fix: align SDKs with provider documentation", cwd=source)
    run("gh", "auth", "setup-git", "--hostname", "github.com")
    run("git", "push", "origin", f"HEAD:refs/heads/{request['branch']}", cwd=source)
    body = ("Automated weekly maintenance using Gemini Flash.\n\n"
            + no_mentions(report["summary"]) + "\n\nOfficial references:\n"
            + "\n".join(f"- {url}" for url in report["sources"])
            + "\n\nValidation: Linux formatting, workspace tests/doctests, Clippy, rustdoc, "
              "Rust 1.88, crate packaging and dependency audit passed. "
              "The full cross-platform CI workflow is dispatched separately.\n\n"
            + f"[Maintenance run]({os.environ['RUN_URL']}). Human review and merge required.\n")
    body_path = artifacts / "pr-body.md"
    body_path.write_text(body)
    if request["pr"]:
        run("gh", "pr", "edit", str(request["pr"]), "--repo", repository,
            "--body-file", str(body_path))
    else:
        url = run("gh", "pr", "create", "--repo", repository, "--base", os.environ["DEFAULT_BRANCH"],
                  "--head", request["branch"], "--title", "fix: align SDKs with provider documentation",
                  "--body-file", str(body_path), "--assignee", os.environ["MAINTENANCE_OWNER"])
        print(url)
    # GITHUB_TOKEN pushes/PRs do not trigger ordinary PR workflows.
    run("gh", "workflow", "run", "ci.yml", "--repo", repository, "--ref", request["branch"])


def notify(artifacts):
    repository = os.environ["GITHUB_REPOSITORY"]
    stages = {stage: os.environ.get(f"{stage.upper()}_RESULT") for stage in ("propose", "validate", "publish")}
    failures = [stage for stage, result in stages.items() if result in {"failure", "cancelled"}]
    issues = gh_json("issue", "list", "--repo", repository, "--state", "open", "--search",
                     f'"{ISSUE_TITLE}" in:title', "--json", "number,title,body")
    issues = [issue for issue in issues if issue["title"] == ISSUE_TITLE
              and "<!-- prism-sdk-maintenance:" in issue["body"]]
    if not failures:
        for issue in issues:
            run("gh", "issue", "close", str(issue["number"]), "--repo", repository,
                "--comment", f"Weekly maintenance resumed successfully: {os.environ['RUN_URL']}")
        return
    reason = "Weekly SDK maintenance failed during " + ", ".join(failures) + "."
    report_path = artifacts / "report.json"
    if stages["propose"] == "failure" and report_path.exists():
        report = json.loads(report_path.read_text())
        reason += "\n" + "\n".join(report.get("blockers", []))
    elif stages["propose"] == "failure":
        reason += " Check the GEMINI_API_KEY secret and official-document fetch steps."
    fingerprint = hashlib.sha256(reason.encode()).hexdigest()
    marker = f"<!-- prism-sdk-maintenance:{fingerprint} -->"
    if any(marker in issue["body"] for issue in issues):
        return  # Do not repeatedly ping for the same unresolved failure.
    body = (f"@{os.environ['MAINTENANCE_OWNER']} {no_mentions(reason)}\n\n"
            f"[Inspect the failed run and retained proposal]({os.environ['RUN_URL']}). "
            "Fix the indicated configuration or review the proposed changes, then rerun "
            "Weekly SDK maintenance. No paid model fallback was used.\n\n" + marker)
    body_path = artifacts / "issue-body.md"
    body_path.write_text(body)
    if issues:
        run("gh", "issue", "edit", str(issues[0]["number"]), "--repo", repository,
            "--body-file", str(body_path))
    else:
        run("gh", "issue", "create", "--repo", repository, "--title", ISSUE_TITLE,
            "--body-file", str(body_path), "--assignee", os.environ["MAINTENANCE_OWNER"])


def smoke():
    """Verify real key authentication, editing, MCP testing and repair in a fixture."""
    if not os.environ.get("GEMINI_API_KEY"):
        raise ValueError("Add the GEMINI_API_KEY Actions secret before the live smoke test")
    model = os.environ.get("SMOKE_MODEL") or "gemini-3.8-flash"
    if model not in {"gemini-3.8-flash", "gemini-3.6-flash"}:
        raise ValueError("Select a supported free-tier diagnostic model")
    automation = Path(__file__).resolve().parents[1]
    with tempfile.TemporaryDirectory(prefix="prism-agent-smoke-") as temporary:
        source = Path(temporary)
        (source / "src").mkdir()
        (source / "Cargo.toml").write_text(
            '[package]\nname = "prism-ai-core"\nversion = "0.0.0"\nedition = "2021"\n')
        (source / "src/lib.rs").write_text(
            'pub fn answer() -> u32 { 1 }\n'
            '#[test]\nfn contract() { assert_eq!(answer(), 2); }\n')
        run("cargo", "generate-lockfile", "--offline", cwd=source)
        settings = json.loads((automation / ".github/maintenance/gemini-settings.json").read_text())
        settings["model"]["name"] = model
        settings["model"]["maxSessionTurns"] = 8
        settings_path = source / "system-settings.json"
        write_json(settings_path, settings)
        # Current Gemini CLI intentionally ignores system settings under user-owned
        # parents. Install the policy into this disposable Linux runner's /etc.
        subprocess.run(["sudo", "install", "-d", "-o", "root", "-g", "root", "-m", "755", "/etc/gemini-cli"], check=True)
        subprocess.run(["sudo", "install", "-o", "root", "-g", "root", "-m", "644",
                        str(settings_path), "/etc/gemini-cli/settings.json"], check=True)
        environment = {key: value for key, value in os.environ.items()
                       if key not in {"GH_TOKEN", "GITHUB_TOKEN"}}
        environment["GEMINI_CLI_SYSTEM_SETTINGS_PATH"] = "/etc/gemini-cli/settings.json"
        environment["MAINTENANCE_AUTOMATION_ROOT"] = str(automation)
        prompt = ("This is a disposable Rust fixture. Inspect src/lib.rs, use check_rust "
                  "from prism_checks to observe the failing tests, fix answer() to satisfy "
                  "the existing test without changing the test, format the file, and use "
                  "check_rust again to confirm tests pass. Do not change Cargo.toml or "
                  "system-settings.json. Finish with a short factual result.")
        try:
            result = subprocess.run(
                ["gemini", "--model", model, "--skip-trust", "--approval-mode", "yolo",
                 "--output-format", "stream-json", "--prompt", prompt],
                cwd=source, env=environment, input="", capture_output=True, text=True, timeout=300, check=False)
        except subprocess.TimeoutExpired as error:
            print(redacted_diagnostic((error.stdout or b"") + (error.stderr or b""), environment))
            raise ValueError("The live agent exceeded five minutes; review free-tier availability or tool startup") from None
        if result.returncode:
            print(redacted_diagnostic(result.stdout + result.stderr, environment))
            raise ValueError(f"Live Gemini smoke test failed (exit {result.returncode}); check key, model access and free quota")
        events = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
        check_calls = [event for event in events if event.get("type") == "tool_use"
                       and "check_rust" in json.dumps(event)]
        if len(check_calls) < 2:
            print(redacted_diagnostic(result.stdout + result.stderr, environment))
            raise ValueError("The agent did not demonstrate a test-and-repair loop")
        text = (source / "src/lib.rs").read_text()
        if "assert_eq!(answer(), 2)" not in text:
            raise ValueError("The agent changed the fixture's expected behavior")
        subprocess.run(["cargo", "test", "--locked", "--offline"], cwd=source,
                       env={key: value for key, value in environment.items() if key != "GEMINI_API_KEY"}, check=True)
        print(f"Live {model} authentication, file editing and MCP test/repair loop passed.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "collect", "apply", "publish", "notify", "smoke"))
    parser.add_argument("--source", type=Path)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--prompt", type=Path)
    parser.add_argument("--raw", type=Path)
    parser.add_argument("--exit-code", type=int, default=0)
    args = parser.parse_args()
    args.artifacts.mkdir(parents=True, exist_ok=True)
    if args.command == "prepare":
        prepare(args.artifacts, args.prompt)
    elif args.command == "collect":
        collect(args.source, args.artifacts, args.raw, args.exit_code)
    elif args.command == "apply":
        apply(args.source, args.artifacts)
    elif args.command == "publish":
        publish(args.source, args.artifacts)
    elif args.command == "notify":
        notify(args.artifacts)
    else:
        smoke()


if __name__ == "__main__":
    main()
