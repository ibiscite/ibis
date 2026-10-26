#!/usr/bin/env python3
"""Diagnostic Linux release-build observations; use only a disposable X11 session."""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import signal
import subprocess
import statistics
import sys
import uuid
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
            "medians_ms": {"summon": statistics.median(summon), "typing": statistics.median(typing)},
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


def require_lossless_trace(text):
    fields = dict(re.findall(r"^(overrun|commit overrun|dropped events):\s*(\d+)\s*$", text, re.MULTILINE))
    if set(fields) != {"overrun", "commit overrun", "dropped events"} or any(int(value) for value in fields.values()):
        raise smoke.Unavailable("Kernel trace events lost or loss statistics unavailable")


def count_wakeup_events(lines, root_pid):
    # Trace all four scheduler events in an isolated instance. PID event filters
    # also include events emitted BY a listed task, so they cannot identify only
    # wakeups OF that task. Instead follow fork/exit lineage and test the target.
    owned, root_births = set(), 0
    counts = {"wakeups": 0, "new_task_wakeups": 0}
    for line in lines:
        if line.startswith("#") or not line.strip():
            continue
        event = re.search(r": (sched_process_fork|sched_process_exit|sched_wakeup_new|sched_wakeup): (.*)$", line)
        if not event:
            raise smoke.Unavailable("Unexpected kernel trace event")
        kind, payload = event.groups()
        if kind == "sched_process_fork":
            match = re.fullmatch(r"comm=.* pid=(\d+) child_comm=.* child_pid=(\d+)", payload)
            if not match:
                raise smoke.Unavailable("Unsupported kernel fork event")
            parent, child = map(int, match.groups())
            if child == root_pid:
                root_births += 1
                owned.add(child)
            elif parent in owned:
                owned.add(child)
        else:
            pattern = (r"comm=.* pid=(\d+) prio=-?\d+(?: group_dead=(?:true|false))?" if kind == "sched_process_exit"
                       else r"comm=.* pid=(\d+) prio=-?\d+(?: success=\d+)? target_cpu=\d+")
            match = re.fullmatch(pattern, payload)
            if not match:
                raise smoke.Unavailable("Unsupported kernel scheduler event")
            target = int(match[1])
            if kind == "sched_process_exit":
                owned.discard(target)
            elif target in owned:
                counts["new_task_wakeups" if kind == "sched_wakeup_new" else "wakeups"] += 1
    if root_births != 1:
        raise smoke.Unavailable("App birth missing or PID recycled in kernel trace")
    return counts


TRACE_ROOT = Path("/sys/kernel/tracing/instances")


def trace_instance(name):
    if not re.fullmatch(r"ibis-perf-[0-9a-f]{32}", name):
        raise ValueError("Invalid private trace instance")
    return TRACE_ROOT / name


def trace_start():
    name = "ibis-perf-" + uuid.uuid4().hex
    path = trace_instance(name)
    path.mkdir()
    try:
        (path / "tracing_on").write_text("0")
        (path / "current_tracer").write_text("nop")
        (path / "events/enable").write_text("0")
        (path / "trace_clock").write_text("global")
        (path / "buffer_size_kb").write_text("4096")  # Per CPU; loss always invalidates the run.
        for event in ("sched_process_fork", "sched_process_exit"):
            (path / f"events/sched/{event}/enable").write_text("1")
        (path / "trace").write_text("")
        (path / "tracing_on").write_text("1")
        return {"instance": name}
    except Exception:
        trace_stop(name)
        raise


def trace_stop(name):
    path = trace_instance(name)
    if path.exists():
        (path / "tracing_on").write_text("0")
        (path / "events/enable").write_text("0")
        path.rmdir()


def trace_window(name, root_pid):
    path = trace_instance(name)
    before = time.monotonic()
    for event in ("sched_wakeup", "sched_wakeup_new"):
        (path / f"events/sched/{event}/enable").write_text("1")
    time.sleep(30)  # One fixed window, never retried or selected for a quiet result.
    (path / "tracing_on").write_text("0")
    seconds = time.monotonic() - before
    stats = list((path / "per_cpu").glob("cpu*/stats"))
    if not stats:
        raise smoke.Unavailable("Kernel trace loss statistics unavailable")
    for stat in stats:
        require_lossless_trace(stat.read_text())
    with (path / "trace").open(errors="replace") as source:
        text = source.read(64 * 1024 * 1024 + 1)
    if len(text) > 64 * 1024 * 1024:
        raise smoke.Unavailable("Kernel trace exceeded observation bound")
    # Raw trace text, including host task names, never leaves this root helper.
    return dict(count_wakeup_events(text.splitlines(), root_pid), seconds=seconds,
                method="tracefs sched_wakeup and sched_wakeup_new")


class TraceUnavailable(smoke.Unavailable):
    def __init__(self, facts):
        super().__init__("Kernel scheduler observation unavailable")
        self.facts = facts


def trace_command(mode, *args):
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise smoke.Unavailable("Kernel observer is limited to disposable CI")
    child = subprocess.run(["sudo", "-n", "--preserve-env=GITHUB_ACTIONS", "/usr/bin/python3", "-B", str(Path(__file__).resolve()),
                            "--trace-" + mode, *map(str, args)], stdout=subprocess.PIPE,
                           stderr=subprocess.DEVNULL, text=True, timeout=60)
    facts = json.loads(child.stdout)
    if child.returncode:
        raise TraceUnavailable(facts)
    return facts


def observe(action, ready, timeout=20, phases=None):
    started = time.monotonic()
    action()
    if phases is not None:
        phases.update(action_ms=(time.monotonic() - started) * 1000, probe_calls=0, probe_ms=0)
    while time.monotonic() - started < timeout:
        before = time.monotonic()
        complete = ready()
        finished = time.monotonic()
        if phases is not None:
            phases["probe_calls"] += 1
            phases["probe_ms"] += (finished - before) * 1000
        if complete:
            return round((finished - started) * 1000, 3)
        time.sleep(.005)
    raise AssertionError("Native observation timed out")


def result_reader(native):
    # Cache the Results subtree. No footer/announcement traversal in timed typing.
    def find():
        stack = [item for item in native.atspi.Registry.getDesktop(0) if item and item.get_process_id() == native.app.pid]
        count = 0
        while stack:
            item = stack.pop()
            count += 1
            if count > 4000:
                raise AssertionError("Native results lookup exceeded bound")
            if item.name == "Results":
                return item
            stack.extend(child for child in item if child)
        raise AssertionError("Native results unavailable")
    root = find()
    def titles():
        nonlocal root
        def read():
            values, stack, count = set(), [root], 0
            while stack:
                item = stack.pop()
                count += 1
                if count > 4000:
                    raise AssertionError("Native results traversal exceeded bound")
                text = item.name or ""
                try:
                    field = item.queryText()
                    text += " " + field.getText(0, min(field.characterCount, 500))
                except NotImplementedError:
                    pass
                values.update(re.findall(r"Ibis smoke reference [0-9]{6}", text))
                stack.extend(child for child in item if child)
            return values
        try:
            if root.getState().contains(native.atspi.STATE_DEFUNCT):
                root = find()
            return read()
        except native.accessibility_error:
            root = find()
            return read()
    return titles


def interaction_samples(native, samples=10):
    summon, typing, probes = [], [], {"summon": [], "typing": []}
    for index in range(samples + 1):
        native.key("Escape")
        observe(lambda: None, lambda: not native.snapshot()["visible"])
        native.focus_editor()
        phases = {}
        def mapped():
            window = native.window(native.app.pid)
            return window and window == smoke.run("xdotool", "getactivewindow")
        elapsed = observe(lambda: native.key("ctrl+shift+y"), mapped, phases=phases)
        def ready():
            state = native.snapshot()
            return state["visible"] and smoke.search_focused(state) and smoke.shows_count(state, "100,000 references")
        observe(lambda: None, ready)  # Verify search readiness outside the map timer.
        if index:
            summon.append(elapsed)
            probes["summon"].append(phases)
    titles = result_reader(native)
    prefix = "smoke00000"
    ten_titles = {f"Ibis smoke reference {index:06d}" for index in range(10)}
    for index in range(samples + 1):
        native.key("ctrl+a")
        native.text(prefix)
        def prefix_ready():
            return smoke.query_failure_facts(native.snapshot(), prefix, "")["exact_search_value"] and titles() == ten_titles
        observe(lambda: None, prefix_ready)
        time.sleep(.35)  # The 300 ms delayed speech announcement must settle first.
        digit = str(index % 10)
        expected = "Ibis smoke reference 00000" + digit
        phases = {}
        elapsed = observe(lambda: native.text(digit), lambda: titles() == {expected}, phases=phases)
        # Prove exact query and 1-result announcement, outside the measured interval.
        smoke.wait_for_query(native.snapshot, prefix + digit, expected, {}, settled=True)
        if index:
            typing.append(elapsed)
            probes["typing"].append(phases)
    return summon, typing, probes


def measure(app, sha):
    native, process, trace = None, None, None
    try:
        native = smoke.Linux()
        with tempfile.TemporaryDirectory(prefix="ibis-release-performance-") as temporary:
            folder = Path(temporary)
            library = folder / "synthetic.bib"
            library.write_text(smoke.fixture(100000), encoding="utf-8")
            env = smoke.real_app_environment(os.environ, library, folder / "config")
            smoke.answer_update_question(smoke.settings_path("Linux", folder))
            trace = trace_command("start")["instance"]
            process = subprocess.Popen([str(app.resolve())], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
            native.app = process
            def loaded():
                if process.poll() is not None:
                    raise AssertionError("Release executable exited")
                state = native.snapshot()
                return state["visible"] and smoke.search_focused(state) and smoke.shows_count(state, "100,000 references")
            observe(lambda: None, loaded, timeout=120)
            summon, typing, probes = interaction_samples(native)
            smoke.escape_to_hide(native, native.snapshot, "idle picker hidden")
            time.sleep(30)  # Fixed settling interval; worker retirement remains fully counted.
            memory_before = native.memory()
            wakeups = trace_command("window", trace, process.pid)
            memory_after = native.memory()
            idle = {"memory_bytes": max(memory_before, memory_after),
                    "wakeups_per_second": (wakeups["wakeups"] + wakeups["new_task_wakeups"]) / wakeups["seconds"],
                    **wakeups}
            if process.poll() is not None or idle["memory_bytes"] <= 0:
                raise AssertionError("Release process unavailable during idle measurement")
            return {"source_sha": sha, "summon_samples_ms": summon, "typing_samples_ms": typing, "idle": idle, "probes": probes}
    finally:
        try:
            if process:
                try:
                    os.killpg(process.pid, signal.SIGTERM)  # This synthetic app owns a new session.
                except ProcessLookupError:
                    pass
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=5)
            if native:
                native.close()
                # ABBA must not start a new window manager while the old one still owns X11.
                for helper in (native.editor, native.wm):
                    try:
                        helper.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        helper.kill()
                        helper.wait(timeout=5)
        finally:
            if trace:
                trace_command("stop", trace)


def paired_measurements(candidate_app, candidate_sha, main_app, main_sha, output):
    output.mkdir(parents=True, exist_ok=True)
    sources = {"candidate": (candidate_app, candidate_sha), "main": (main_app, main_sha)}
    runs = {key: [] for key in sources}
    for index, key in enumerate(("main", "candidate", "candidate", "main")):
        app, sha = sources[key]
        run = measure(app, sha)
        runs[key].append(run)
        (output / f"{index}-{key}.json").write_text(json.dumps(run, indent=2, allow_nan=False) + "\n")
    for key, (app, sha) in sources.items():
        idle_runs = [run["idle"] for run in runs[key]]
        seconds = sum(run["seconds"] for run in idle_runs)
        idle = {"memory_bytes": max(run["memory_bytes"] for run in idle_runs), "seconds": seconds,
                "wakeups_per_second": sum(run["wakeups_per_second"] * run["seconds"] for run in idle_runs) / seconds,
                "runs": idle_runs}
        result = make_report(sha, [value for run in runs[key] for value in run["summon_samples_ms"]],
                             [value for run in runs[key] for value in run["typing_samples_ms"]], idle)
        result["executable_sha256"] = hashlib.sha256(app.read_bytes()).hexdigest()
        (output / f"{key}.json").write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")



def failure_facts(error):
    # Only public code locations and numeric observer facts; no exception text,
    # stack locals, native tree, filenames or user paths.
    frames, traceback = [], error.__traceback__
    while traceback:
        if traceback.tb_frame.f_code.co_filename == __file__:
            frames.append({"function": traceback.tb_frame.f_code.co_name, "line": traceback.tb_lineno})
        traceback = traceback.tb_next
    facts = {"schema": 1, "status": "unavailable", "error_class": type(error).__name__, "code_frames": frames}
    if isinstance(error, TraceUnavailable):
        facts["kernel_observer"] = error.facts
    return facts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path)
    parser.add_argument("--sha")
    parser.add_argument("--baseline-app", type=Path)
    parser.add_argument("--main-sha")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    try:
        if not all((args.app, args.sha, args.baseline_app, args.main_sha)):
            parser.error("measurement needs both executables and exact source SHAs")
        paired_measurements(args.app, args.sha, args.baseline_app, args.main_sha, args.output)
        candidate, baseline = [json.loads((args.output / f"{key}.json").read_text()) for key in ("candidate", "main")]
        rows = compare(candidate, baseline)
        result = {"schema": 1, "candidate_sha": candidate["source_sha"], "main_sha": baseline["source_sha"], "comparison": rows}
        (args.output / "comparison.json").write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
        lines = [f"Candidate `{candidate['source_sha']}` vs main `{baseline['source_sha']}`\n",
                 "| Observation | Main | Candidate | Change |", "| --- | ---: | ---: | ---: |"]
        for key, row in rows.items():
            change = f"{row['change_percent']:+.1f}%" if row["change_percent"] is not None else "n/a (zero main)"
            lines.append(f"| {key} | {row['main']:.2f} | {row['candidate']:.2f} | {change} |")
        lines.append("\nDiagnostic only. Timings include keyboard delivery, native reads and 5 ms polling; "
                     "probe/action overhead, medians and raw samples are in JSON. Summon ends when the X window maps and activates; "
                     "search focus and library count are checked afterwards. Typing ends on the exact singleton result title, "
                     "with the delayed spoken result count checked afterwards. One discarded warm-up per interaction per launch; "
                     "ABBA launch order (main, candidate, candidate, main), ten observations per interaction per launch. "
                     "Idle is hidden after 30 seconds settling, then a fixed 30-second window without UI polling. "
                     "Memory sums process-tree RSS (shared pages may be counted twice). Wakeups count kernel sched_wakeup and sched_wakeup_new events targeting app descendants, "
                     "including short-lived workers and the accessibility bridge; lost events invalidate the run. "
                     "OS caches persist. "
                     "No internal app latency budget is certified by these observations.")
        (args.output / "comparison.md").write_text("\n".join(lines) + "\n")
        return 0
    except Exception as error:
        # Native/library errors can contain private strings; retain only a fixed class.
        (args.output / "unavailable.json").write_text(json.dumps(failure_facts(error)) + "\n")
        print("::warning::Release performance observation unavailable; see diagnostic JSON.")
        return 1


if __name__ == "__main__":
    if sys.argv[1:2] in (["--trace-start"], ["--trace-window"], ["--trace-stop"]):
        try:
            if os.environ.get("GITHUB_ACTIONS") != "true" or not hasattr(os, "geteuid") or os.geteuid() != 0:
                raise smoke.Unavailable("Kernel observer needs a disposable hosted Linux runner")
            mode = sys.argv[1]
            if mode == "--trace-start" and len(sys.argv) == 2:
                result = trace_start()
            elif mode == "--trace-stop" and len(sys.argv) == 3:
                trace_stop(sys.argv[2])
                result = {"stopped": True}
            elif mode == "--trace-window" and len(sys.argv) == 4 and sys.argv[3].isdigit() and int(sys.argv[3]) > 0:
                result = trace_window(sys.argv[2], int(sys.argv[3]))
            else:
                raise ValueError("Invalid kernel observer arguments")
            print(json.dumps(result, allow_nan=False))
        except Exception as error:
            print(json.dumps(failure_facts(error)))
            raise SystemExit(1)
    else:
        raise SystemExit(main())
