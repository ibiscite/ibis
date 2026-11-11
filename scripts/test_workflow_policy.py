"""Workflow rules that only the workflow's structure can show, read with a YAML
parser, never with regular expressions (decision 0035).

"CI result" is the one required check. GitHub counts a skipped required check as
passing, so the job must run whatever its needs did (`if: always()`) and must
need every validation job, or a red pull request could merge.
The issue reporter runs after the result and is outside the PR gate.
"""

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

try:
    import yaml
except ImportError:  # Linux runners and the Linux container always have it.
    yaml = None


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

    def test_runs_no_local_workflow_or_action(self):
        # ci-paths.mjs treats every other workflow as documentation, which holds
        # only while pull request CI runs nothing but ci.yml.
        local = []
        for name, job in self.workflow["jobs"].items():
            uses = [job.get("uses", "")] + [step.get("uses", "") for step in job.get("steps", [])]
            local += [f"{name}: {use}" for use in uses if str(use).startswith("./")]
        self.assertEqual(local, [])

    def test_dependency_rules_and_identities_remain_in_the_required_job(self):
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
        self.assertEqual(events["push"]["tags"], ["v*"])
        jobs = self.workflow["jobs"]
        self.assertIn("dependencies", jobs["result"]["needs"])
        self.assertNotIn("if", jobs["dependencies"])
        self.assertEqual(jobs["dependencies"]["permissions"], {"contents": "read", "issues": "write"})
        reporter = next(step for step in jobs["dependencies"]["steps"] if step.get("name") == "Report advisory failure")
        self.assertEqual(reporter["env"]["SCHEDULE"], "${{ github.event.schedule }}")

    def test_heavy_qualification_and_failure_report_stay_off_prs(self):
        jobs = self.workflow["jobs"]
        for name in ["webkit", "installed", "release-checks", "bibtex-fuzz"]:
            self.assertEqual(jobs[name]["if"], "github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'")
            self.assertIn(name, jobs["result"]["needs"])
        self.assertEqual(jobs["nightly-report"]["needs"], "result")
        events = self.workflow.get("on", self.workflow.get(True))
        self.assertNotIn("merge_group", events)

    def test_release_advisories_gate_publication(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/preview.yml").read_text(encoding="utf-8"))
        jobs = workflow["jobs"]
        self.assertIn("release-checks", jobs["publish"]["needs"])
        steps = jobs["release-checks"]["steps"]
        advisory = next(step for step in steps if step.get("with", {}).get("command") == "check advisories")
        self.assertEqual(advisory["if"], "startsWith(github.ref, 'refs/tags/')")
        self.assertFalse(advisory.get("continue-on-error", False))

    def test_each_ubuntu_installer_restores_its_package_cache_first(self):
        for name, profile in [("desktop", "desktop"), ("web", "web")]:
            steps = self.workflow["jobs"][name]["steps"]
            cache_index, cache = next((i, step) for i, step in enumerate(steps) if str(step.get("uses", "")).startswith("actions/cache@"))
            self.assertEqual(cache["with"]["path"], "/var/cache/apt/archives")
            self.assertEqual(cache["with"]["key"], f"apt-ubuntu-24.04-{profile}-${{{{ steps.apt.outputs.key }}}}")
            chooser = next(i for i, step in enumerate(steps) if step.get("id") == "apt")
            installer = next(i for i, step in enumerate(steps) if f"bash scripts/ci-ubuntu-deps.sh {profile}" in step.get("run", "").splitlines())
            self.assertLess(chooser, cache_index)
            self.assertLess(cache_index, installer)

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
