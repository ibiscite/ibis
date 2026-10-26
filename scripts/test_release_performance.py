"""Synthetic contracts for the diagnostic release-build comparison."""
import importlib.util
import json
import os
import re
import subprocess
import sys
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
        ticks = iter(range(20))
        kernel = {"wakeups": 2, "new_task_wakeups": 1, "seconds": 30, "method": "synthetic"}
        with patch.object(perf.smoke, "Linux", return_value=native), \
             patch.object(perf.subprocess, "Popen", return_value=process) as launch, \
             patch.object(perf, "interaction_samples", return_value=([1] * 10, [2] * 10, {})), \
             patch.object(perf.smoke, "escape_to_hide"), patch.object(perf.time, "sleep"), \
             patch.object(perf.time, "monotonic", side_effect=lambda: next(ticks)), \
             patch.object(perf.os, "killpg", create=True) as kill, \
             patch.object(perf, "trace_command", side_effect=({"instance": "synthetic"}, kernel, {"stopped": True})) as trace:
            result = perf.measure(Path("synthetic-executable"), "a" * 40)
        self.assertTrue(launch.call_args.kwargs["start_new_session"])
        kill.assert_called_once_with(123, perf.signal.SIGTERM)
        native.editor.wait.assert_called_once_with(timeout=5)
        native.wm.wait.assert_called_once_with(timeout=5)
        self.assertEqual(len(result["summon_samples_ms"]), 10)
        self.assertEqual(result["idle"]["seconds"], 30)
        self.assertEqual(result["idle"]["wakeups_per_second"], .1)
        self.assertEqual([call.args[0] for call in trace.call_args_list], ["start", "window", "stop"])


    def test_failure_evidence_has_public_code_locations_without_private_text(self):
        try:
            perf.require_lossless_trace("overrun: 1")
        except perf.smoke.Unavailable as error:
            facts = perf.failure_facts(error)
        self.assertTrue(any(frame["function"] == "require_lossless_trace" for frame in facts["code_frames"]))
        private = "SYNTHETIC_PRIVATE_EXCEPTION_TEXT"
        self.assertNotIn(private, json.dumps(perf.failure_facts(RuntimeError(private))))

    def test_kernel_events_count_short_lived_descendants_and_exclude_outside_targets(self):
        lines = [
            "p-1 [000] 1.0: sched_process_fork: comm=p pid=1 child_comm=app child_pid=10",
            "app-10 [000] 1.1: sched_process_fork: comm=app pid=10 child_comm=worker child_pid=11",
            "other-99 [001] 2.0: sched_wakeup: comm=worker pid=11 prio=120 target_cpu=000",
            "app-10 [000] 2.1: sched_wakeup: comm=other pid=99 prio=120 target_cpu=000",
            "app-10 [000] 2.2: sched_process_fork: comm=app pid=10 child_comm=brief child_pid=12",
            "app-10 [000] 2.3: sched_wakeup_new: comm=brief pid=12 prio=120 target_cpu=000",
            "brief-12 [000] 2.4: sched_wakeup: comm=brief pid=12 prio=120 target_cpu=000",
            "brief-12 [000] 2.5: sched_process_exit: comm=brief pid=12 prio=120 group_dead=false",
            "other-99 [000] 2.6: sched_process_fork: comm=other pid=99 child_comm=reused child_pid=12",
            "app-10 [000] 2.7: sched_wakeup: comm=reused pid=12 prio=120 target_cpu=000",
        ]
        self.assertEqual(perf.count_wakeup_events(lines, 10), {"wakeups": 2, "new_task_wakeups": 1})
        with self.assertRaises(perf.smoke.Unavailable):
            perf.count_wakeup_events(lines, 123)

    def test_trace_overruns_or_missing_loss_statistics_never_become_zero_wakeups(self):
        perf.require_lossless_trace("entries: 10\noverrun: 0\ncommit overrun: 0\ndropped events: 0\n")
        for stats in ("overrun: 1\ncommit overrun: 0\ndropped events: 0\n", "overrun: 0\n"):
            with self.subTest(stats=stats), self.assertRaises(perf.smoke.Unavailable):
                perf.require_lossless_trace(stats)


    def test_trace_window_is_fixed_and_returns_only_counts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ("sched_wakeup", "sched_wakeup_new"):
                path = root / f"events/sched/{name}/enable"
                path.parent.mkdir(parents=True)
                path.write_text("0")
            (root / "tracing_on").write_text("1")
            stat = root / "per_cpu/cpu0/stats"
            stat.parent.mkdir(parents=True)
            stat.write_text("overrun: 0\ncommit overrun: 0\ndropped events: 0\n")
            (root / "trace").write_text(
                "p-1 [000] 1.0: sched_process_fork: comm=p pid=1 child_comm=app child_pid=10\n"
                "other-99 [000] 2.0: sched_wakeup: comm=app pid=10 prio=120 target_cpu=000\n")
            with patch.object(perf, "trace_instance", return_value=root), patch.object(perf.time, "sleep") as sleep, \
                 patch.object(perf.time, "monotonic", side_effect=(0, 30)):
                result = perf.trace_window("synthetic", 10)
            sleep.assert_called_once_with(30)
            self.assertEqual(result["wakeups"], 1)
            self.assertEqual(result["seconds"], 30)
            self.assertNotIn("child_comm", json.dumps(result))
            self.assertEqual((root / "tracing_on").read_text(), "0")

    def test_trace_instance_name_cannot_select_another_path(self):
        for name in ("../other", "/tmp/private", "ibis-perf-../other", "ibis-perf-x"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                perf.trace_instance(name)


    @unittest.skipIf(sys.platform == "win32", "Linux workflow shell")
    def test_tracefs_mount_uses_the_privileged_capability_probe(self):
        workflow = (Path(__file__).resolve().parents[1] / ".github/workflows/performance.yml").read_text()
        match = re.search(r"          if .*tracing/instances.*\n.*\n          fi", workflow)
        self.assertIsNotNone(match)
        block = match[0].replace("/sys/kernel/tracing/instances", "/synthetic-unsearchable-tracefs/instances")
        stub = '''sudo() {
  if [ "$1" = test ]; then [ "$ROOT_READY" = yes ];
  elif [ "$1" = mount ]; then echo mounted;
  else return 99; fi
}
'''
        for ready, expected in (("yes", ""), ("no", "mounted\n")):
            result = subprocess.run(["bash", "-ec", stub + block], text=True, capture_output=True,
                                    env=dict(os.environ, ROOT_READY=ready), timeout=5)
            with self.subTest(ready=ready):
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, expected)


if __name__ == "__main__":
    unittest.main()
