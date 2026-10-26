"""Synthetic contracts for the diagnostic release-build comparison."""
import importlib.util
import json
import tempfile
from pathlib import Path
import unittest
from unittest.mock import Mock, patch

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
        self.assertEqual(perf.wakeup_delta(before, after | {(12, 102): 3}), 8)
        for invalid in ({(10, 100): 23}, {(10, 100): 23, (11, 102): 7},
                        {(10, 100): 19, (11, 101): 7}):
            with self.subTest(invalid=invalid), self.assertRaises(perf.smoke.Unavailable):
                perf.wakeup_delta(before, invalid)

    def test_wakeups_are_scheduler_wakeups_not_context_switches(self):
        self.assertEqual(perf.parse_wakeups("synthetic (123, #threads: 1)\nnr_switches : 900\nnr_wakeups : 12\n"), 12)
        for text in ("nr_wakeups_sync : 42\n", "nr_switches : 900\n", "nr_wakeups : -1\n", "nr_wakeups : unknown\n"):
            with self.subTest(text=text), self.assertRaises(perf.smoke.Unavailable):
                perf.parse_wakeups(text)


    class Native:
        def __init__(self, stale=False):
            self.value, self.visible, self.selected = "", True, False
            self.stale = stale
            self.shows = 0
            self.typed = []
            self.app = type("App", (), {"pid": 123})()

        def key(self, chord):
            if chord == "Escape":
                self.visible = False
            elif chord == "ctrl+shift+y":
                self.visible = True
                self.shows += 1
            elif chord == "ctrl+a":
                self.selected = True

        def window(self, pid):
            return "42" if self.visible else None

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
        def observation(action, ready, timeout=20, phases=None):
            action()
            self.assertTrue(ready())
            return 1.25
        def titles(model):
            def read():
                if len(model.value) == 10:
                    return {f"Ibis smoke reference {i:06d}" for i in range(10)}
                return {"Ibis smoke reference 00000" + ("x" if model.stale else model.value[-1:])}
            return read
        with patch.object(perf, "observe", side_effect=observation), patch.object(perf, "result_reader", side_effect=titles), \
             patch.object(perf.smoke, "run", return_value="42"), patch.object(perf.smoke, "wait_for_query"), patch.object(perf.time, "sleep"):
            summon, typing, probes = perf.interaction_samples(native)
        self.assertEqual(summon, [1.25] * 10)
        self.assertEqual(typing, [1.25] * 10)
        self.assertEqual(native.shows, 11)
        self.assertEqual(len(probes["typing"]), 10)
        self.assertEqual(len([text for text in native.typed if len(text) == 1]), 11)
        with patch.object(perf, "observe", side_effect=observation), patch.object(perf, "result_reader", side_effect=titles), \
             patch.object(perf.smoke, "run", return_value="42"), patch.object(perf.smoke, "wait_for_query"), \
             patch.object(perf.time, "sleep"), self.assertRaises(AssertionError):
            perf.interaction_samples(self.Native(stale=True))

    def test_native_observation_waits_for_evidence_and_never_subtracts_overhead(self):
        states = iter((False, False, True))
        ticks = iter((0, .001, .002, .003, .020, .021, .022, .040, .041, .045))
        with patch.object(perf.time, "monotonic", side_effect=lambda: next(ticks)), patch.object(perf.time, "sleep") as sleep:
            elapsed = perf.observe(lambda: None, lambda: next(states))
        self.assertEqual(elapsed, 45)
        self.assertEqual(sleep.call_count, 2)

    def test_native_observation_has_a_deadline(self):
        with patch.object(perf.time, "monotonic", side_effect=(0, 21)), self.assertRaises(AssertionError):
            perf.observe(lambda: None, lambda: False)


    def test_pair_uses_abba_twenty_samples_and_source_executable_hashes(self):
        order = []
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            candidate, baseline = root / "candidate-app", root / "main-app"
            candidate.write_bytes(b"synthetic candidate executable")
            baseline.write_bytes(b"synthetic baseline executable")
            def measure(app, sha):
                order.append(app.name)
                value = self.report(sha, len(order))
                value["summon_samples_ms"] = value["summon_samples_ms"][:10]
                value["typing_samples_ms"] = value["typing_samples_ms"][:10]
                return value
            with patch.object(perf, "measure", side_effect=measure):
                perf.paired_measurements(candidate, "b" * 40, baseline, "a" * 40, root / "reports")
            self.assertEqual(order, ["main-app", "candidate-app", "candidate-app", "main-app"])
            reports = [json.loads((root / "reports" / f"{key}.json").read_text()) for key in ("candidate", "main")]
            self.assertNotEqual(reports[0]["executable_sha256"], reports[1]["executable_sha256"])
            self.assertTrue(all(len(report["summon_samples_ms"]) == 20 for report in reports))


    def test_launches_are_isolated_and_helpers_exit_before_next_abba_run(self):
        native = Mock()
        native.snapshot.return_value = {"visible": True, "nodes": [
            {"name": "Search references", "focused": True}, {"name": "100,000 references"}]}
        native.memory.return_value = 1024
        process = Mock(pid=123)
        process.poll.return_value = None
        before, after = {(123, 123, 99): 1}, {(123, 123, 99): 3}
        ticks = iter(range(20))
        with patch.object(perf.smoke, "Linux", return_value=native), \
             patch.object(perf.subprocess, "Popen", return_value=process) as launch, \
             patch.object(perf, "interaction_samples", return_value=([1] * 10, [2] * 10, {})), \
             patch.object(perf.smoke, "escape_to_hide"), patch.object(perf.time, "sleep"), \
             patch.object(perf.time, "monotonic", side_effect=lambda: next(ticks)), \
             patch.object(perf.os, "killpg", create=True) as kill, patch.object(perf, "wakeup_census", side_effect=(before, after)):
            result = perf.measure(Path("synthetic-executable"), "a" * 40)
        self.assertTrue(launch.call_args.kwargs["start_new_session"])
        kill.assert_called_once_with(123, perf.signal.SIGTERM)
        native.editor.wait.assert_called_once_with(timeout=5)
        native.wm.wait.assert_called_once_with(timeout=5)
        self.assertEqual(len(result["summon_samples_ms"]), 10)
        self.assertEqual(result["idle"]["seconds"], 1)
        self.assertEqual(result["idle"]["wakeups_per_second"], 2)


    def test_failure_evidence_has_public_code_locations_and_numeric_census_only(self):
        try:
            perf.wakeup_delta({(1, 1, 1): 2}, {})
        except perf.CensusChanged as error:
            facts = perf.failure_facts(error)
        self.assertEqual(facts["census"]["lost_threads"], 1)
        self.assertTrue(any(frame["function"] == "wakeup_delta" for frame in facts["code_frames"]))
        private = "SYNTHETIC_PRIVATE_EXCEPTION_TEXT"
        self.assertNotIn(private, json.dumps(perf.failure_facts(RuntimeError(private))))
        self.assertNotIn("Idle process", json.dumps(facts))


if __name__ == "__main__":
    unittest.main()
