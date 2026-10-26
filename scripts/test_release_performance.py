"""Synthetic contracts for the diagnostic release-build comparison."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

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


    class Native:
        def __init__(self, stale=False):
            self.value, self.visible, self.selected = "", True, False
            self.stale = stale
            self.shows = 0
            self.typed = []

        def key(self, chord):
            if chord == "Escape":
                self.visible = False
            elif chord == "ctrl+shift+y":
                self.visible = True
                self.shows += 1
            elif chord == "ctrl+a":
                self.selected = True

        def focus_editor(self):
            assert not self.visible

        def text(self, text):
            self.value = text if self.selected else self.value + text
            self.selected = False
            self.typed.append(text)

        def snapshot(self):
            query = self.value
            count = "1 result" if len(query) == 11 else "10 results"
            title = "Ibis smoke reference 00000" + ("x" if self.stale else query[-1:])
            return {"visible": self.visible, "nodes": [
                {"name": "Search references", "role": "combo box", "value": query, "focused": self.visible},
                {"name": "100,000 references", "role": "label"},
                {"name": count, "role": "status"}, {"name": title, "role": "list item"}]}

    def test_native_samples_require_real_show_and_unique_completed_query(self):
        native = self.Native()
        def observation(action, ready, timeout=20):
            action()
            self.assertTrue(ready())
            return 1.25
        with patch.object(perf, "observe", side_effect=observation):
            summon, typing = perf.interaction_samples(native)
        self.assertEqual(summon, [1.25] * 20)
        self.assertEqual(typing, [1.25] * 20)
        self.assertEqual(native.shows, 21)
        self.assertEqual(len([text for text in native.typed if len(text) == 1]), 21)
        with patch.object(perf, "observe", side_effect=observation), self.assertRaises(AssertionError):
            perf.interaction_samples(self.Native(stale=True))

    def test_native_observation_waits_for_evidence_and_never_subtracts_overhead(self):
        states = iter((False, False, True))
        ticks = iter((0, .001, .020, .040, .045))
        with patch.object(perf.time, "monotonic", side_effect=lambda: next(ticks)), patch.object(perf.time, "sleep") as sleep:
            elapsed = perf.observe(lambda: None, lambda: next(states))
        self.assertEqual(elapsed, 45)
        self.assertEqual(sleep.call_count, 2)

    def test_native_observation_has_a_deadline(self):
        with patch.object(perf.time, "monotonic", side_effect=(0, 21)), self.assertRaises(AssertionError):
            perf.observe(lambda: None, lambda: False)


if __name__ == "__main__":
    unittest.main()
