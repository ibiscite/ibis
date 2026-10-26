#!/usr/bin/env python3
"""Diagnostic Linux release-build observations; use only a disposable X11 session."""
import argparse
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

spec = importlib.util.spec_from_file_location("smoke_native", Path(__file__).with_name("smoke-native.py"))
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)

METRICS = ("summon_observed_p95_ms", "typing_observed_p95_ms", "idle_memory_bytes", "idle_wakeups_per_second")
SAMPLES = 20


def p95(values):
    if len(values) != SAMPLES or any(type(v) not in (int, float) or not math.isfinite(v) or v < 0 for v in values):
        raise ValueError("Expected twenty finite nonnegative observations")
    return sorted(values)[math.ceil(.95 * len(values)) - 1]


def make_report(sha, summon, typing, idle):
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError("Expected exact source SHA")
    return {"schema": 1, "status": "measured", "source_sha": sha, "platform": "Linux/X11",
            "build_profile": "release", "references": 100000,
            "summon_samples_ms": summon, "typing_samples_ms": typing, "idle": idle,
            "metrics": dict(zip(METRICS, (p95(summon), p95(typing), idle["memory_bytes"], idle["wakeups_per_second"])))}


def compare(candidate, baseline):
    for report in (candidate, baseline):
        if any(report.get(k) != v for k, v in {"schema": 1, "status": "measured", "platform": "Linux/X11",
                                               "build_profile": "release", "references": 100000}.items()):
            raise ValueError("Incomparable measurement")
        if (set(report["metrics"]) != set(METRICS) or
                any(type(v) not in (int, float) or not math.isfinite(v) or v < 0 for v in report["metrics"].values())):
            raise ValueError("Incomplete or invalid measurement")
    return {key: {"candidate": candidate["metrics"][key], "main": baseline["metrics"][key],
                  "delta": candidate["metrics"][key] - baseline["metrics"][key],
                  "change_percent": (100 * (candidate["metrics"][key] / baseline["metrics"][key] - 1)
                                     if baseline["metrics"][key] else None)} for key in METRICS}


def parse_wakeups(text):
    # Linux kernel/sched/debug.c exposes nr_wakeups only with schedstats enabled.
    match = re.search(r"^nr_wakeups\s*:\s*(\d+)\s*$", text, re.MULTILINE)
    if not match:
        raise smoke.Unavailable("Scheduler wakeup counters unavailable")
    return int(match[1])


def wakeup_delta(before, after):
    # Never silently omit an exited thread or mistake a recycled TID for it.
    if not before or before.keys() != after.keys() or any(after[key] < value for key, value in before.items()):
        raise smoke.Unavailable("Idle process/thread census changed; rerun measurement")
    return sum(after[key] - value for key, value in before.items())


def wakeup_census(pid):
    rows = [tuple(map(int, line.split())) for line in smoke.run("ps", "-eo", "pid=,ppid=").splitlines()]
    owned = {pid}
    while True:
        found = owned | {child for child, parent in rows if parent in owned}
        if found == owned:
            break
        owned = found
    counters = {}
    for process in owned:
        for task in (Path("/proc") / str(process) / "task").iterdir():
            # /proc stat's comm may contain spaces and parentheses; starttime is field 22.
            fields = (task / "stat").read_text().rsplit(")", 1)[1].split()
            identity = (int(task.name), int(fields[19]))
            counters[identity] = parse_wakeups((task / "sched").read_text())
    return counters


def observe(action, ready, timeout=20):
    started = time.monotonic()
    action()
    while time.monotonic() - started < timeout:
        if ready():
            return round((time.monotonic() - started) * 1000, 3)
        time.sleep(.005)
    raise AssertionError("Native observation timed out")


def interaction_samples(native):
    def ready():
        state = native.snapshot()
        return state["visible"] and smoke.search_focused(state) and smoke.shows_count(state, "100,000 references")
    summon, typing = [], []
    for index in range(SAMPLES + 1):
        native.key("Escape")
        observe(lambda: None, lambda: not native.snapshot()["visible"])
        native.focus_editor()
        elapsed = observe(lambda: native.key("ctrl+shift+y"), ready)
        if index:  # Discard one warm-up for each interaction.
            summon.append(elapsed)
    prefix = "smoke00000"
    for index in range(SAMPLES + 1):
        native.key("ctrl+a")
        native.text(prefix)
        def prefix_ready():
            state = native.snapshot()
            return (smoke.query_failure_facts(state, prefix, "")["exact_search_value"]
                    and "10 results" in smoke.strings(state).splitlines())
        observe(lambda: None, prefix_ready)
        digit = str(index % 10)
        def result_ready():
            facts = smoke.query_failure_facts(native.snapshot(), prefix + digit, "Ibis smoke reference 00000" + digit)
            return facts["exact_search_value"] and facts["expected_result"] and facts["one_result"]
        elapsed = observe(lambda: native.text(digit), result_ready)
        if index:
            typing.append(elapsed)
    return summon, typing


def measure(app, sha):
    native, process = None, None
    try:
        native = smoke.Linux()
        with tempfile.TemporaryDirectory(prefix="ibis-release-performance-") as temporary:
            folder = Path(temporary)
            library = folder / "synthetic.bib"
            library.write_text(smoke.fixture(100000), encoding="utf-8")
            env = smoke.real_app_environment(os.environ, library, folder / "config")
            smoke.answer_update_question(smoke.settings_path("Linux", folder))
            process = subprocess.Popen([str(app.resolve())], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            native.app = process
            def loaded():
                if process.poll() is not None:
                    raise AssertionError("Release executable exited")
                state = native.snapshot()
                return state["visible"] and smoke.search_focused(state) and smoke.shows_count(state, "100,000 references")
            observe(lambda: None, loaded, timeout=120)
            summon, typing = interaction_samples(native)
            smoke.escape_to_hide(native, native.snapshot, "idle picker hidden")
            time.sleep(10)  # Let input/accessibility traffic and transient workers settle.
            before = wakeup_census(process.pid)
            started = time.monotonic()
            memory_before = native.memory()
            time.sleep(10)  # No input or accessibility polling during the idle window.
            memory_after = native.memory()
            after = wakeup_census(process.pid)
            seconds = time.monotonic() - started
            idle = {"memory_bytes": max(memory_before, memory_after),
                    "wakeups_per_second": wakeup_delta(before, after) / seconds,
                    "seconds": seconds, "threads": len(before)}
            if process.poll() is not None or idle["memory_bytes"] <= 0:
                raise AssertionError("Release process unavailable during idle measurement")
            return make_report(sha, summon, typing, idle)
    finally:
        if process and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        if native:
            native.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path)
    parser.add_argument("--sha")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--compare", nargs=2, type=Path, metavar=("CANDIDATE", "MAIN"))
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    try:
        if args.compare:
            candidate, baseline = [json.loads(path.read_text()) for path in args.compare]
            rows = compare(candidate, baseline)
            result = {"schema": 1, "candidate_sha": candidate["source_sha"], "main_sha": baseline["source_sha"], "comparison": rows}
            print(f"Candidate `{candidate['source_sha']}` vs main `{baseline['source_sha']}`\n")
            print("| Observation | Main | Candidate | Change |\n| --- | ---: | ---: | ---: |")
            for key, row in rows.items():
                change = f"{row['change_percent']:+.1f}%" if row["change_percent"] is not None else "n/a (zero main)"
                print(f"| {key} | {row['main']:.2f} | {row['candidate']:.2f} | {change} |")
            print("\nDiagnostic only. Timings include keyboard delivery, native accessibility reads and 5 ms polling; "
                  "they are observed upper bounds, not internal app timings. One discarded warm-up and 20 samples per interaction. "
                  "Typing measures a single digit completing a unique query in 100,000 references. "
                  "Idle is hidden after 10 seconds settling, then 10 seconds without UI polling. "
                  "Memory sums process-tree RSS (shared pages may be counted twice). Wakeups sum Linux nr_wakeups "
                  "for process-tree threads present at both endpoints; census changes invalidate the run. "
                  "Threads born and exited wholly within the interval are not observed. OS caches persist; baseline runs first. "
                  "No fixed app latency budget is certified by these observations.")
        else:
            if not args.app or not args.sha:
                parser.error("measurement needs --app and --sha")
            result = measure(args.app, args.sha)
        args.output.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
        return 0
    except Exception as error:
        # Native/library errors can contain private strings; retain only a fixed class.
        args.output.write_text(json.dumps({"schema": 1, "status": "unavailable", "error_class": type(error).__name__}) + "\n")
        print("::warning::Release performance observation unavailable; see diagnostic JSON.")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
