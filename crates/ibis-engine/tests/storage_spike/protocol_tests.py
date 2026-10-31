"""Fast refusal/oracle tests for the external measurement consumer."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("runner", Path(__file__).parents[1] / "storage_spike.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def evidence():
    return dict(case="bib-cache", records=10000, seed=1534, returncode=0, release=True,
                record_count=10000, first_hits=50, spawn_to_first_results_ms=700,
                first_results_ms=690, first_search_ms=5, loaded_ms=685,
                verified_ready_ms=1800, peak_process_mib=150, settled_process_mib=120,
                stages_ns={"index": 200_000_000, "snapshot_fingerprints": 100_000_000},
                queries=[{"query": query, "hits": 50, "ms": 5} for query in ["signals", "星図", "signlas"]])


class Protocol(unittest.TestCase):
    def test_incomplete_wrong_empty_and_stale_results_are_rejected(self):
        self.assertEqual(runner.validate(evidence()), [])
        changes = [dict(returncode=101), dict(release=False), dict(record_count=9999),
                   dict(seed=1535), dict(first_hits=0), dict(first_hits=49), dict(queries=[]),
                   dict(stages_ns={"index": 1, "unrelated": 1}), dict(spawn_to_first_results_ms=float("nan"))]
        for change in changes:
            with self.subTest(change=change):
                self.assertTrue(runner.validate(dict(evidence(), **change)))

    def test_baseline_excursion_rejects_the_whole_size_without_dropping_runs(self):
        runs = []
        for count in [10000, 50000]:
            for case in runner.CASES:
                for _ in range(5):
                    run = dict(evidence(), records=count, record_count=count, case=case, rejections=[])
                    runs.append(run)
        report = dict(runs=runs, baseline_validity={}, summaries=[])
        runner.summarize(report, 1.25)
        self.assertTrue(report["complete"])
        report = copy.deepcopy(dict(runs=runs, baseline_validity={}, summaries=[]))
        report["runs"][0]["spawn_to_first_results_ms"] *= 1.26
        runner.summarize(report, 1.25)
        self.assertFalse(report["complete"])
        self.assertEqual(len(report["runs"]), 50)
        self.assertFalse(report["baseline_validity"]["10000"]["valid"])
        self.assertTrue(report["baseline_validity"]["50000"]["valid"])


if __name__ == "__main__":
    unittest.main()
