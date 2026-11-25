"""Workflow rules that only the workflow's structure can show, read with a YAML
parser, never with regular expressions.

"CI result" is the required merge check and confirms main before publication.
Main and daily/manual runs test every platform. Pull requests select checks by
changed paths. Daily/manual runs every qualifier. The reporter is outside the
result.
"""

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

try:
    import yaml
except ImportError:  # Linux runners and the Linux container always have it.
    yaml = None


FORK_GUARD = "github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository"


def result_job(workflow):
    jobs = workflow["jobs"]
    named = [key for key, job in jobs.items() if job.get("name") == "CI result"]
    if len(named) != 1:
        raise AssertionError(f"expected one job named 'CI result', found {named}")
    return named[0], jobs


class CiResultTest(unittest.TestCase):
    def setUp(self):
        if yaml is None:
            if sys.platform.startswith("linux"):
                self.fail("PyYAML is required on Linux to check the workflow")
            self.skipTest("PyYAML is not installed on this system")
        text = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.workflow = yaml.safe_load(text)

    def test_runs_whatever_its_needs_did(self):
        key, jobs = result_job(self.workflow)
        self.assertEqual(str(jobs[key].get("if", "")).strip(), "always()")

    def test_needs_every_other_job(self):
        key, jobs = result_job(self.workflow)
        needs = jobs[key].get("needs", [])
        needs = [needs] if isinstance(needs, str) else list(needs)
        self.assertEqual(sorted(needs), sorted(name for name in jobs if name not in {key, "nightly-report"}))

    @unittest.skipUnless(sys.platform.startswith("linux"), "contract report runs on Linux")
    def test_contract_report_failure_only_writes_an_advisory_notice(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/contract-report.yml").read_text(encoding="utf-8"))
        self.assertEqual(workflow["permissions"], {"contents": "read"})
        steps = workflow["jobs"]["report"]["steps"]
        self.assertIs(steps[0]["with"]["persist-credentials"], False)
        for job in self.workflow["jobs"].values():
            self.assertFalse(any("contract-compat.mjs" in step.get("run", "") for step in job.get("steps", [])))
        # The reporter is unavailable in the temporary checkout.
        with tempfile.TemporaryDirectory() as directory:
            summary = Path(directory) / "summary.md"
            result = subprocess.run(
                ["bash", "-e", "-c", steps[-1]["run"]], cwd=directory,
                env={**os.environ, "GITHUB_STEP_SUMMARY": str(summary)},
                capture_output=True, text=True, timeout=5, check=True,
            )
            self.assertIn("::notice::Contract report skipped.", result.stdout)
            self.assertIn("Contract report skipped.", summary.read_text(encoding="utf-8"))

    def test_rust_tests_all_three_systems_when_selected(self):
        events = self.workflow.get("on", self.workflow.get(True))
        self.assertEqual(set(events), {"push", "pull_request", "schedule", "workflow_dispatch"})
        self.assertEqual(events["push"], {"branches": ["main"]})
        self.assertEqual(events["pull_request"], {"branches": ["main"]})
        jobs = self.workflow["jobs"]
        for name in ["plan", "interface", "web", "dependencies"]:
            self.assertEqual(jobs[name]["if"], FORK_GUARD)
        # Planned jobs need the plan, which a fork's run skips, so they skip too.
        self.assertEqual(jobs["rust"]["if"], "needs.plan.outputs.rust == 'true'")
        self.assertEqual(jobs["pictures"]["if"], "needs.plan.outputs.pictures == 'true'")
        self.assertEqual(jobs["conformance"]["if"], "needs.plan.outputs.csl == 'true'")
        self.assertEqual(jobs["rust"]["strategy"]["matrix"]["os"], ["ubuntu-24.04", "macos-15", "windows-2025"])
        native_checkout = next(step for step in jobs["rust"]["steps"] if "actions/checkout@" in step.get("uses", ""))
        self.assertNotIn("path", native_checkout["with"])  # The path guard uses the hosted workspace roots.
        self.assertEqual(jobs["rust"]["needs"], ["plan", "interface"])
        self.assertEqual(jobs["pictures"]["needs"], "plan")
        self.assertEqual(jobs["conformance"]["needs"], "plan")
        self.assertEqual(jobs["release-checks"]["needs"], "plan")
        self.assertEqual(jobs["release-checks"]["if"], "needs.plan.outputs.workspace == 'true'")

    def test_a_newer_push_replaces_a_pull_requests_run_but_never_mains(self):
        concurrency = self.workflow["concurrency"]
        self.assertEqual(concurrency["group"], "ci-${{ github.event_name }}-${{ github.event_name == 'push' && github.sha || github.ref }}")
        self.assertEqual(concurrency["cancel-in-progress"], "${{ github.event_name == 'pull_request' }}")

    def test_the_plan_selects_jobs_and_ci_result_checks_each_selection(self):
        jobs = self.workflow["jobs"]
        plan = jobs["plan"]
        self.assertEqual(plan["outputs"], {name: f"${{{{ steps.plan.outputs.{name} }}}}" for name in ["rust", "workspace", "pictures", "csl"]})
        checkout = next(step for step in plan["steps"] if "actions/checkout@" in step.get("uses", ""))
        self.assertEqual(checkout["with"]["fetch-depth"], 2)
        self.assertNotIn("ref", checkout["with"])
        choose = next(step for step in plan["steps"] if step.get("id") == "plan")
        self.assertEqual(choose["run"], "node scripts/ci-plan.mjs")
        self.assertEqual(choose["env"]["EVENT"], "${{ github.event_name }}")
        self.assertEqual(choose["env"]["LABELS"], "${{ toJSON(github.event.pull_request.labels.*.name) }}")
        self.assertEqual(choose["env"]["SELECTION"], "${{ vars.CI_SELECTION }}")
        self.assertEqual(choose["env"]["MACOS_RUNNER_OVERRIDE"], "${{ github.event.pull_request.user.login != 'dependabot[bot]' && fromJSON(vars.RUNNERS || '{}')['macos-15'] && 'true' || '' }}")
        self.assertFalse(choose.get("continue-on-error", False))
        result = next(step for step in jobs["result"]["steps"] if step.get("run") == "scripts/ci-result.sh")
        self.assertEqual(result["env"]["SELECTED"], "rust:${{ needs.plan.outputs.rust }}:${{ needs.rust.result }} pictures:${{ needs.plan.outputs.pictures }}:${{ needs.pictures.result }} csl:${{ needs.plan.outputs.csl }}:${{ needs.conformance.result }} release:${{ needs.plan.outputs.workspace }}:${{ needs.release-checks.result }}")
        self.assertEqual(result["env"]["WORKSPACE"], "${{ needs.plan.outputs.workspace }}")
        self.assertEqual(result["env"]["RESULTS"], " ".join(f"${{{{ needs.{name}.result }}}}" for name in ["plan", "interface", "web", "dependencies"]))
        # Every job the result needs is either always required, planned or qualification.
        planned = {"rust", "pictures", "conformance", "release-checks"}
        qualification = {"webkit", "fuzz", "installed"}
        self.assertEqual(set(jobs["result"]["needs"]), {"plan", "interface", "web", "dependencies"} | planned | qualification)
        for name, job in jobs.items():
            if "needs.plan.outputs" in str(job.get("if", "")):
                self.assertIn(name, planned)

    def test_only_workspace_steps_follow_the_workspace_selection(self):
        steps = self.workflow["jobs"]["rust"]["steps"]
        conditional = {step["name"] for step in steps if "needs.plan.outputs.workspace" in str(step.get("if", ""))}
        self.assertEqual(conditional, {"Lint the workspace", "Test", "Install Pandoc", "Compare rendering from BibTeX and from CSL export"})
        by_name = {step.get("name"): step for step in steps}
        for name in ["Lint the app", "Test and build the app", "Check generated files (Linux)", "Format"]:
            self.assertNotIn("needs.plan", str(by_name[name].get("if", "")), name)

    def test_selected_conformance_job_runs_the_suite(self):
        steps = self.workflow["jobs"]["conformance"]["steps"]
        for command in ["node scripts/csl-suite.mjs", "cargo test --locked -p ibis-csl --lib conformance -- --ignored"]:
            step = next(step for step in steps if step.get("run") == command)
            self.assertNotIn("if", step)
            self.assertFalse(step.get("continue-on-error", False))

    def test_dependency_rules_and_identities_remain_in_confirmation(self):
        jobs = self.workflow["jobs"]
        self.assertEqual(jobs["dependencies"]["name"], "Dependency policy and commit identities")
        steps = jobs["dependencies"]["steps"]
        policy = [step for step in steps if step.get("with", {}).get("command") == "check licenses bans sources"]
        self.assertEqual(len(policy), 1)
        self.assertFalse(policy[0].get("continue-on-error", False))
        self.assertNotIn("if", policy[0])
        advisories = [step for step in steps if step.get("with", {}).get("command") == "check advisories"]
        self.assertEqual(len(advisories), 1)
        self.assertEqual(advisories[0]["id"], "advisories")
        self.assertIs(advisories[0]["continue-on-error"], True)
        self.assertLess(advisories[0]["timeout-minutes"], jobs["dependencies"]["timeout-minutes"])
        reporter = next(step for step in steps if step.get("run") == "bash scripts/ci-advisories.sh report")
        self.assertEqual(reporter["if"], "${{ !cancelled() && steps.advisories.outcome == 'failure' }}")
        self.assertEqual(reporter["env"]["BLOCKING"], "${{ steps.advisory_policy.outputs.blocking }}")
        self.assertFalse(reporter.get("continue-on-error", False))
        identity = next(step for step in steps if step.get("name") == "Public commit identities")
        self.assertEqual(identity["run"], "node scripts/check-agent-identity.mjs")
        self.assertNotIn("if", identity)

    def test_weekly_advisory_run_reuses_the_dependency_job(self):
        events = self.workflow.get("on", self.workflow.get(True))
        self.assertIn({"cron": "43 5 * * 1"}, events["schedule"])
        jobs = self.workflow["jobs"]
        self.assertIn("dependencies", jobs["result"]["needs"])
        self.assertEqual(jobs["dependencies"]["permissions"], {"contents": "read", "issues": "write"})
        reporter = next(step for step in jobs["dependencies"]["steps"] if step.get("name") == "Report advisory failure")
        self.assertEqual(reporter["env"]["SCHEDULE"], "${{ github.event.schedule }}")

    def test_daily_qualification_and_failure_reporting_remain_required(self):
        jobs = self.workflow["jobs"]
        self.assertIn("release-checks", jobs["result"]["needs"])
        for name in ["webkit", "installed", "fuzz"]:
            self.assertEqual(jobs[name]["if"], "github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'")
            self.assertIn(name, jobs["result"]["needs"])
        self.assertEqual(jobs["nightly-report"]["needs"], "result")

    def test_release_advisories_gate_publication(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/preview.yml").read_text(encoding="utf-8"))
        jobs = workflow["jobs"]
        self.assertIn("release-checks", jobs["publish"]["needs"])
        steps = jobs["release-checks"]["steps"]
        advisory = next(step for step in steps if step.get("with", {}).get("command") == "check advisories")
        self.assertEqual(advisory["if"], "startsWith(github.ref, 'refs/tags/')")
        self.assertFalse(advisory.get("continue-on-error", False))

    def test_one_nightly_a_day_from_a_green_head(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/preview.yml").read_text(encoding="utf-8"))
        events = workflow.get("on", workflow.get(True))
        self.assertEqual(events["push"], {"tags": ["v*"]})
        self.assertEqual([entry["cron"] for entry in events["schedule"]], ["7 2 * * *", "37 4 * * *"])
        jobs = workflow["jobs"]
        self.assertEqual(jobs["nightly-commit"]["if"], "github.event.schedule == '7 2 * * *'")
        choose = next(step for step in jobs["nightly-commit"]["steps"] if step.get("id") == "choose")
        self.assertEqual(choose["env"]["SHA"], "${{ github.sha }}")
        self.assertLess(choose["run"].index("node scripts/ci-main.mjs"), choose["run"].index("node scripts/publication-guard.mjs nightly"))
        gate = "${{ !cancelled() && !inputs.update_test && (github.event_name != 'schedule' || needs.nightly-commit.outputs.build == 'true') }}"
        for name in ["notices", "windows-paste"]:
            self.assertEqual(jobs[name]["needs"], "nightly-commit")
            self.assertEqual(jobs[name]["if"], gate)
        # Every package carries the one notices folder (#1271): no build without it.
        self.assertEqual(jobs["build"]["needs"], ["nightly-commit", "notices"])
        self.assertEqual(jobs["build"]["if"], gate.replace(" }}", " && needs.notices.result == 'success' }}"))
        self.assertEqual(jobs["nightly"]["if"], "${{ !cancelled() && github.ref == 'refs/heads/main' && (github.event_name == 'schedule' || inputs.publish) && needs.build.result == 'success' && needs.verify.result == 'success' && needs.sbom.result == 'success' && needs.install.result == 'success' }}")
        # build needs the nightly choice, which a tag skips; anything after build that
        # relied on the implicit success() check would be skipped with it.
        for name in ["sign-macos", "verify", "sbom", "install", "publish", "stable-feed", "nightly"]:
            self.assertTrue(jobs[name]["if"].startswith("${{ !cancelled() && "), name)
        self.assertEqual(jobs["update-e2e"]["if"], "github.ref == 'refs/heads/main' && (github.event.schedule == '37 4 * * *' || inputs.update_test)")

    def test_nothing_installs_or_publishes_an_unverified_package(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/preview.yml").read_text(encoding="utf-8"))
        jobs = workflow["jobs"]
        verify = jobs["verify"]
        self.assertEqual(verify["needs"], ["build", "sign-macos"])
        targets = {entry["target"] for entry in verify["strategy"]["matrix"]["include"]}
        self.assertEqual(targets, {entry["target"] for entry in jobs["build"]["strategy"]["matrix"]["include"]})
        run = next(step for step in verify["steps"] if step.get("name") == "Verify every package")["run"]
        self.assertIn("--self-test", run)
        self.assertNotIn("continue-on-error", str(verify))
        # Each consumer names the verification and requires its success explicitly.
        for name in ["install", "publish", "nightly"]:
            self.assertIn("verify", jobs[name]["needs"], name)
            self.assertIn("needs.verify.result == 'success'", jobs[name]["if"], name)
        # The verifier holds no secret and writes nothing.
        self.assertNotIn("secrets.", str(verify))
        self.assertNotIn("permissions", verify)
        self.assertEqual(workflow["permissions"], {"contents": "read"})

    def test_every_published_package_has_an_attested_sbom(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/preview.yml").read_text(encoding="utf-8"))
        jobs = workflow["jobs"]
        sbom = jobs["sbom"]
        # Attested on the final bytes, after macOS signing, by a job without secrets.
        self.assertEqual(sbom["needs"], ["notices", "build", "sign-macos"])
        self.assertEqual(sbom["permissions"], {"contents": "read", "id-token": "write", "attestations": "write"})
        self.assertNotIn("secrets.", str(sbom))
        attests = [step for step in sbom["steps"] if str(step.get("uses", "")).startswith("actions/attest@")]
        self.assertEqual(len(attests), 2)
        self.assertTrue(all("sbom-path" in step["with"] for step in attests))
        targets = {entry["target"] for entry in sbom["strategy"]["matrix"]["include"]}
        self.assertEqual(targets, {entry["target"] for entry in jobs["build"]["strategy"]["matrix"]["include"]})
        for name in ["publish", "nightly"]:
            self.assertIn("sbom", jobs[name]["needs"], name)
            self.assertIn("needs.sbom.result == 'success'", jobs[name]["if"], name)
            runs = "\n".join(step.get("run", "") for step in jobs[name]["steps"])
            self.assertIn("bash scripts/verify-sboms.sh", runs, name)
            # Checked before anything is signed with the update key.
            steps = [step.get("name", "") for step in jobs[name]["steps"]]
            check = next(i for i, step in enumerate(jobs[name]["steps"]) if "verify-sboms" in step.get("run", ""))
            self.assertLess(check, steps.index("Sign the update packages"), name)
        # No new tool or action reaches a job that holds a signing key.
        for name in ["sign-macos", "publish", "nightly"]:
            uses = {str(step.get("uses", "")).split("@")[0] for step in jobs[name]["steps"] if step.get("uses")}
            self.assertNotIn("actions/attest", uses, name)

    def test_each_ubuntu_installer_restores_its_package_cache_first(self):
        for name, profile in [("rust", "desktop"), ("web", "web")]:
            steps = self.workflow["jobs"][name]["steps"]
            cache_index, cache = next((i, step) for i, step in enumerate(steps) if str(step.get("uses", "")).startswith("actions/cache@"))
            self.assertEqual(cache["with"]["path"], "/var/cache/apt/archives")
            self.assertEqual(cache["with"]["key"], f"apt-ubuntu-24.04-{profile}-${{{{ steps.apt.outputs.key }}}}")
            chooser = next(i for i, step in enumerate(steps) if step.get("id") == "apt")
            installer = next(i for i, step in enumerate(steps) if f"bash scripts/ci-ubuntu-deps.sh {profile}" in step.get("run", "").splitlines())
            self.assertLess(chooser, cache_index)
            self.assertLess(cache_index, installer)

    def test_jobs_that_can_run_on_our_machines_never_touch_cargo_home(self):
        # rust-cache restores and prunes CARGO_HOME/bin; on a self-hosted runner that
        # is the owner's own toolchain (it removed rustup from the Mac once).
        for name, job in self.workflow["jobs"].items():
            if "vars.RUNNERS" not in str(job.get("runs-on", "")):
                continue
            for step in job["steps"]:
                if "Swatinem/rust-cache" in step.get("uses", ""):
                    self.assertEqual(step.get("if"), "runner.environment == 'github-hosted'", name)

    def test_dependabot_pull_requests_stay_on_hosted_macos(self):
        runs_on = self.workflow["jobs"]["rust"]["runs-on"]
        self.assertIn(
            "github.event.pull_request.user.login != 'dependabot[bot]' && fromJSON(vars.RUNNERS || '{}')",
            runs_on,
        )

    def test_actions_are_pinned_to_immutable_revisions(self):
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            workflow = yaml.safe_load(path.read_text(encoding="utf-8"))
            for job in workflow["jobs"].values():
                for step in [job, *job.get("steps", [])]:
                    if "uses" not in step:
                        continue
                    use = str(step["uses"])
                    _, sep, revision = use.rpartition("@")
                    digest = revision.removeprefix("sha256:") if use.startswith("docker://") else revision
                    self.assertEqual(sep, "@", use)
                    self.assertEqual(len(digest), 64 if use.startswith("docker://") else 40, use)
                    self.assertTrue(all(char in "0123456789abcdef" for char in digest), use)


if __name__ == "__main__":
    unittest.main()
