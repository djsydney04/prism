"""Exercise change selection and fail-closed required checks without running CI tools."""
import copy
import importlib.util
import itertools
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("prism_ci", Path(__file__).resolve().parents[1] / "ci.py")
ci = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ci)


def needs_for(selected):
    needs = {"changes": {"result": "success", "outputs": ci.outputs_for(selected)}}
    needs.update({job: {"result": "success" if selected[flag] else "skipped"}
                  for job, flag in ci.JOBS.items()})
    return needs


class SelectionTests(unittest.TestCase):
    def test_documentation_only_changes_do_not_start_build_jobs(self):
        selected = ci.plan(["README.md", "docs/openai.md", "SECURITY.md", "CHANGELOG.md"])
        self.assertFalse(any(selected[key] for key in ci.FLAGS))
        self.assertEqual(selected["packages"], [])

    def test_provider_changes_select_only_the_affected_crates(self):
        for crate, package in ci.PACKAGES.items():
            if crate == "core":
                continue
            for file in ("src/lib.rs", "tests/contracts.rs", "examples/chat.rs", "README.md"):
                with self.subTest(crate=crate, file=file):
                    selected = ci.plan([f"crates/{crate}/{file}"])
                    self.assertEqual(selected["packages"], [package])
                    self.assertTrue(selected["rust"])
                    self.assertTrue(selected["packaging"])
                    self.assertFalse(selected["audit"])
                    self.assertFalse(selected["tooling"])

    def test_shared_core_changes_include_every_dependent_provider(self):
        selected = ci.plan(["crates/core/src/transport.rs"])
        self.assertEqual(selected["packages"], list(ci.PACKAGES.values()))

    def test_multiple_provider_changes_have_stable_package_order(self):
        selected = ci.plan(["crates/groq/src/lib.rs", "crates/openai/src/lib.rs"])
        self.assertEqual(selected["packages"], ["prism-openai", "prism-groq"])

    def test_dependency_changes_require_a_fresh_audit(self):
        for path in ("Cargo.toml", "Cargo.lock", ".cargo/config.toml", "rust-toolchain.toml"):
            with self.subTest(path=path):
                selected = ci.plan([path])
                self.assertEqual(selected["packages"], list(ci.PACKAGES.values()))
                self.assertTrue(selected["audit"])
                self.assertTrue(selected["packaging"])
                self.assertFalse(selected["tooling"])
        self.assertTrue(ci.plan(["crates/openai/Cargo.toml"])["audit"])

    def test_ci_configuration_and_unknown_build_inputs_run_everything(self):
        for path in (".github/workflows/ci.yml", ".github/workflows/publish.yml", "scripts/ci.py",
                     "scripts/tests/test_ci.py", "new-build-input", "crates/new-provider/src/lib.rs"):
            with self.subTest(path=path):
                self.assertEqual(ci.plan([path]), ci.plan())

    def test_release_script_changes_need_tooling_tests_without_rust_builds(self):
        selected = ci.plan(["scripts/release.py", "scripts/tests/test_release.py"])
        self.assertTrue(selected["tooling"])
        self.assertFalse(selected["rust"])
        self.assertFalse(selected["packaging"])
        self.assertFalse(selected["audit"])

    def test_package_inputs_retain_archive_verification(self):
        for path in ("scripts/verify-packages.py", "LICENSE-MIT", "version.txt"):
            with self.subTest(path=path):
                selected = ci.plan([path])
                self.assertTrue(selected["packaging"])
                self.assertFalse(selected["rust"])

    def test_changed_paths_are_unlimited_nul_delimited_and_include_rename_sources(self):
        paths = [f"docs/{i}.md" for i in range(400)] + ["crates/core/src/deleted.rs", "docs/new name.md"]
        event = {"pull_request": {"base": {"sha": "a" * 40}, "head": {"sha": "b" * 40}}}
        with patch.object(ci.subprocess, "check_output", return_value="\0".join(paths).encode() + b"\0") as git:
            changed = ci.changed_paths(event)
        self.assertEqual(changed, paths)
        command = git.call_args.args[0]
        self.assertIn("--no-renames", command)
        self.assertIn("-z", command)
        self.assertEqual(command[-2:], ["a" * 40 + "..." + "b" * 40, "--"])
        self.assertEqual(ci.plan(changed)["packages"], list(ci.PACKAGES.values()))

    def test_invalid_commit_arguments_never_reach_git(self):
        event = {"pull_request": {"base": {"sha": "--help"}, "head": {"sha": "b" * 40}}}
        with patch.object(ci.subprocess, "check_output") as git, self.assertRaises(ValueError):
            ci.changed_paths(event)
        git.assert_not_called()

    def test_diff_failures_run_everything_instead_of_skipping(self):
        with tempfile.TemporaryDirectory() as directory:
            event_path = Path(directory) / "event.json"
            event_path.write_text(json.dumps({"pull_request": {"base": {"sha": "a" * 40},
                                                                  "head": {"sha": "b" * 40}}}))
            for error in (ValueError("bad diff"), subprocess.CalledProcessError(1, ["git"])):
                with self.subTest(error=error), patch.object(ci, "changed_paths", side_effect=error):
                    self.assertEqual(ci.event_plan("pull_request", event_path), ci.plan())

    def test_release_manual_and_merge_queue_events_run_everything(self):
        for event in ("workflow_dispatch", "workflow_call", "merge_group", "push"):
            with self.subTest(event=event):
                self.assertEqual(ci.event_plan(event, "unused"), ci.plan())
        self.assertEqual(ci.event_plan("pull_request", "unused", force_full=True), ci.plan())

    def test_weekly_schedule_only_refreshes_dependency_advisories(self):
        selected = ci.event_plan("schedule", "unused")
        self.assertEqual([key for key in ci.FLAGS if selected[key]], ["audit"])

    def test_plan_command_writes_validated_job_outputs(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            with patch.dict(os.environ, {"GITHUB_EVENT_NAME": "workflow_dispatch", "GITHUB_OUTPUT": str(output)}, clear=True), \
                    patch("sys.argv", ["ci.py", "plan"]):
                ci.main()
            self.assertEqual(dict(line.split("=", 1) for line in output.read_text().splitlines()),
                             ci.outputs_for(ci.plan()))


class RequiredGateTests(unittest.TestCase):
    def test_required_gate_covers_every_workflow_job(self):
        workflow = (ci.ROOT / ".github/workflows/ci.yml").read_text()
        jobs = set(re.findall(r"^  ([a-z][a-z-]*):$", workflow.split("\njobs:\n", 1)[1], re.MULTILINE))
        self.assertEqual(jobs, {"changes", "required", *ci.JOBS})
        gate = workflow.split("\n  required:\n", 1)[1]
        dependencies = re.search(r"needs: \[([^]]+)\]", gate).group(1)
        self.assertEqual({job.strip() for job in dependencies.split(",")}, {"changes", *ci.JOBS})
        self.assertIn("if: ${{ always() }}", gate)

    def test_all_valid_selection_combinations_pass(self):
        for flags in itertools.product((False, True), repeat=len(ci.FLAGS)):
            selected = dict(zip(ci.FLAGS, flags))
            selected["packages"] = ["prism-openai"] if selected["rust"] else []
            with self.subTest(selected=selected):
                ci.verify(needs_for(selected))

    def test_every_required_job_rejects_failure_cancellation_skipping_or_absence(self):
        for job in ci.JOBS:
            for result in ("failure", "cancelled", "skipped", "timed_out", None):
                needs = needs_for(ci.plan())
                if result is None:
                    del needs[job]
                else:
                    needs[job]["result"] = result
                with self.subTest(job=job, result=result), self.assertRaises(ValueError):
                    ci.verify(needs)

    def test_unselected_jobs_cannot_hide_a_failed_or_missing_result(self):
        for job in ci.JOBS:
            needs = needs_for(ci.plan([]))
            needs[job]["result"] = "failure"
            with self.subTest(job=job), self.assertRaises(ValueError):
                ci.verify(needs)

    def test_failed_selection_or_invalid_outputs_never_pass(self):
        original = needs_for(ci.plan())
        for result in ("failure", "cancelled", "skipped", None):
            needs = copy.deepcopy(original)
            needs["changes"]["result"] = result
            with self.subTest(result=result), self.assertRaises(ValueError):
                ci.verify(needs)
        for key in ci.FLAGS:
            needs = copy.deepcopy(original)
            del needs["changes"]["outputs"][key]
            with self.subTest(key=key), self.assertRaises(ValueError):
                ci.verify(needs)

    def test_missing_unknown_or_duplicate_package_selection_never_passes(self):
        for packages in ("", "unknown-crate", "prism-openai prism-openai"):
            needs = needs_for(ci.plan())
            needs["changes"]["outputs"]["packages"] = packages
            with self.subTest(packages=packages), self.assertRaises(ValueError):
                ci.verify(needs)


if __name__ == "__main__":
    unittest.main()
