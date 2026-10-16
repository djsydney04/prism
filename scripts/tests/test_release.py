"""Behavioral tests for release validation and safe, resumable publication.

No Cargo process, registry lookup, publish, or sleep is performed by the release
helper while these tests run. Workflow output tests write only owned temporary
files, leaving the checkout unchanged.
"""

import copy
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from unittest.mock import call, patch


SCRIPT = Path(__file__).resolve().parents[1] / "release.py"
SPEC = importlib.util.spec_from_file_location("prism_release_under_test", SCRIPT)
release = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = release
SPEC.loader.exec_module(release)

VERSION = "0.2.0"
NAMES = ("prism-ai-core", "prism-openai", "prism-gemini", "prism-groq")


def metadata(version=VERSION):
    packages = []
    for name in NAMES:
        dependencies = []
        if name != "prism-ai-core":
            dependencies.append({
                "name": "prism-ai-core", "req": f"^{version}", "source": None,
                "path": str(release.ROOT / "crates" / "core"),
                "kind": None, "rename": None, "optional": False,
            })
        package_id = f"path+file://{release.ROOT}/{name}#{version}"
        packages.append({
            "name": name, "version": version, "dependencies": dependencies,
            "id": package_id,
            "manifest_path": str(release.ROOT / "crates" / name / "Cargo.toml"),
        })
    return {
        "packages": packages,
        "workspace_members": [p["id"] for p in packages],
        "workspace_root": str(release.ROOT),
    }


class ReleaseTestCase(unittest.TestCase):
    """Register context cleanup on Python versions predating enterContext()."""

    def context(self, manager):
        value = manager.__enter__()
        self.addCleanup(manager.__exit__, None, None, None)
        return value

    def setUp(self):
        self.version_file_contents = None
        version_file = release.ROOT / "version.txt"
        original_exists = Path.exists
        original_read = Path.read_text

        def exists(path):
            if path == version_file:
                return self.version_file_contents is not None
            return original_exists(path)

        def read(path, *args, **kwargs):
            if path == version_file:
                if self.version_file_contents is None:
                    raise FileNotFoundError(path)
                return self.version_file_contents
            return original_read(path, *args, **kwargs)

        self.context(patch.object(Path, "exists", new=exists))
        self.context(patch.object(Path, "read_text", new=read))


class MetadataTests(ReleaseTestCase):
    def setUp(self):
        super().setUp()
        self.output = self.context(redirect_stdout(io.StringIO()))
        self.cargo = self.context(patch.object(release.subprocess, "check_output"))
        self.use_metadata(metadata())

    def use_metadata(self, document):
        self.cargo.return_value = json.dumps(document).encode()

    def test_version_comes_from_core_even_when_package_order_changes(self):
        document = metadata()
        document["packages"] = list(reversed(document["packages"]))
        document["packages"][0]["version"] = "9.9.9"
        self.use_metadata(document)
        self.assertEqual(release.version(), VERSION)

    def test_matching_tag_validates_all_four_packages(self):
        release.check(f"v{VERSION}")
        self.assertIn(f"v{VERSION}", self.output.getvalue())

    def test_matching_prerelease_tag_is_supported(self):
        self.use_metadata(metadata("1.0.0-rc.2"))
        release.check("v1.0.0-rc.2")

    def test_malformed_and_mismatched_tags_are_rejected(self):
        for tag in ("", VERSION, "v0.2", "v0.2.0.0", "v0.2.0\n", "v0.2.0/extra",
                    "v0.2.0-", "v0.2.0-rc.1", "v0.3.0", "v00.2.0"):
            with self.subTest(tag=tag), self.assertRaises(SystemExit):
                release.check(tag)

    def test_missing_or_unexpected_workspace_packages_are_rejected(self):
        for name in NAMES:
            document = metadata()
            document["packages"] = [p for p in document["packages"] if p["name"] != name]
            self.use_metadata(document)
            with self.subTest(missing=name), self.assertRaises(SystemExit):
                release.check(f"v{VERSION}")
        document = metadata()
        extra = copy.deepcopy(document["packages"][0])
        extra["name"] = "unexpected-sdk"
        document["packages"].append(extra)
        self.use_metadata(document)
        with self.assertRaises(SystemExit):
            release.check(f"v{VERSION}")

    def test_each_provider_version_must_match_core(self):
        for name in NAMES[1:]:
            document = metadata()
            next(p for p in document["packages"] if p["name"] == name)["version"] = "0.1.0"
            self.use_metadata(document)
            with self.subTest(package=name), self.assertRaises(SystemExit):
                release.check(f"v{VERSION}")

    def test_core_dependency_requirement_must_match_release_version(self):
        for name in NAMES[1:]:
            document = metadata()
            package = next(p for p in document["packages"] if p["name"] == name)
            package["dependencies"][0]["req"] = "^0.1.0"
            self.use_metadata(document)
            with self.subTest(package=name), self.assertRaises(SystemExit):
                release.check(f"v{VERSION}")

    def test_every_provider_requires_a_core_dependency(self):
        for name in NAMES[1:]:
            document = metadata()
            package = next(p for p in document["packages"] if p["name"] == name)
            package["dependencies"] = []
            self.use_metadata(document)
            with self.subTest(package=name), self.assertRaises(SystemExit):
                release.check(f"v{VERSION}")

    def test_version_file_when_present_must_match_metadata(self):
        for text, valid in ((f"{VERSION}\n", True), ("0.1.0\n", False)):
            self.version_file_contents = text
            with self.subTest(contents=text):
                if valid:
                    release.check(f"v{VERSION}")
                else:
                    with self.assertRaises(SystemExit):
                        release.check(f"v{VERSION}")


class RegistryLookupTests(unittest.TestCase):
    def test_registry_lookup_uses_a_temporary_directory_outside_the_workspace(self):
        temporary_directory = Path("/tmp/prism-registry-unit-test")
        with patch.object(release.tempfile, "TemporaryDirectory") as temporary, \
                patch.object(release.subprocess, "run") as cargo:
            temporary.return_value.__enter__.return_value = str(temporary_directory)
            cargo.return_value.returncode = 0
            self.assertTrue(release.published("prism-openai", VERSION))
            arguments, keywords = cargo.call_args
            command = arguments[0]
            self.assertEqual(command[0:2], ["cargo", "info"])
            self.assertIn(f"prism-openai@{VERSION}", command)
            self.assertEqual(command[command.index("--registry") + 1], "crates-io")
            cwd = Path(keywords["cwd"]).resolve()
            self.assertEqual(cwd, temporary_directory.resolve())
            self.assertFalse(cwd.is_relative_to(release.ROOT.resolve()))
            temporary.return_value.__exit__.assert_called_once()

    def test_failed_registry_lookup_does_not_claim_publication(self):
        for status in (1, 101):
            with self.subTest(status=status), \
                    patch.object(release.tempfile, "TemporaryDirectory") as temporary, \
                    patch.object(release.subprocess, "run") as cargo:
                temporary.return_value.__enter__.return_value = "/tmp/prism-registry-unit-test"
                cargo.return_value.returncode = status
                self.assertFalse(release.published("prism-ai-core", VERSION))


class ResolveTests(ReleaseTestCase):
    def setUp(self):
        super().setUp()
        self.tag = f"v{VERSION}"
        self.sha = "a" * 40
        self.github_release = {"tagName": self.tag, "isDraft": False}
        self.events = []
        self.output = self.context(redirect_stdout(io.StringIO()))
        temporary = self.context(tempfile.TemporaryDirectory(prefix="prism-resolve-tests-"))
        self.workflow_output = Path(temporary) / "github-output"
        self.workflow_output.write_text("existing=value\n", encoding="utf-8")
        self.context(patch.dict(release.os.environ, {"GITHUB_OUTPUT": str(self.workflow_output)}, clear=True))
        self.read_command = self.context(patch.object(
            release.subprocess, "check_output", side_effect=self.command_output,
        ))
        self.run_command = self.context(patch.object(
            release.subprocess, "run", side_effect=self.command_status,
        ))

    def command_output(self, command, **kwargs):
        self.assertEqual(kwargs["cwd"], release.ROOT)
        self.assertFalse(kwargs.get("shell", False))
        if command[:2] == ["git", "rev-parse"]:
            self.assertEqual(command, ["git", "rev-parse", f"refs/tags/{self.tag}^{{commit}}"])
            self.events.append("resolve-tag")
            return self.sha + "\n"
        self.assertEqual(command[:3], ["gh", "release", "view"])
        self.assertEqual(command[3], self.tag)
        self.assertIn("--json", command)
        self.events.append("github-release")
        return json.dumps(self.github_release).encode()

    def command_status(self, command, **kwargs):
        self.assertEqual(command, ["git", "merge-base", "--is-ancestor", self.sha, "origin/main"])
        self.assertEqual(kwargs["cwd"], release.ROOT)
        self.assertTrue(kwargs["check"], "a failed ancestry check must stop resolution")
        self.assertFalse(kwargs.get("shell", False))
        self.events.append("verify-main-ancestry")
        return subprocess.CompletedProcess(command, 0)

    def assert_no_workflow_output(self):
        self.assertEqual(self.workflow_output.read_text(encoding="utf-8"), "existing=value\n")

    def test_malformed_tags_are_rejected_before_any_commands_or_outputs(self):
        for tag in ("", VERSION, "v0.2", "refs/tags/v0.2.0", "--help", "v0.2.0\n",
                    "v0.2.0/extra", "v0.2.0; echo unsafe", "v0.2.0$(anything)"):
            with self.subTest(tag=tag), self.assertRaises(SystemExit):
                release.resolve(tag, self.sha)
        self.read_command.assert_not_called()
        self.run_command.assert_not_called()
        self.assert_no_workflow_output()

    def test_malformed_expected_sha_is_rejected_before_any_commands_or_outputs(self):
        for sha in ("abc1234", "a" * 39, "a" * 41, "z" * 40, "a" * 40 + "\n", "--help"):
            with self.subTest(sha=sha), self.assertRaises(SystemExit):
                release.resolve(self.tag, sha)
        self.read_command.assert_not_called()
        self.run_command.assert_not_called()
        self.assert_no_workflow_output()

    def test_unexpected_tag_commit_stops_before_ancestry_and_github(self):
        with self.assertRaises(SystemExit):
            release.resolve(self.tag, "b" * 40)
        self.assertEqual(self.events, ["resolve-tag"])
        self.run_command.assert_not_called()
        self.assert_no_workflow_output()

    def test_tag_outside_main_history_stops_before_github_and_output(self):
        self.run_command.side_effect = subprocess.CalledProcessError(1, ["git", "merge-base"])
        with self.assertRaises(subprocess.CalledProcessError):
            release.resolve(self.tag, self.sha)
        self.assertTrue(self.run_command.call_args.kwargs["check"])
        command = self.run_command.call_args.args[0]
        self.assertEqual(command, ["git", "merge-base", "--is-ancestor", self.sha, "origin/main"])
        self.assertEqual(self.read_command.call_count, 1)
        self.assert_no_workflow_output()

    def test_github_draft_release_cannot_produce_publish_outputs(self):
        self.github_release["isDraft"] = True
        with self.assertRaises(SystemExit):
            release.resolve(self.tag, self.sha)
        self.assertEqual(self.events, ["resolve-tag", "verify-main-ancestry", "github-release"])
        self.assert_no_workflow_output()

    def test_github_release_tag_must_match_requested_tag(self):
        self.github_release["tagName"] = "v9.9.9"
        with self.assertRaises(SystemExit):
            release.resolve(self.tag, self.sha)
        self.assertEqual(self.events, ["resolve-tag", "verify-main-ancestry", "github-release"])
        self.assert_no_workflow_output()

    def test_valid_published_release_appends_exact_tag_and_commit_outputs(self):
        release.resolve(self.tag, self.sha)
        self.assertEqual(self.events, ["resolve-tag", "verify-main-ancestry", "github-release"])
        self.assertEqual(
            self.workflow_output.read_text(encoding="utf-8"),
            f"existing=value\ntag={self.tag}\nsha={self.sha}\n",
        )
        self.assertIn(self.tag, self.output.getvalue())
        self.assertIn(self.sha, self.output.getvalue())

    def test_explicit_sha_is_optional_but_main_ancestry_and_release_are_required(self):
        release.resolve(self.tag)
        self.assertEqual(self.events, ["resolve-tag", "verify-main-ancestry", "github-release"])
        self.assertIn(f"sha={self.sha}\n", self.workflow_output.read_text(encoding="utf-8"))

    def test_failed_github_lookup_never_produces_publish_outputs(self):
        original = self.read_command.side_effect

        def command_output(command, **kwargs):
            if command[0] == "gh":
                raise subprocess.CalledProcessError(1, command)
            return original(command, **kwargs)

        self.read_command.side_effect = command_output
        with self.assertRaises(subprocess.CalledProcessError):
            release.resolve(self.tag, self.sha)
        self.assertEqual(self.events, ["resolve-tag", "verify-main-ancestry"])
        self.assert_no_workflow_output()

    def test_no_output_environment_still_verifies_release_without_writing(self):
        with patch.dict(release.os.environ, {}, clear=True):
            release.resolve(self.tag, self.sha)
        self.assertEqual(self.events, ["resolve-tag", "verify-main-ancestry", "github-release"])
        self.assert_no_workflow_output()


class PublicationTests(ReleaseTestCase):
    def setUp(self):
        super().setUp()
        self.output = self.context(redirect_stdout(io.StringIO()))
        self.context(patch.dict(release.os.environ, {"CARGO_REGISTRY_TOKEN": "fake-unit-token"}, clear=True))
        self.metadata = self.context(patch.object(release.subprocess, "check_output", return_value=json.dumps(metadata()).encode()))
        self.version = self.context(patch.object(release, "version", return_value=VERSION))
        self.lookup = self.context(patch.object(release, "published"))
        self.cargo = self.context(patch.object(release.subprocess, "run"))
        self.sleep = self.context(patch.object(release.time, "sleep"))

    def published_commands(self):
        return [invocation.args[0] for invocation in self.cargo.call_args_list]

    def published_names(self):
        return [command[command.index("-p") + 1] for command in self.published_commands()]

    def test_no_token_fails_before_any_cargo_or_registry_work(self):
        with patch.dict(release.os.environ, {}, clear=True), self.assertRaises(SystemExit):
            release.publish()
        self.version.assert_not_called()
        self.metadata.assert_not_called()
        self.lookup.assert_not_called()
        self.cargo.assert_not_called()
        self.sleep.assert_not_called()

    def test_existing_versions_make_reruns_idempotent(self):
        self.lookup.return_value = True
        release.publish()
        self.assertEqual(self.lookup.call_args_list, [call(name, VERSION) for name in NAMES])
        self.cargo.assert_not_called()
        self.sleep.assert_not_called()

    def test_unpublished_crates_publish_core_first_in_dependency_order(self):
        self.lookup.side_effect = [False, True] * len(NAMES)
        release.publish()
        self.assertEqual(self.published_names(), list(NAMES))
        for invocation in self.cargo.call_args_list:
            command = invocation.args[0]
            self.assertIn("publish", command)
            self.assertIn("--locked", command)
            self.assertNotIn("fake-unit-token", command)
            self.assertEqual(invocation.kwargs["cwd"], release.ROOT)
            self.assertTrue(invocation.kwargs["check"])
        self.sleep.assert_not_called()

    def test_partial_release_resumes_without_republishing_existing_versions(self):
        self.lookup.side_effect = [True, False, True, True, False, True]
        release.publish()
        self.assertEqual(self.published_names(), ["prism-openai", "prism-groq"])

    def test_next_crate_waits_until_core_appears_in_registry(self):
        events = []
        answers = iter([False, False, True, True, True, True])

        def lookup(name, expected):
            events.append(("lookup", name, expected))
            return next(answers)

        def publish_command(command, **_kwargs):
            events.append(("publish", command[command.index("-p") + 1]))

        self.lookup.side_effect = lookup
        self.cargo.side_effect = publish_command
        self.sleep.side_effect = lambda seconds: events.append(("sleep", seconds))
        release.publish()
        self.assertEqual(events, [
            ("lookup", "prism-ai-core", VERSION), ("publish", "prism-ai-core"),
            ("lookup", "prism-ai-core", VERSION), ("sleep", 10),
            ("lookup", "prism-ai-core", VERSION),
            ("lookup", "prism-openai", VERSION), ("lookup", "prism-gemini", VERSION),
            ("lookup", "prism-groq", VERSION),
        ])

    def test_registry_timeout_stops_before_publishing_dependents(self):
        self.lookup.return_value = False
        with self.assertRaisesRegex(SystemExit, "prism-ai-core"):
            release.publish()
        self.assertEqual(self.published_names(), ["prism-ai-core"])
        self.assertEqual(self.lookup.call_count, 31)
        self.assertEqual(self.sleep.call_args_list, [call(10)] * 30)

    def test_publish_failure_propagates_without_polling_or_continuing(self):
        self.lookup.return_value = False
        self.cargo.side_effect = subprocess.CalledProcessError(101, ["cargo", "publish"])
        with self.assertRaises(subprocess.CalledProcessError):
            release.publish()
        self.assertEqual(self.published_names(), ["prism-ai-core"])
        self.lookup.assert_called_once_with("prism-ai-core", VERSION)
        self.sleep.assert_not_called()


if __name__ == "__main__":
    unittest.main()
