"""Workflow rules that only the workflow's structure can show, read with a YAML
parser, never with regular expressions (decision 0035).

"CI result" is the one required check. GitHub counts a skipped required check as
passing, so the job must run whatever its needs did (`if: always()`) and must
need every other job, or a red pull request could merge.
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
        self.assertEqual(sorted(needs), sorted(name for name in jobs if name != key))

    def test_runs_no_local_workflow_or_action(self):
        # ci-paths.mjs treats every other workflow as documentation, which holds
        # only while pull request CI runs nothing but ci.yml.
        local = []
        for name, job in self.workflow["jobs"].items():
            uses = [job.get("uses", "")] + [step.get("uses", "") for step in job.get("steps", [])]
            local += [f"{name}: {use}" for use in uses if str(use).startswith("./")]
        self.assertEqual(local, [])


if __name__ == "__main__":
    unittest.main()
