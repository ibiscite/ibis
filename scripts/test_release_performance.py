"""Synthetic contracts for the diagnostic release-build comparison."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("release_performance", Path(__file__).with_name("release-performance.py"))
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)


class ReleasePerformanceTests(unittest.TestCase):
    def report(self, sha="a" * 40, scale=1):
        return perf.make_report(sha, [float(i * scale) for i in range(1, 21)],
                                [float(i * scale) for i in range(21, 41)],
                                {"memory_bytes": 1024 * scale, "wakeups_per_second": 0.5 * scale,
                                 "seconds": 10.0, "threads": 4})

    def test_p95_keeps_raw_samples_and_exact_source(self):
        report = self.report()
        self.assertEqual(report["source_sha"], "a" * 40)
        self.assertEqual(report["metrics"]["summon_observed_p95_ms"], 19)
        self.assertEqual(report["metrics"]["typing_observed_p95_ms"], 39)
        self.assertEqual(len(report["summon_samples_ms"]), 20)
        self.assertEqual(report["references"], 100000)
        self.assertEqual(report["build_profile"], "release")

    def test_compare_main_and_candidate_including_zero_baseline(self):
        baseline, candidate = self.report(), self.report("b" * 40, 2)
        baseline["metrics"]["idle_wakeups_per_second"] = 0
        rows = perf.compare(candidate, baseline)
        self.assertEqual(rows["summon_observed_p95_ms"]["change_percent"], 100)
        self.assertIsNone(rows["idle_wakeups_per_second"]["change_percent"])
        self.assertEqual(rows["idle_wakeups_per_second"]["delta"], 1)

    def test_never_compare_incomplete_nonfinite_or_different_measurements(self):
        for key, value in (("references", 1000), ("build_profile", "debug"), ("schema", 2)):
            candidate = self.report()
            candidate[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                perf.compare(candidate, self.report())
        for value in (True, -1, float("nan"), float("inf")):
            candidate = self.report()
            candidate["metrics"]["idle_memory_bytes"] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                perf.compare(candidate, self.report())

    def test_wakeup_delta_cannot_hide_missing_threads_or_counter_resets(self):
        before = {(10, 100): 20, (11, 101): 5}
        after = {(10, 100): 23, (11, 101): 7}
        self.assertEqual(perf.wakeup_delta(before, after), 5)
        for invalid in ({(10, 100): 23}, {(10, 100): 23, (11, 102): 7},
                        {(10, 100): 19, (11, 101): 7}):
            with self.subTest(invalid=invalid), self.assertRaises(perf.smoke.Unavailable):
                perf.wakeup_delta(before, invalid)

    def test_wakeups_are_scheduler_wakeups_not_context_switches(self):
        self.assertEqual(perf.parse_wakeups("synthetic (123, #threads: 1)\nnr_switches : 900\nnr_wakeups : 12\n"), 12)
        for text in ("nr_switches : 900\n", "nr_wakeups : -1\n", "nr_wakeups : unknown\n"):
            with self.subTest(text=text), self.assertRaises(perf.smoke.Unavailable):
                perf.parse_wakeups(text)


if __name__ == "__main__":
    unittest.main()
