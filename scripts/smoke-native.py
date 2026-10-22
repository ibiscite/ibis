#!/usr/bin/env python3
"""Exercise the embedded native picker; never substitute a browser or IPC mock.

Exit 1 means a failed assertion; exit 2 means the session lacks a required native
capability. Only synthetic test text is recorded, never the accessibility tree or
clipboard contents. See docs/testing.md before running on a personal desktop.
"""
import argparse
from contextlib import redirect_stderr, redirect_stdout
import io
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, call, patch


class Unavailable(RuntimeError):
    pass


# The short Copied notice is a second visible window of the app (#496); it is
# never the picker, so picker visibility checks skip it.
NOTICE_TITLE = "Ibis notice"


def run(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.PIPE, timeout=20).strip()


def windows_creation_identity(app):
    # Query the original Popen handle, never a new handle to a possibly reused PID.
    import ctypes
    from ctypes import wintypes
    get_times = ctypes.WinDLL("kernel32", use_last_error=True).GetProcessTimes
    get_times.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(wintypes.FILETIME)] * 4
    get_times.restype = wintypes.BOOL
    times = [wintypes.FILETIME() for _ in range(4)]
    if app.poll() is not None or not get_times(int(app._handle), *(ctypes.byref(value) for value in times)):
        raise RuntimeError("Native app creation identity unavailable")
    ticks = (times[0].dwHighDateTime << 32) | times[0].dwLowDateTime
    if ticks <= 0:
        raise RuntimeError("Native app creation identity unavailable")
    # FILETIME starts in 1601; DateTime starts in year 1. CIM keeps microseconds.
    return (ticks + 504911232000000000) // 10


def memory_facts(value):
    failure = "Native process-tree memory facts unavailable"
    if (not isinstance(value, dict) or set(value) != {"memory_bytes", "components", "excluded_older"}
            or type(value["excluded_older"]) is not int or not 0 <= value["excluded_older"] <= 65536
            or not isinstance(value["components"], list) or not 1 <= len(value["components"]) <= 64):
        raise RuntimeError(failure)
    for index, row in enumerate(value["components"]):
        if (not isinstance(row, dict) or set(row) != {"index", "parent_index", "depth", "age_us", "memory_bytes"}
                or any(type(number) is not int or not 0 <= number < 2**63 for number in row.values())
                or row["index"] != index or row["memory_bytes"] == 0
                or (index == 0 and (row["parent_index"], row["depth"], row["age_us"]) != (0, 0, 0))
                or (index > 0 and (row["parent_index"] >= index or row["depth"] != value["components"][row["parent_index"]]["depth"] + 1))):
            raise RuntimeError(failure)
    if type(value["memory_bytes"]) is not int or value["memory_bytes"] != sum(row["memory_bytes"] for row in value["components"]):
        raise RuntimeError(failure)
    return value


def fixture(count=3):
    return "\n".join(
        f"@article{{smoke{i:06d}, title={{Ibis smoke reference {i:06d}}}, "
        "author={Example, Alex}, year={2026}}" for i in range(count)
    )


def real_app_environment(inherited, library, config):
    env = dict(inherited, IBIS_LIBRARY=str(library), XDG_CONFIG_HOME=str(config),
               NO_AT_BRIDGE="0", GTK_MODULES="gail:atk-bridge")
    # Gallery fixtures cannot supply evidence about actual native behavior.
    env.pop("IBIS_GALLERY", None)
    env.pop("IBIS_GALLERY_STATE", None)
    return env


def strings(snapshot):
    return "\n".join(str(n.get(k, "")) for n in snapshot["nodes"] for k in ("name", "value"))


def shows_count(snapshot, count):
    """Whether the footer's count, such as "3 references", reaches the accessibility tree.

    The footer splits the library name and the count into separate elements.
    WebKitGTK exposes only text in elements it keeps (the name's <bdi>, the
    count's <bdi>, the spoken detail's <p>); a plain <span> beside them loses its text.
    """
    return count in strings(snapshot)


def missing_library_evidence(snapshot, name):
    text = strings(snapshot)
    # Bidi isolates can expose the separator and state as separate native
    # text nodes. Their boundaries are whitespace, not missing status words.
    summary_text = " ".join(text.split())
    return {key: fragment if type(fragment) is bool else fragment in text for key, fragment in {
        "failed_source_named": f"The library could not be opened: {name}",
        "retention_explained": "the library file is unavailable; keeping the last loaded references",
        "recovery_action": "Choose it again or reconnect the drive.",
        # The footer says the library was not updated; its detail, read by
        # screen readers, keeps the retained count. Names are isolated (U+2068/9).
        "retained_snapshot": " · not updated" in summary_text and " · 4 references" in summary_text,
    }.items()}


def helper_timeout_stages(stderr):
    allowed = {f"NATIVE HELPER STAGE {stage}" for stage in (
        "windows-encoding-start", "windows-encoding-ready", "windows-uia-assembly-start",
        "windows-uia-assembly-ready", "windows-input-compile-start", "windows-input-compile-ready",
        "windows-dispatch",
    )}
    if isinstance(stderr, bytes):
        stderr = stderr.decode("utf-8", errors="replace")
    return [line for line in (stderr or "").splitlines() if line in allowed]


def editor_focus_facts(native, stage):
    state = native.call("editor-focus-state")
    keys = ("system_events_editor_frontmost", "workspace_editor_frontmost", "workspace_ibis_frontmost",
            "workspace_other_frontmost", "editor_running_application_active", "editor_window_visible")
    if not isinstance(state, dict) or any(type(state.get(key)) is not bool for key in keys):
        raise RuntimeError("Native editor focus metadata unavailable")
    return {"stage": stage, **{key: state[key] for key in keys}}


# Library actions live in the picker's More menu (Ctrl/Cmd+K), not the footer.
TREE_SETTLE_SECONDS = 10
MENU_ACTIONS = {"Choose library…", "Use Zotero", "Reload", "Quit Ibis"}


def menu_action(native, name):
    """Open More, then press the named menu item, as a user would."""
    if name not in MENU_ACTIONS:
        raise ValueError(f"{name!r} is not a More menu action")
    native.click("More")
    # The accessibility tree can lag the menu opening; wait for the item.
    def pressed():
        try:
            native.click(name)
            return True
        except (AssertionError, RuntimeError, subprocess.CalledProcessError):
            return False
    wait_for(pressed, f"{name} appears in the More menu", timeout=5)


def named(snapshot, name):
    return [n for n in snapshot["nodes"] if n.get("name") == name]


def search_focused(snapshot):
    return any(n.get("focused") for n in named(snapshot, "Search references"))


def menu_escape_focus(native, snapshot, shortcut, report):
    """Prove Escape/Tab destinations with real keys, not only accessibility focus."""
    def values(state):
        return [node["value"] for node in named(state, "Search references")
                if node.get("role", "").lower().replace(" ", "") in EDITABLE_ROLES
                and isinstance(node.get("value"), str)]
    initial = snapshot()
    assert initial["visible"] and initial["foreground"], "Native picker unavailable before menu check"
    original = values(initial)
    if not original or len(set(original)) != 1:
        raise RuntimeError("Native search value unavailable for menu focus check")
    facts = {key: False for key in ("shortcut_opened", "escape_closed", "escape_invoker_focused",
                                   "space_reopened", "search_unchanged", "tab_search_typed", "search_restored")}
    report["menu_escape_focus"] = facts
    native.key(shortcut)
    wait_for(lambda: bool(named(snapshot(), "Reload")), "shortcut opens More", timeout=5)
    facts["shortcut_opened"] = True
    native.key("Escape")
    wait_for(lambda: not named(snapshot(), "Reload"), "Escape closes More", timeout=5)
    facts["escape_closed"] = True
    def restored():
        current = snapshot()
        facts["escape_invoker_focused"] = any(node.get("focused") for node in named(current, "More"))
        facts["search_unchanged"] = values(current) == original
        return facts["escape_invoker_focused"]
    wait_for(restored, "Escape restores More focus", timeout=5)
    native.key("space")
    def reopened():
        current = snapshot()
        facts["space_reopened"] = bool(named(current, "Reload"))
        # A successful modal open hides Search from the accessibility tree.
        if not facts["space_reopened"]:
            facts["search_unchanged"] = facts["search_unchanged"] and values(current) == original
        return facts["space_reopened"]
    wait_for(reopened, "Space reopens More after Escape", timeout=5)
    native.key("Tab")
    wait_for(lambda: not named(snapshot(), "Reload"), "Tab closes More", timeout=5)

    def picker():
        current = snapshot()  # The caller also verifies the owned app is alive.
        assert current["visible"] and current["foreground"] and not named(current, "Reload"), "Native picker unavailable after Tab"
        return current

    def available():
        current = picker()
        assert values(current), "Native Search unavailable after Tab"
        return current

    def ready():
        current = picker()
        # AT-SPI/UIA can expose the combobox's active option as focused while
        # DOM keyboard focus remains on Search. This is only a settling cue;
        # actual typing/readback below must prove ownership, even for AX focus.
        return bool(values(current)) and (search_focused(current) or any(node.get("focused") and
            node.get("role", "").lower().replace(" ", "") in ("listitem", "controltype.listitem")
            for node in current["nodes"]))

    wait_for(ready, "Tab settles before native typing", timeout=5)
    facts["search_unchanged"] = facts["search_unchanged"] and values(available()) == original
    assert facts["search_unchanged"], "Menu changed the native query"
    select_all = "super+a" if shortcut == "super+k" else "ctrl+a"
    probe = "ibis tab probe" if original[0] != "ibis tab probe" else "ibis tab probe alternate"
    for text, fact in ((probe, "tab_search_typed"), (original[0], "search_restored")):
        available()
        native.key(select_all)
        available()
        native.text(text)
        wait_for(lambda: values(picker()) == [text] * len(original), "native Tab query readback", timeout=5)
        facts[fact] = True
    assert all(facts.values()), "Native menu keyboard ownership failed"


def control_activation_facts(state):
    keys = {"pressed", "button_role", "menu_item_role", "toggle_role",
            "invoke_pattern", "expand_collapse_pattern", "toggle_pattern"}
    if (not isinstance(state, dict) or type(state.get("pressed")) is not bool or
            any(type(state[key]) is not bool for key in keys & state.keys())):
        raise RuntimeError("Native button action metadata unavailable")
    return {key: state[key] for key in sorted(keys & state.keys())}


def has_search_text(snapshot):
    return any(n.get("value") for n in named(snapshot, "Search references"))


ESCAPE_STEP_TIMEOUT = 5


def escape_to_hide(native, snapshot, description):
    """Escape clears a search first and hides on the next press.

    macOS can report a cleared search value late, so with a search each press
    waits only briefly and a picker that is still visible gets one spare press.
    """
    last = {}
    if has_search_text(snapshot()):
        for press in range(3):
            if not snapshot()["visible"]:
                return
            native.key("Escape")
            deadline = time.monotonic() + ESCAPE_STEP_TIMEOUT
            while time.monotonic() < deadline:
                last.update(snapshot())
                if not last["visible"] or (press == 0 and not has_search_text(last)):
                    break
                time.sleep(0.1)
    else:
        native.key("Escape")
    def hidden():
        last.update(snapshot())
        return not last["visible"]
    try:
        wait_for(hidden, description)
    except AssertionError:
        print("ESCAPE STATE " + json.dumps({"visible": bool(last.get("visible")),
              "search_has_text": has_search_text(last) if last else None}), flush=True)
        raise


def prepare_initial_picker_show(native, snapshot):
    # A nonactivating panel can remain key while the editor is already the
    # frontmost application. Prove it is hidden before testing shortcut SHOW.
    if snapshot()["visible"]:
        def ready():
            current = snapshot()
            return current["visible"] and search_focused(current)
        wait_for(ready, "initial picker search ready")
        escape_to_hide(native, snapshot, "initial Escape hides picker")


EDITABLE_ROLES = {"edit", "text", "entry", "combobox", "axcombobox", "axtextfield", "axsearchfield", "controltype.edit", "controltype.combobox"}


def query_failure_facts(snapshot, query, expected):
    values = [n.get("value") for n in named(snapshot, "Search references")
              if n.get("role", "").lower().replace(" ", "") in EDITABLE_ROLES]
    text = strings(snapshot)
    return {"exact_search_value": query in values,
            "search_empty": bool(values) and all(value == "" for value in values),
            "expected_result": expected in text, "one_result": "1 result" in text.splitlines()}


TYPE_ATTEMPTS = 3
TYPE_SETTLE_TIMEOUT = 5


def type_query(native, snapshot, text, select_all):
    """Type into the search field until it reads back exactly, bounded."""
    for _attempt in range(TYPE_ATTEMPTS):
        native.key(select_all)
        native.text(text)
        deadline = time.monotonic() + TYPE_SETTLE_TIMEOUT
        while time.monotonic() < deadline:
            if query_failure_facts(snapshot(), text, "")["exact_search_value"]:
                return
            time.sleep(0.1)
    raise AssertionError(f"Search field did not hold the typed query after {TYPE_ATTEMPTS} attempts")


def wait_for_query(snapshot, query, expected, report, *, settled=False):
    last = {"nodes": []}
    def matched():
        nonlocal last
        last = snapshot()
        if settled:
            facts = query_failure_facts(last, query, expected)
            return facts["exact_search_value"] and facts["expected_result"] and facts["one_result"]
        return expected in strings(last)
    try:
        wait_for(matched, "native typed query")
    except AssertionError:
        # Keep the final observed snapshot; do not make another native read or
        # expose accessible query/result text just to diagnose a failure.
        report["query_failure"] = query_failure_facts(last, query, expected)
        print("NATIVE QUERY FAILURE " + json.dumps(report["query_failure"]), file=sys.stderr)
        raise


def fixed_toggle_diagnostics(paths):
    allowed = {
        "ibis-picker: toggle visible focused hide",
        "ibis-picker: toggle visible unfocused show",
        "ibis-picker: toggle visible focus-unavailable show",
        "ibis-picker: toggle not-known-visible show",
    }
    return [line for path in paths for line in path.read_text(encoding="utf-8", errors="replace").splitlines()
            if line in allowed]


PROBE_PHASE_KEYS = {"bootstrap_ms", "tree_ms", "properties_ms", "visibility_ms", "helper_ms", "elements"}
LIBRARY_PHASE_KEYS = {"load_ms", "load_finished_ms", "install_ms", "publish_ms", "completed_ms", "accepted", "load_ok"}


def numeric_phase_facts(value, keys):
    if (not isinstance(value, dict) or set(value) != keys or
            any(type(v) not in (int, float) or v < 0 or v > sys.float_info.max or not math.isfinite(v) for v in value.values())):
        return None
    if any(value[key] not in (0, 1) for key in ("accepted", "load_ok") if key in keys):
        return None
    if "elements" in keys and (type(value["elements"]) is not int or value["elements"] > 4000):
        return None
    return dict(value)


def library_phase_diagnostics(paths):
    facts = []
    for path in paths:
        try:
            with path.open(encoding="utf-8", errors="replace") as log:
                lines = log.read(1024 * 1024).splitlines()
        except OSError:
            continue
        for line in lines:
            if not line.startswith("ibis-library-phases: ") or len(line) > 2048:
                continue
            try:
                value = numeric_phase_facts(json.loads(line.removeprefix("ibis-library-phases: ")), LIBRARY_PHASE_KEYS)
            except (ValueError, TypeError, RecursionError):
                continue
            if value is not None:
                facts.append(value)
                if len(facts) == 64:
                    return facts
    return facts


def settings_path(system, temporary, native=None, identifier="io.github.ibiscite.ibis"):
    if system == "Windows":
        # dirs7 uses SHGetKnownFolderPath, not the APPDATA environment variable.
        roaming = Path(native.call("config-dir"))
        if not roaming.is_absolute():
            raise Unavailable("Windows native configuration directory unavailable")
        return roaming / identifier / "settings.json"
    if system == "Darwin":
        return Path.home() / "Library/Application Support" / identifier / "settings.json"
    return temporary / "config" / identifier / "settings.json"


def answer_update_question(settings):
    """Release builds ask about update checks at first launch; answer "Not Now"
    in advance so the question never takes focus from the picker."""
    if not settings.exists():
        settings.parent.mkdir(parents=True, exist_ok=True)
        settings.write_text(json.dumps({"schema_version": 2, "update_checks": False}), encoding="utf-8")


def claim_settings(candidate):
    if any(candidate.with_suffix(suffix).exists() or candidate.with_suffix(suffix).is_symlink()
           for suffix in (".json", ".json.bak", ".json.tmp")):
        raise Unavailable("Use a disposable desktop account without existing Ibis settings")
    return candidate


def clean_settings(owned):
    for suffix in (".json", ".json.bak", ".json.tmp"):
        owned.with_suffix(suffix).unlink(missing_ok=True)


CLIPBOARD_SENTINEL = "Ibis native smoke clipboard sentinel"


def prime_clipboard(native):
    native.set_clipboard(CLIPBOARD_SENTINEL)
    assert native.clipboard() == CLIPBOARD_SENTINEL, "Native clipboard sentinel could not be established"


def copy_picker_observation(native):
    """One existing native probe, reduced to fixed facts; never a readiness wait."""
    try:
        # Scripted.call can print helper errors before raising. Optional probes
        # must not disclose them or replace the actual copy-test failure.
        with open(os.devnull, "w") as quiet, redirect_stdout(quiet), redirect_stderr(quiet):
            current = native.snapshot()
            if type(current["visible"]) is not bool:
                return {"available": False}
            lines = strings(current).splitlines()
            return {"available": True, "visible": current["visible"],
                    "search_focused": search_focused(current),
                    "exact_search_value": query_failure_facts(current, "smoke000001", "")["exact_search_value"],
                    "title_present": "Ibis smoke reference 000001" in lines,
                    "one_result": "1 result" in lines,
                    "copy_error_present": "Nothing was copied. Try again, or use Reload." in lines}
    except Exception:
        return {"available": False}


def quiet_key_window(key_window):
    """One optional key-window probe for failure facts; never a readiness wait."""
    try:
        with open(os.devnull, "w") as quiet, redirect_stdout(quiet), redirect_stderr(quiet):
            state = key_window()
        return state if type(state) is bool else None
    except Exception:
        return None


def copy_with_diagnostics(native, report, key_window=None):
    """Press Return once in the picker and require the exact citation.

    On macOS the search field's accessible focus is only the WebView's DOM
    focus; it stays set when the panel is no longer the key window, and a
    Return posted then goes to another app (#454). Where the platform can
    observe it, wait for the picker to own the keyboard before pressing.
    """
    prime_clipboard(native)
    before = copy_picker_observation(native)
    if key_window is not None:
        try:
            wait_for(key_window, "picker is the key window before Return")
        except AssertionError:
            report["copy_failure"] = {"before_return": before, "key_window": False}
            print("NATIVE COPY FAILURE " + json.dumps(report["copy_failure"]), file=sys.stderr)
            raise
    native.key("Return")
    started = time.monotonic()
    clipboard = {"polls": 0, "expected": False, "sentinel": False, "empty": False}

    def copied():
        value = native.clipboard()
        clipboard.update(polls=clipboard["polls"] + 1, expected=value == "[@smoke000001]",
                         sentinel=value == CLIPBOARD_SENTINEL, empty=value == "")
        return clipboard["expected"]

    try:
        # Keep the existing equality and deadline ordering. A slow native read
        # can finish after the deadline; this is not a hard wall-clock bound.
        wait_for(copied, "expected citation on OS clipboard")
    except (AssertionError, OSError, subprocess.SubprocessError):
        clipboard["elapsed_seconds"] = round(time.monotonic() - started, 3)
        report["copy_failure"] = {"before_return": before, "clipboard": clipboard,
                                  "after_timeout": copy_picker_observation(native)}
        if key_window is not None:
            report["copy_failure"]["key_window_after_timeout"] = quiet_key_window(key_window)
        print("NATIVE COPY FAILURE " + json.dumps(report["copy_failure"]), file=sys.stderr)
        raise


def measure_startup(launch, snapshot, phases):
    """Keep the launch-to-count timer, exposing only numeric probe overhead.

    Readiness includes accessibility calls, predicate work and polling sleeps;
    it is not an independent measurement of the app's library loading time.
    Optional helper components overlap this interval and real backend work.
    Missing/invalid diagnostics never supply readiness or change acceptance.
    Backend clocks start at run_open, not process launch; absent debug lines
    (including an unfinished/stopped load) are recorded as an empty list.
    """
    phases.update(snapshot_calls=0, snapshot_ms=0, snapshot_max_ms=0)
    started = time.monotonic()
    try:
        launch()
    finally:
        launched = time.monotonic()
        phases["launch_ms"] = round((launched - started) * 1000, 2)
    def observed():
        before = time.monotonic()
        try:
            current = snapshot()
        finally:
            elapsed = (time.monotonic() - before) * 1000
            phases["snapshot_calls"] += 1
            phases["snapshot_ms"] += elapsed
            phases["snapshot_max_ms"] = max(phases["snapshot_max_ms"], elapsed)
        probe = numeric_phase_facts(current.get("probe_phases"), PROBE_PHASE_KEYS)
        if probe is not None:
            phases["probe_samples"] = phases.get("probe_samples", 0) + 1
            totals = phases.setdefault("probe_phases", dict.fromkeys(PROBE_PHASE_KEYS, 0))
            for key, value in probe.items():
                totals[key] += value
        return "100,000 references" in strings(current)
    try:
        wait_for(observed, "100,000-reference library load", timeout=120)
    finally:
        finished = time.monotonic()
        phases["readiness_ms"] = round((finished - launched) * 1000, 2)
        for key in ("snapshot_ms", "snapshot_max_ms"):
            phases[key] = round(phases[key], 2)
    return (finished - started) * 1000


PERF_METRICS = ("startup_ms", "first_result_ms", "memory_bytes")
TIMED_METRICS = ("startup_ms", "first_result_ms")
MEASURED_LAUNCHES = 3
# Any single launch, warm-up included, fails above this multiple of the budget (#464).
HARD_CEILING_FACTOR = 1.5


def summarize_performance(samples):
    measured = [sample for sample in samples if not sample.get("warmup")]
    if not measured:
        raise ValueError("No measured 100k launches")
    return {"status": "measured-without-budget", "references": 100000,
            "sample_count": len(measured), "warmup_count": len(samples) - len(measured), "samples": samples,
            "summary": "timings: median of fresh measured launches after one discarded warm-up; memory: maximum of all launches; "
                       "OS file caches persist",
            **{metric: statistics.median(sample[metric] for sample in measured) for metric in TIMED_METRICS},
            "memory_bytes": max(sample["memory_bytes"] for sample in samples),
            "maxima": {metric: max(sample[metric] for sample in samples) for metric in TIMED_METRICS},
            "memory_kind": "summed current process-tree RSS/working set; shared pages may be counted more than once"}


def enforce_performance_budgets(performance, budgets):
    ceilings = {metric: budgets[metric] for metric in PERF_METRICS}
    hard = {metric: ceilings[metric] * HARD_CEILING_FACTOR for metric in TIMED_METRICS}
    performance.update(status="failed", ceilings=ceilings, hard_ceilings=hard)
    for metric, ceiling in ceilings.items():
        if not 0 < performance[metric] <= ceiling:
            performance["failed_metric"] = metric
            kind = "maximum" if metric == "memory_bytes" else "median"
            raise AssertionError(f"100k {kind} {metric} exceeded measured baseline budget")
    for metric, ceiling in hard.items():
        if not performance["maxima"][metric] <= ceiling:
            performance["failed_metric"] = metric
            raise AssertionError(f"100k single-launch {metric} exceeded {HARD_CEILING_FACTOR}x budget hard ceiling")
    performance["status"] = "passed"


def require_tree(snapshot):
    for name in ("Search references", "Search results", "Results"):
        assert named(snapshot, name), f"Accessibility tree omits named {name!r}"
    search = named(snapshot, "Search references")
    assert any(n["role"].lower().replace(" ", "") in EDITABLE_ROLES for n in search), "Named search is not an editable control"
    results = named(snapshot, "Results")
    assert any("list" in n["role"].lower() and "item" not in n["role"].lower() for n in results), "Named Results is not a list"


def window_state_skips_taskbar(xprop_line):
    """Whether an `xprop _NET_WM_STATE` line asks to stay out of the taskbar and pager."""
    _, separator, atoms = xprop_line.partition("=")
    names = {atom.strip() for atom in atoms.split(",")} if separator else set()
    return {"_NET_WM_STATE_SKIP_TASKBAR", "_NET_WM_STATE_SKIP_PAGER"} <= names


def supporting_wm_window(xprop_line):
    """The window an `xprop _NET_SUPPORTING_WM_CHECK` line names, or None."""
    match = re.fullmatch(r"_NET_SUPPORTING_WM_CHECK\(WINDOW\): window id # (0x[0-9a-fA-F]+)", xprop_line.strip())
    return match.group(1) if match else None


CENSUS_PIDS, CENSUS_WINDOWS, CENSUS_NODES = 32, 64, 4000
CENSUS_MAP_STATES = ("IsUnMapped", "IsUnviewable", "IsViewable", "unknown")


def linux_census_facts(pid, wm_pid):
    if any(type(value) is not int or not 0 < value <= 0x7fffffff for value in (pid, wm_pid)):
        raise ValueError("Census requires owned process identities")
    booleans = ("app_alive wm_alive process_query_available process_query_succeeded process_tree_complete "
                "x_root_available x_root_succeeded window_query_available window_query_complete window_bound_hit "
                "map_queries_available atspi_attempted atspi_available atspi_desktop_succeeded atspi_complete "
                "atspi_bound_hit named_search expected_count").split()
    counts = ("window_queries_failed window_queries_empty ownership_mismatches window_properties_failed "
              "map_queries_attempted map_queries_failed exact_windows descendant_windows exact_hidden descendant_hidden "
              "atspi_apps atspi_exact_apps atspi_descendant_apps atspi_nodes atspi_queries_failed").split()
    return {**dict.fromkeys(booleans, False), **dict.fromkeys(counts, 0), "app_pid": pid, "wm_pid": wm_pid,
            "descendant_pids": [], "map_states": {scope: dict.fromkeys(CENSUS_MAP_STATES, 0) for scope in ("exact", "descendant")}}


def linux_census_frame(output, pid, wm_pid):
    """Accept only fixed numeric/Boolean fields, including after worker timeout."""
    template = linux_census_facts(pid, wm_pid)
    def valid(value, expected):
        if type(expected) is dict:
            return type(value) is dict and value.keys() == expected.keys() and all(valid(value[key], item) for key, item in expected.items())
        if type(expected) is list:
            return type(value) is list and len(value) <= CENSUS_PIDS and all(type(item) is int and 0 < item <= 0x7fffffff for item in value)
        return type(value) is type(expected) and (type(value) is bool or 0 <= value <= 0x7fffffff)
    if type(output) is bytes:
        output = output.decode("utf-8", errors="replace")
    if type(output) is not str or len(output) > 65536:
        return None
    last = None
    for line in output.splitlines()[-8:]:
        try:
            value = json.loads(line)
            if valid(value, template) and value["app_pid"] == pid and value["wm_pid"] == wm_pid:
                last = value
        except (ValueError, TypeError):
            pass
    return last


def linux_census(pid, wm_pid, alive, runner, bridge_loader, emit):
    """Read-only failure census: no titles, text, paths, XIDs or raw errors escape."""
    facts = linux_census_facts(pid, wm_pid)
    facts.update(app_alive=bool(alive(pid)), wm_alive=bool(alive(wm_pid)))
    def flush():
        frame = linux_census_frame(json.dumps(facts), pid, wm_pid)
        if frame is not None:
            emit(frame)
    def command(argv):
        try:
            result = runner(argv)
            if result is None:
                return False, False, False, ""
            valid = type(result.stdout) is str and len(result.stdout) <= 131072
            return True, result.returncode == 0 and valid, result.returncode == 1 and result.stdout == "" and result.stderr == "", result.stdout if valid else ""
        except (OSError, subprocess.SubprocessError):
            return True, False, False, ""
    available, succeeded, _, text = command(["ps", "-eo", "pid=,ppid="])
    facts.update(process_query_available=available, process_query_succeeded=succeeded)
    owned = {pid}
    if succeeded and all(re.fullmatch(r"\s*[0-9]{1,10}\s+[0-9]{1,10}\s*", line) for line in text.splitlines()):
        rows = [tuple(map(int, line.split())) for line in text.splitlines()]
        valid_rows = any(child == pid for child, _ in rows) and all(max(child, parent) <= 0x7fffffff for child, parent in rows)
        for _ in range(CENSUS_PIDS if valid_rows else 0):
            found = owned | {child for child, parent in rows if child > 0 and parent in owned}
            if len(found) > CENSUS_PIDS:
                break
            if found == owned:
                facts["process_tree_complete"] = True
                break
            owned = found
    facts["descendant_pids"] = sorted(owned - {pid})
    available, succeeded, _, _ = command(["xprop", "-root", "_NET_CLIENT_LIST"])
    facts.update(x_root_available=available, x_root_succeeded=succeeded, window_query_complete=True)
    windows = set()
    for owner in sorted(owned):
        available, succeeded, empty, text = command(["xdotool", "search", "--pid", str(owner)])
        facts["window_query_available"] |= available
        lines = text.splitlines()
        facts["window_bound_hit"] |= len(lines) > CENSUS_WINDOWS
        valid = len(lines) <= CENSUS_WINDOWS and all(re.fullmatch(r"[0-9]{1,10}", line) and 0 < int(line) <= 0xffffffff for line in lines)
        if not (succeeded or empty) or not valid:
            facts["window_queries_failed"] += 1
            facts["window_query_complete"] = False
            continue
        facts["window_queries_empty"] += not lines
        windows.update(map(int, lines))
        if len(windows) > CENSUS_WINDOWS:
            facts.update(window_bound_hit=True, window_query_complete=False)
            windows = set(sorted(windows)[:CENSUS_WINDOWS])
            break
    for window in sorted(windows):
        _, succeeded, _, text = command(["xprop", "-id", str(window), "_NET_WM_PID", "_NET_WM_STATE"])
        owner = re.search(r"^_NET_WM_PID\(CARDINAL\) = ([0-9]{1,10})$", text, re.M)
        if not succeeded or not owner:
            facts["window_properties_failed"] += 1
            continue
        if int(owner[1]) not in owned:
            facts["ownership_mismatches"] += 1
            continue
        scope = "exact" if int(owner[1]) == pid else "descendant"
        facts[f"{scope}_windows"] += 1
        state = re.search(r"^_NET_WM_STATE\(ATOM\) = ([A-Z0-9_, ]*)$", text, re.M)
        facts[f"{scope}_hidden"] += bool(state and "_NET_WM_STATE_HIDDEN" in state[1].split(", "))
        available, succeeded, _, text = command(["xwininfo", "-id", str(window), "-stats"])
        facts["map_queries_available"] |= available
        facts["map_queries_attempted"] += 1
        mapped = re.search(r"^\s*Map State: (IsUnMapped|IsUnviewable|IsViewable)\s*$", text, re.M)
        status = mapped[1] if succeeded and mapped else "unknown"
        facts["map_states"][scope][status] += 1
        facts["map_queries_failed"] += status == "unknown"
    flush()  # Preserve process/X facts even if importing or querying AT-SPI blocks.
    facts["atspi_attempted"] = True
    try:
        bridge = bridge_loader()
        facts["atspi_available"] = True
        flush()
        desktop = bridge.Registry.getDesktop(0)
        count = desktop.childCount
        if type(count) is not int or not 0 <= count <= 0x7fffffff:
            raise ValueError("Invalid census node count")
        facts.update(atspi_desktop_succeeded=True, atspi_apps=count, atspi_bound_hit=count > CENSUS_PIDS)
        stack = []
        for index in range(min(count, CENSUS_PIDS)):
            try:
                app = desktop[index]
                owner = app.get_process_id()
                if type(owner) is int and owner in owned:
                    facts["atspi_exact_apps" if owner == pid else "atspi_descendant_apps"] += 1
                    stack.append(app)
            except Exception:
                facts["atspi_queries_failed"] += 1
        while stack and facts["atspi_nodes"] < CENSUS_NODES:
            node = stack.pop()
            facts["atspi_nodes"] += 1
            try:
                name = node.name
                facts["named_search"] |= type(name) is str and name == "Search references"
                values = [name] if type(name) is str and len(name) <= 500 else []
                try:
                    text = node.queryText()
                    length = text.characterCount
                    if type(length) is not int or length < 0:
                        raise ValueError("Invalid census text length")
                    values.append(text.getText(0, min(length, 500)))
                except NotImplementedError:
                    pass
                facts["expected_count"] |= any(type(value) is str and re.search(r"(?<!\w)3 references(?!\w)", value) is not None for value in values)
                children = node.childCount
                if type(children) is not int or not 0 <= children <= 0x7fffffff:
                    raise ValueError("Invalid census node count")
                remaining = CENSUS_NODES - facts["atspi_nodes"] - len(stack)
                facts["atspi_bound_hit"] |= children > remaining
                stack.extend(node[index] for index in range(min(children, max(0, remaining))))
            except Exception:
                facts["atspi_queries_failed"] += 1
        facts["atspi_complete"] = not facts["atspi_bound_hit"] and not facts["atspi_queries_failed"]
    except Exception:
        facts["atspi_queries_failed"] += 1
    flush()
    return facts


def linux_census_command(argv, deadline):
    if not shutil.which(argv[0]):
        return None
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise subprocess.TimeoutExpired("census", 0)
    return subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                          timeout=min(.3, remaining), env=dict(os.environ, LC_ALL="C"))


def linux_failure_census(native):
    result = {"worker_completed": False, "worker_timed_out": False, "facts_valid": False}
    pid, wm_pid = native.app.pid, native.wm.pid
    output = ""
    try:
        child = subprocess.run([sys.executable, str(Path(__file__).resolve()), "--linux-failure-census", str(pid), str(wm_pid)],
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=4)
        result["worker_completed"], output = child.returncode == 0, child.stdout
    except subprocess.TimeoutExpired as error:
        result["worker_timed_out"], output = True, error.stdout
    except OSError:
        pass
    facts = linux_census_frame(output, pid, wm_pid)
    if facts is not None:
        result.update(facts_valid=True, facts=facts)
    return result


def linux_census_worker(pid, wm_pid):
    deadline = time.monotonic() + 3
    def alive(owner):
        try:
            os.kill(owner, 0)
            return True
        except OSError:
            return False
    linux_census(pid, wm_pid, alive, lambda argv: linux_census_command(argv, deadline),
                 lambda: __import__("pyatspi"), lambda frame: print(json.dumps(frame), flush=True))


class Linux:
    def __init__(self):
        for program in ("xdotool", "xclip", "xterm", "openbox", "xprop"):
            if not shutil.which(program):
                raise Unavailable(f"Linux native smoke needs {program}")
        if not os.environ.get("DISPLAY") or not os.environ.get("DBUS_SESSION_BUS_ADDRESS"):
            raise Unavailable("Linux smoke needs X11 and a D-Bus session (use xvfb-run and dbus-run-session)")
        try:
            import pyatspi
            from gi.repository import GLib
            self.atspi = pyatspi
            self.accessibility_error = GLib.Error
        except (ImportError, ValueError) as error:
            raise Unavailable("Use /usr/bin/python3 with distro python3-pyatspi installed") from error
        self.wm = subprocess.Popen(["openbox"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        # A window mapped before the window manager runs keeps no EWMH state:
        # GTK sends later hint changes such as skip-taskbar to the manager
        # instead of writing them. Desktops start their manager first; so must we.
        wait_for(self.window_manager_ready, "openbox manages the display")
        self.editor = subprocess.Popen(["xterm", "-title", "Ibis synthetic editor", "-e", "/bin/cat"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.app = None
        self.last_window_state = "not read"

    def window(self, pid):
        try:
            windows = run("xdotool", "search", "--onlyvisible", "--pid", str(pid)).splitlines()
        except subprocess.CalledProcessError:
            return None
        for window in windows:
            try:
                if run("xdotool", "getwindowname", window) == NOTICE_TITLE:
                    continue
            except subprocess.CalledProcessError:
                continue  # A window closed while being listed is not the picker.
            return window
        return None

    def snapshot(self):
        nodes = []
        desktop = self.atspi.Registry.getDesktop(0)
        stack = [a for a in desktop if a and a.get_process_id() == self.app.pid]
        while stack:
            item = stack.pop()
            if len(nodes) >= 4000:
                raise RuntimeError("Accessibility tree exceeded smoke traversal bound")
            try:
                state = item.getState()
                value = ""
                try:
                    text = item.queryText()
                    value = text.getText(0, min(text.characterCount, 500))
                except NotImplementedError:
                    pass
                nodes.append({"role": item.getRoleName(), "name": item.name or "", "value": value,
                              "focused": state.contains(self.atspi.STATE_FOCUSED)})
                stack.extend(c for c in item if c)
            except self.accessibility_error:
                continue
        window = self.window(self.app.pid)
        try:
            active = run("xdotool", "getactivewindow")
        except subprocess.CalledProcessError:
            active = ""
        return {"nodes": nodes, "visible": bool(window), "foreground": window == active if window else False}

    def key(self, chord):
        run("xdotool", "key", "--clearmodifiers", chord)

    def text(self, text):
        run("xdotool", "type", "--clearmodifiers", "--delay", "0", text)

    def click(self, name):
        desktop = self.atspi.Registry.getDesktop(0)
        stack = [a for a in desktop if a and a.get_process_id() == self.app.pid]
        while stack:
            item = stack.pop()
            if item.name == name:
                try:
                    action = item.queryAction()
                    if action.nActions and action.doAction(0):
                        return
                except NotImplementedError:
                    pass
            stack.extend(c for c in item if c)
        raise AssertionError(f"No invokable accessibility control {name!r}")

    def notice_visible(self):
        try:
            return bool(run("xdotool", "search", "--onlyvisible", "--pid", str(self.app.pid), "--name", f"^{NOTICE_TITLE}$"))
        except subprocess.CalledProcessError:
            return False

    def focus_editor(self):
        window = self.window(self.editor.pid)
        if not window:
            raise Unavailable("Synthetic terminal editor has no X11 window")
        run("xdotool", "windowactivate", "--sync", window)

    def editor_active(self):
        return self.window(self.editor.pid) == run("xdotool", "getactivewindow")

    def editor_focused(self):
        return self.editor_active() and self.editor_has_input_focus()

    def editor_has_input_focus(self):
        # getwindowfocus resolves to the WM_CLASS ancestor by default, so a
        # terminal child without _NET_WM_PID does not hide the actual owner.
        focused = run("xdotool", "getwindowfocus")
        return run("xdotool", "getwindowpid", focused) == str(self.editor.pid)

    def window_manager_ready(self):
        """Whether an EWMH window manager owns the display (the spec's two-way check)."""
        if self.wm.poll() is not None:
            raise RuntimeError("openbox exited during startup")
        try:
            window = supporting_wm_window(run("xprop", "-root", "_NET_SUPPORTING_WM_CHECK"))
            return window is not None and supporting_wm_window(
                run("xprop", "-id", window, "_NET_SUPPORTING_WM_CHECK")) == window
        except subprocess.CalledProcessError:
            return False

    def skips_taskbar(self):
        window = self.window(self.app.pid)
        # Window state atoms only, kept to explain a failure.
        self.last_window_state = f"{window}: {run('xprop', '-id', window, '_NET_WM_STATE')}" if window else "no visible window"
        return bool(window) and window_state_skips_taskbar(self.last_window_state.partition(": ")[2])

    def clipboard(self, *, timeout=20):
        # Clipboard equality includes whitespace; command-output normalization
        # is appropriate for window IDs, not copied citation text.
        return subprocess.check_output(["xclip", "-selection", "clipboard", "-out"],
                                       text=True, stderr=subprocess.PIPE, timeout=timeout)

    def set_clipboard(self, text):
        subprocess.run(["xclip", "-selection", "clipboard", "-in"], input=text, text=True, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=20)

    def memory(self):
        rows = [line.split() for line in run("ps", "-eo", "pid=,ppid=,rss=").splitlines()]
        pids = {self.app.pid}
        while True:
            found = pids | {int(pid) for pid, parent, _ in rows if int(parent) in pids}
            if found == pids:
                break
            pids = found
        return sum(int(rss) * 1024 for pid, _, rss in rows if int(pid) in pids)

    def close(self):
        self.editor.terminate()
        self.wm.terminate()


class Scripted:
    def __init__(self, system):
        self.system = system
        self.editor_folder = None
        self.editor_pid = 0
        initialized = False
        if system == "Windows":
            self.command = ["pwsh", "-NoProfile", "-File", str(Path(__file__).with_name("smoke-windows.ps1"))]
        else:
            self.command = ["osascript", "-l", "JavaScript", str(Path(__file__).with_name("smoke-macos.js"))]
        try:
            self.call("probe")
            if system == "Darwin":
                # TextEdit is a separate Apple Events target from System Events.
                # Open an owned file normally; UI inspection needs no TextEdit
                # scripting consent, and must never touch an existing instance.
                self.call("editor-preflight")
                self.editor_folder = tempfile.TemporaryDirectory(prefix="ibis synthetic editor ")
                self.editor_path = Path(self.editor_folder.name).resolve() / "Ibis synthetic editor.txt"
                self.editor_path.write_text("", encoding="utf-8")
                # Ignore saved state so a fresh instance never restores a
                # window left behind by an earlier synthetic editor.
                subprocess.run(["open", "-n", "-a", "TextEdit", str(self.editor_path),
                                "--args", "-ApplePersistenceIgnoreState", "YES"], check=True,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=20)
                def registered():
                    self.editor_pid = self.call("editor")
                    if type(self.editor_pid) is not int or self.editor_pid < 0:
                        raise RuntimeError("Synthetic editor process identity unavailable")
                    return self.editor_pid > 0
                wait_for(registered, "new synthetic TextEdit process is registered")
                try:
                    wait_for(lambda: self.call("editor-owned", str(self.editor_path)), "owned TextEdit document is accessible")
                except AssertionError:
                    state = self.call("editor-state", str(self.editor_path))
                    fields = ("bundle_verified", "window_count", "document_metadata_count",
                              "exact_document_url_matches", "owned_document_path_matches", "owned")
                    print("NATIVE EDITOR STATE " + json.dumps({key: state[key] for key in fields}), flush=True)
                    raise
            else:
                editor = self.call("editor")
                if not isinstance(editor, dict) or any(type(editor.get(key)) is not int or editor[key] <= 0
                                                       for key in ("pid", "started")):
                    raise RuntimeError("Synthetic editor process identity unavailable")
                self.editor_pid, self.editor_started = editor["pid"], editor["started"]
            initialized = True
        except FileNotFoundError as error:
            raise Unavailable("Native automation tool is missing") from error
        except subprocess.CalledProcessError as error:
            details = error.stderr or ""
            if system == "Darwin" and any(marker in details for marker in (
                "TCC Accessibility is not granted", "TCC event posting is not granted",
                "not allowed assistive access", "(-1743)",
            )):
                raise Unavailable("macOS Accessibility/Automation permission is unavailable; see helper diagnostics") from error
            raise RuntimeError("Native automation preflight failed; see helper diagnostics") from error
        finally:
            if not initialized and self.editor_folder:
                try:
                    self.close()
                except Exception:
                    # Keep the original preflight failure, without killing a
                    # process whose owned-document identity is unverified.
                    print("Native owned editor cleanup failed", file=sys.stderr)

    def call(self, action, text="", *, timeout=30):
        if self.system == "Windows" and action in ("focus-editor", "editor-focused", "close-editor"):
            text = str(self.editor_started)
        if getattr(self, "last_action", None) != action:
            print(f"NATIVE ACTION {action}", flush=True)
        self.last_action = action
        arguments = [action, str(getattr(getattr(self, "app", None), "pid", 0)), str(getattr(self, "editor_pid", 0)), text]
        if self.system == "Windows" and action == "click":
            # A modal popover hides Search from UIA, so discovery cannot run
            # again. Only the HWND established by this process's snapshot fits.
            handle = getattr(self, "picker_handle", None)
            if (getattr(self, "picker_app_pid", None) != getattr(getattr(self, "app", None), "pid", None)
                    or type(handle) is not int or handle == 0):
                raise RuntimeError("Verified native picker window identity unavailable")
            arguments.append(str(handle))
        try:
            result = subprocess.run(self.command + arguments, encoding="utf-8", stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)
        except subprocess.TimeoutExpired as error:
            # Never print TimeoutExpired itself: its command contains paths and
            # synthetic clipboard/query arguments. The fixed action is enough.
            for stage in helper_timeout_stages(error.stderr):
                print(stage, file=sys.stderr)
            raise AssertionError(f"Native helper timed out during {action} after {timeout:g}s") from None
        if result.returncode:
            # Helpers emit fixed diagnostics only, never tree or clipboard text.
            print(result.stderr, file=sys.stderr)
            raise subprocess.CalledProcessError(result.returncode, self.command, stderr=result.stderr)
        return json.loads(result.stdout)

    def snapshot(self, *, timeout=None):
        if self.system != "Windows":
            return self.call("snapshot", **({} if timeout is None else {"timeout": timeout}))
        deadline = None if timeout is None else time.monotonic() + timeout
        if getattr(self, "picker_app_pid", None) != self.app.pid:
            self.picker_handle = 0
            self.picker_app_pid = self.app.pid
        current = None
        def settled():
            # While the interface renders, a control can disappear between
            # listing the tree and reading it, and right after launch the
            # window can still be half created (#361). The helper reports only
            # that fact; any other helper failure still fails at once.
            nonlocal current
            options = {}
            if deadline is not None:
                remaining = deadline - time.monotonic()
                assert remaining > 0, "Native snapshot exceeded the Quit budget"
                options["timeout"] = remaining
            current = self.call("snapshot", str(self.picker_handle), **options)
            if current == {"tree_changed": True}:
                return False
            if not isinstance(current, dict) or "tree_changed" in current:
                raise RuntimeError("Native accessibility snapshot metadata unavailable")
            return True
        wait_for(settled, "native accessibility tree settles", timeout=TREE_SETTLE_SECONDS if timeout is None else min(TREE_SETTLE_SECONDS, timeout))
        handle = current.get("picker_handle")
        if handle is not None:
            if type(handle) is not int or handle == 0 or (self.picker_handle and handle != self.picker_handle):
                raise RuntimeError("Verified native picker window identity changed")
            self.picker_handle = handle
        elif current["visible"] or self.picker_handle:
            raise RuntimeError("Native picker window identity unavailable")
        return current

    def key(self, chord):
        self.call("key", chord)

    def text(self, text):
        self.call("text", text)

    def settle_editor_windows(self):
        """Close leftover windows of the owned editor, then require exactly one."""
        self.call("close-extra-editor-windows", str(self.editor_path))
        try:
            wait_for(lambda: self.call("editor-owned", str(self.editor_path)), "single owned synthetic editor window", timeout=10)
        except AssertionError:
            state = self.call("editor-state", str(self.editor_path))
            raise AssertionError(f"A single synthetic editor window is required; found {state['window_count']}") from None

    def click(self, name):
        if self.system != "Darwin":
            facts = control_activation_facts(self.call("click", name))
            print("NATIVE CONTROL STATE " + json.dumps(facts), flush=True)
            assert facts["pressed"], "Native button was not activated"
            return
        state = {}
        def pressed():
            nonlocal state
            state = self.call("click", name)
            return control_activation_facts(state)["pressed"]
        try:
            wait_for(pressed, "exact named native button becomes available")
        except AssertionError:
            print("NATIVE CONTROL STATE " + json.dumps(state), flush=True)
            raise
        print("NATIVE CONTROL STATE " + json.dumps(control_activation_facts(state)), flush=True)

    def focus_editor(self):
        state = self.call("focus-editor")
        if self.system == "Windows":
            keys = {"owned", "window_valid", "request_accepted", "current_foreground"}
            if not isinstance(state, dict) or set(state) != keys or any(type(value) is not bool for value in state.values()):
                raise RuntimeError("Native editor focus request metadata unavailable")
            if not state["owned"] or not state["window_valid"]:
                raise AssertionError("Native editor window identity was lost")
            print("NATIVE EDITOR FOCUS REQUEST " + json.dumps(state), flush=True)
            # SetForegroundWindow reports a request outcome. Only this fresh
            # native observation can satisfy focus, including before samples.
            wait_for(self.editor_focused, "owned editor receives actual foreground focus")

    def editor_focused(self):
        return self.call("editor-focused")

    def picker_key(self):
        state = self.call("picker-key")
        if type(state) is not bool:
            raise RuntimeError("Native picker key-window state unavailable")
        return state

    def clipboard(self, *, timeout=None):
        return self.call("clipboard", **({} if timeout is None else {"timeout": timeout}))

    def set_clipboard(self, text):
        self.call("write-clipboard", text)

    def memory(self):
        if self.system != "Windows":
            return self.call("memory")
        self.memory_facts = memory_facts(self.call("memory", str(windows_creation_identity(self.app))))
        return self.memory_facts["memory_bytes"]

    def close(self):
        if self.system == "Darwin":
            try:
                if self.editor_pid <= 0 or not self.call("editor-owned", str(self.editor_path)):
                    raise RuntimeError("Synthetic editor ownership could not be verified for cleanup")
                try:
                    os.kill(self.editor_pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                wait_for(lambda: not self.call("editor-running"), "owned TextEdit process exits after termination")
            finally:
                self.editor_folder.cleanup()
        else:
            self.call("close-editor")


def wait_for(predicate, description, timeout=20):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.1)
    raise AssertionError(f"Timed out: {description}")


def saved_typst(settings):
    if settings.is_symlink():
        raise AssertionError("Native preference file is a symlink")
    if not settings.exists():
        return False
    def unique(pairs):
        value = {}
        for key, item in pairs:
            assert key not in value, "Native preference JSON contains duplicate fields"
            value[key] = item
        return value
    def invalid_constant(_value):
        raise ValueError("Non-JSON numeric constant")
    with settings.open("rb") as source:
        content = source.read(65537)
    assert len(content) <= 65536, "Native preference file exceeded its bound"
    try:
        value = json.loads(content.decode("utf-8"), object_pairs_hook=unique, parse_constant=invalid_constant)
    except (UnicodeError, ValueError):
        raise AssertionError("Native preference JSON is malformed") from None
    assert (isinstance(value, dict) and type(value.get("schema_version")) is int
            and value["schema_version"] == 2), "Native preferences must use settings format 2"
    if value.get("copy_format") != "typst":
        return False
    assert value.get("citation_style") is None, "Native key format retained a citation style"
    return True


def quit_normally(native, app):
    deadline = time.monotonic() + 10
    def remaining():
        budget = deadline - time.monotonic()
        assert budget > 0, "Normal Quit exceeded its shared 10-second budget"
        return budget
    def activate(name):
        if isinstance(native, Scripted):
            result = native.call("click", name, timeout=remaining())
            facts = control_activation_facts(result)
            print("NATIVE CONTROL STATE " + json.dumps(facts), flush=True)
            assert facts["pressed"], "Native Quit control was not activated"
        else:
            native.click(name)
        remaining()
    def ready():
        current = native.snapshot(timeout=remaining()) if isinstance(native, Scripted) else native.snapshot()
        remaining()
        return current["visible"] and bool(named(current, "Quit Ibis"))
    activate("More")
    # The macOS click guard already acquires one exact named native control
    # and fails on absence or ambiguity before AXPress. A second full-tree
    # read can exhaust the Quit budget before that single guarded activation.
    if not (isinstance(native, Scripted) and native.system == "Darwin"):
        wait_for(ready, "Quit appears in More", timeout=remaining())
    activate("Quit Ibis")  # One activation; never retry an uncertain native action.
    assert app.wait(timeout=remaining()) == 0, "Normal Quit did not exit zero"
    # Use the same final sample for acceptance and elapsed-time evidence.
    return round(10 - remaining(), 3)


def first_run_preferences_main(args):
    args.output.mkdir(parents=True, exist_ok=True)
    system = platform.system()
    report = {"platform": system, "scenario": "first-run-preferences", "status": "failed", "checks": []}
    native = app = log = owned_settings = temporary = None
    exit_code = 1
    try:
        if not args.app.is_file() or system not in ("Linux", "Darwin", "Windows"):
            raise Unavailable("Build the embedded app on a supported native platform")
        if system == "Darwin":
            owned_settings = claim_settings(settings_path(system, Path("."), identifier=args.identifier))
        native = Linux() if system == "Linux" else Scripted(system)
        temporary = tempfile.TemporaryDirectory(prefix="ibis native preferences ")
        folder = Path(temporary.name)
        library = folder / "synthetic 引用.bib"
        library.write_text(fixture(), encoding="utf-8")
        settings = settings_path(system, folder, native, args.identifier)
        owned_settings = claim_settings(settings)
        env = real_app_environment(os.environ, library, folder / "config")
        shortcut = args.shortcut or ("ctrl+super+r" if system == "Darwin" else "ctrl+shift+y")

        def launch(label, with_library=False):
            nonlocal app, log
            if log:
                log.close()
            log = (args.output / f"startup-{label}.log").open("w", encoding="utf-8")
            child = dict(env)
            if not with_library:
                child.pop("IBIS_LIBRARY", None)
            answer_update_question(settings)
            app = subprocess.Popen([str(args.app.resolve())], env=child, stdout=log, stderr=log)
            native.app = app

        def snapshot():
            assert app.poll() is None, "Native app exited unexpectedly"
            current = native.snapshot()
            report["last_native_state"] = {
                "visible": bool(current["visible"]), "foreground": bool(current["foreground"]),
                "named_search": bool(named(current, "Search references")),
                "search_focused": search_focused(current),
                "result_focused": any(bool(node.get("focused")) and
                    node.get("role", "").lower().replace(" ", "") in
                    {"option", "listitem", "axrow", "controltype.listitem"} for node in current["nodes"]),
            }
            return current

        def visible_text(text):
            current = snapshot()
            return current["visible"] and text in strings(current)

        def picker_ready(*, typing=False):
            current = snapshot()
            return (current["visible"] and current["foreground"] and bool(named(current, "Search references"))
                    and (not typing or search_focused(current)))

        def passed(name):
            report["checks"].append({"name": name, "status": "passed"})
            print(f"PASS {name}", flush=True)

        launch("first-run")
        wait_for(lambda: visible_text("Choose library to get started"), "native first-run setup")
        native.click("Typst")
        wait_for(lambda: saved_typst(settings), "Typst preferences saved before copying")
        passed("first-run Typst choice is saved before any copy")
        report["first_quit_seconds"] = quit_normally(native, app)
        passed("settled first-run More/Quit exits zero within ten seconds")
        assert saved_typst(settings), "Normal Quit lost the saved Typst choice"
        launch("relaunch", with_library=True)
        wait_for(lambda: visible_text("3 references"), "synthetic references after relaunch")
        wait_for(lambda: picker_ready(typing=True), "restarted native search focus")
        type_query(native, snapshot, "smoke000001", "super+a" if system == "Darwin" else "ctrl+a")
        wait_for_query(snapshot, "smoke000001", "Ibis smoke reference 000001", report, settled=True)
        prime_clipboard(native)
        native.key("Return")
        copy_deadline = time.monotonic() + 20
        def copied():
            remaining = copy_deadline - time.monotonic()
            assert remaining > 0, "Native copy exceeded its clipboard-read budget"
            value = native.clipboard(timeout=remaining)
            assert time.monotonic() < copy_deadline, "Native copy exceeded its clipboard-read budget"
            return value == "@smoke000001"
        wait_for(copied, "saved Typst output on the OS clipboard")
        passed("relaunch uses saved Typst without choosing a format again")
        wait_for(lambda: not snapshot()["visible"], "Typst copy hides picker")
        native.key(shortcut)
        # AT-SPI can focus the active result while the combobox retains input.
        # More/Quit needs the same foreground/named-search state as reopen().
        wait_for(picker_ready, "picker available for normal Quit")
        report["second_quit_seconds"] = quit_normally(native, app)
        passed("restarted More/Quit exits zero within ten seconds")
        report["status"], exit_code = "passed", 0
    except Unavailable as error:
        report.update(status="unavailable", reason=str(error))
        exit_code = 2
    except (AssertionError, RuntimeError, ValueError, KeyError, TypeError, subprocess.SubprocessError, OSError) as error:
        report["reason"] = str(error) if isinstance(error, AssertionError) else type(error).__name__
    finally:
        def stop():
            if app and app.poll() is None:
                app.terminate()
                try:
                    app.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    app.kill()
                    app.wait(timeout=5)
        for cleanup in (stop, log.close if log else None, native.close if native else None,
                        (lambda: clean_settings(owned_settings)) if owned_settings else None,
                        temporary.cleanup if temporary else None):
            try:
                if cleanup:
                    cleanup()
            except (OSError, subprocess.SubprocessError, RuntimeError, AssertionError):
                report.update(status="failed", reason="Owned native preference cleanup failed")
                exit_code = 1
        print("NATIVE FIRST-RUN PREFERENCES " + json.dumps(report), flush=True)
        (args.output / "smoke.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return exit_code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--identifier", default="io.github.ibiscite.ibis", help="Bundle identifier, which names the settings folder")
    parser.add_argument("--shortcut", help="xdotool-style chord; defaults to Ctrl+Shift+Y or Control+Command+R on macOS")
    parser.add_argument("--budgets", type=Path, help="Measured per-platform baseline JSON, with startup_ms, first_result_ms, memory_bytes ceilings")
    parser.add_argument("--first-run-preferences", action="store_true", help="Exercise first-run format persistence and settled More/Quit only")
    args = parser.parse_args()
    if args.first_run_preferences:
        return first_run_preferences_main(args)
    args.output.mkdir(parents=True, exist_ok=True)
    system = platform.system()
    report = {"platform": system, "checks": [], "performance": {"status": "not-run"}, "status": "failed"}
    native = None
    app = None
    log = None
    exit_code = 1
    owned_settings = None
    diagnostic_logs = []
    started = time.monotonic()
    try:
        if not args.app.is_file():
            raise Unavailable("Build the embedded native app before running smoke")
        if system not in ("Linux", "Darwin", "Windows"):
            raise Unavailable("Unsupported native smoke platform")
        if system == "Darwin":
            owned_settings = claim_settings(settings_path(system, Path("."), identifier=args.identifier))
        native = Linux() if system == "Linux" else Scripted(system)
        with tempfile.TemporaryDirectory(prefix="ibis native smoke ") as temporary:
            folder = Path(temporary)
            library = folder / "synthetic 引用.bib"
            library.write_text(fixture(), encoding="utf-8")
            env = real_app_environment(os.environ, library, folder / "config")
            # Linux honors XDG_CONFIG_HOME. macOS/Windows use their native user
            # config locations; preflight refuses all existing preferences.
            # Preference choices can save even with IBIS_LIBRARY, so clean only
            # preference files this disposable run can create.
            settings = settings_path(system, folder, native, args.identifier)
            if system == "Windows":
                owned_settings = claim_settings(settings)

            def launch(label):
                nonlocal app, log
                log_path = args.output / f"startup-{label}.log"
                log = log_path.open("w", encoding="utf-8")
                diagnostic_logs.append(log_path)
                answer_update_question(settings)
                app = subprocess.Popen([str(args.app.resolve())], env=env, stdout=log, stderr=log)
                native.app = app

            def stop():
                nonlocal app, log
                if app and app.poll() is None:
                    app.terminate()
                    try:
                        app.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        app.kill()
                        app.wait(timeout=5)
                if log:
                    log.close()
                app = None

            def snapshot():
                assert app.poll() is None, "Native app exited unexpectedly"
                current = native.snapshot()
                # Only structural facts belong in failure evidence. Accessible
                # names/values can contain document or clipboard contents.
                report["last_native_state"] = {
                    "visible": current["visible"], "foreground": current["foreground"],
                    "search_focused": search_focused(current),
                    "focused_roles": sorted({node["role"] for node in current["nodes"] if node.get("focused")}),
                }
                return current

            def picker_ready():
                current = snapshot()
                return current["visible"] and current["foreground"] and search_focused(current)

            def capture_editor_focus(stage):
                if system == "Darwin":
                    facts = editor_focus_facts(native, stage)
                    report.setdefault("editor_focus_facts", []).append(facts)
                    print("NATIVE EDITOR FOCUS " + json.dumps(facts), flush=True)

            def check(name, action):
                try:
                    action()
                except Exception:
                    if system == "Linux":
                        try:
                            report["linux_census"] = linux_failure_census(native)
                        except Exception:
                            report["linux_census"] = {"collector_failed": True}
                        print("LINUX FAILURE CENSUS " + json.dumps(report["linux_census"]), file=sys.stderr)
                    raise
                report["checks"].append({"name": name, "status": "passed"})
                print(f"PASS {name}", flush=True)

            def query(text, expected):
                if system == "Windows":
                    current = snapshot()
                    if not current["visible"] or not current["foreground"]:
                        # Windows may retain editor activation after a restart.
                        # Summon deliberately before delivering real keystrokes.
                        native.key(shortcut)
                        wait_for(picker_ready, "Windows picker activated before typing")
                if system == "Darwin":
                    type_query(native, snapshot, text, "super+a")
                else:
                    native.key("ctrl+a")
                    native.text(text)
                wait_for_query(snapshot, text, expected, report)

            shortcut = args.shortcut or ("ctrl+super+r" if system == "Darwin" else "ctrl+shift+y")
            launch("small")
            check("startup", lambda: wait_for(lambda: shows_count(snapshot(), "3 references"), "synthetic library startup"))
            check("accessibility named search/count/list", lambda: require_tree(snapshot()))

            def skips_taskbar():
                # Window managers build the taskbar and Alt+Tab from these hints.
                try:
                    wait_for(native.skips_taskbar, "picker window skips the taskbar and pager")
                except AssertionError as error:
                    raise AssertionError(f"{error}; last {native.last_window_state}") from None
            if system == "Linux":
                check("first picker stays out of the taskbar and window switcher", skips_taskbar)

            def show():
                if system == "Darwin":
                    prepare_initial_picker_show(native, snapshot)
                native.focus_editor()
                wait_for(native.editor_focused, "previous editor focused")
                def focus_state(stage):
                    if system == "Linux":
                        snapshot()
                        state = dict(report["last_native_state"], stage=stage,
                                     editor_active=native.editor_active(),
                                     editor_input_focused=native.editor_has_input_focus())
                        report.setdefault("shortcut_states", []).append(state)
                        print("NATIVE SHORTCUT STATE " + json.dumps(state), flush=True)
                focus_state("before-first-shortcut")
                capture_editor_focus("before-first-shortcut")
                native.key(shortcut)
                focus_state("after-first-shortcut")
                wait_for(picker_ready, "global shortcut visible, foreground and search focus")
                escape_to_hide(native, snapshot, "Escape hides picker")
                native.focus_editor()
                wait_for(native.editor_focused, "previous editor focused")
                focus_state("before-second-shortcut")
                capture_editor_focus("before-second-shortcut")
                native.key(shortcut)
                focus_state("after-second-shortcut")
                wait_for(picker_ready, "global shortcut visible, foreground and search focus")
            check("global shortcut shows foreground picker with focused search", show)
            if system == "Linux":
                check("shown picker stays out of the taskbar and window switcher", skips_taskbar)

            def second_instance():
                escape_to_hide(native, snapshot, "picker hidden before second launch")
                native.focus_editor()
                wait_for(native.editor_focused, "editor focused before second launch")
                capture_editor_focus("before-second-launch")
                with (args.output / "startup-second-instance.log").open("w", encoding="utf-8") as second_log:
                    second = subprocess.Popen([str(args.app.resolve())], env=env, stdout=second_log, stderr=second_log)
                    try:
                        assert second.wait(timeout=20) == 0, "Second instance did not exit successfully"
                    finally:
                        if second.poll() is None:
                            second.terminate()
                            second.wait(timeout=10)
                wait_for(picker_ready, "second launch activates existing picker and focuses search")
                assert app.poll() is None, "Primary instance exited during second launch"
            check("second launch exits and activates existing picker", second_instance)
            check("native typing and accessible result count", lambda: query("smoke000001", "1 result"))
            assert "Ibis smoke reference 000001" in strings(snapshot()), "Expected native result missing"
            check("shortcut menu Escape returns to invoker", lambda: menu_escape_focus(
                native, snapshot, "super+k" if system == "Darwin" else "ctrl+k", report))
            # Restoring the query can schedule another search; do not copy the
            # temporary probe's results while that request is still pending.
            wait_for_query(snapshot, "smoke000001", "Ibis smoke reference 000001", report, settled=True)

            def copy():
                # Installed release builds never report the non-activating panel as the
                # key window although Return still reaches it (#454), so the
                # key-window wait stays out of the native copy check.
                copy_with_diagnostics(native, report)
                wait_for(lambda: not snapshot()["visible"], "copy hides picker")
                capture_editor_focus("after-copy-hides-picker")
                try:
                    wait_for(native.editor_focused, "copy restores focus to previous editor")
                except AssertionError:
                    capture_editor_focus("copy-restoration-timeout")
                    raise
                # The Copied notice (#496) is up while the editor keeps focus.
                # Only the Linux adapter can see it; others skip this check.
                if hasattr(native, "notice_visible"):
                    wait_for(lambda: native.notice_visible() and native.editor_focused(),
                             "Copied notice shows while the editor keeps focus", timeout=5)

            def live_settings_copy():
                paths = [settings.with_suffix(suffix) for suffix in (".json", ".json.bak", ".json.tmp")]
                prior = {path: path.read_bytes() if path.exists() else None for path in paths}
                external = json.loads(prior[settings]) if prior[settings] is not None else {"schema_version": 1}
                external["shortcut"] = "Control+Alt+F12"
                external.pop("copy_format", None)  # Copying must not recreate a removed preference.
                settings.parent.mkdir(parents=True, exist_ok=True)
                settings.write_text(json.dumps(external), encoding="utf-8")
                external_bytes = settings.read_bytes()
                try:
                    copy()
                    saved = json.loads(settings.read_text(encoding="utf-8"))
                    assert saved.get("shortcut") == external["shortcut"], "Native copy lost a live external settings edit"
                    assert settings.read_bytes() == external_bytes, "Native copy changed live settings"
                finally:
                    for path, content in prior.items():
                        if content is None:
                            path.unlink(missing_ok=True)
                        else:
                            path.write_bytes(content)
            check("Enter copies, hides, restores editor focus and preserves live settings edits", live_settings_copy)

            def reopen():
                native.key(shortcut)
                def shown():
                    current = snapshot()
                    return current["visible"] and current["foreground"] and named(current, "Search references")
                wait_for(shown, "shortcut shows foreground picker with named search")
                # AT-SPI can expose aria-activedescendant as the focused result
                # option while the combobox receives keyboard input. Prove real
                # focus and selection by typing, then inspect the exact value.
                native.text("smoke000002")  # Reopening must select the old query.
                def replaced():
                    current = snapshot()
                    return (any(node.get("value") == "smoke000002" for node in named(current, "Search references"))
                            and "Ibis smoke reference 000002" in strings(current) and "1 result" in strings(current))
                wait_for(replaced, "typing replaces selected prior query")
            check("reopened picker selects previous search", reopen)

            def reload():
                library.write_text(fixture(4), encoding="utf-8")
                wait_for(lambda: shows_count(snapshot(), "4 references"), "file watcher updates reference count")
                query("smoke000003", "Ibis smoke reference 000003")
            check("edited library reloads through native watcher", reload)

            def missing():
                library.unlink()
                menu_action(native, "Reload")
                def explained():
                    report["missing_library_evidence"] = missing_library_evidence(snapshot(), library.name)
                    return all(report["missing_library_evidence"].values())
                try:
                    wait_for(explained, "missing library explains recovery and retained reference count")
                except AssertionError:
                    print("MISSING LIBRARY EVIDENCE " + json.dumps(report.get("missing_library_evidence", {})), flush=True)
                    raise
            check("missing library shows an error", missing)

            def recover():
                library.write_text("", encoding="utf-8")
                menu_action(native, "Reload")
                wait_for(lambda: " · no references" in strings(snapshot()), "empty library count")
                library.write_text(fixture(), encoding="utf-8")
                menu_action(native, "Reload")
                wait_for(lambda: shows_count(snapshot(), "3 references"), "reload recovers after error")
            check("empty library and recovery after error", recover)
            stop()

            def settings_error(label, content, problem):
                settings.parent.mkdir(parents=True, exist_ok=True)
                settings.write_text(content, encoding="utf-8")
                backup = settings.with_suffix(".json.bak")
                if backup.exists():
                    backup.unlink()
                launch(label)
                wait_for(lambda: shows_count(snapshot(), "3 references"), f"{label} retains library access")
                def explained():
                    text = strings(snapshot())
                    return all(fragment in text for fragment in (problem,
                        "using defaults. Changes are not being saved while this problem remains.",
                        f"Settings file: {settings}"))
                wait_for(explained, f"{label} explains defaults, preservation and correct settings file")
                assert settings.read_text(encoding="utf-8") == content, "Invalid/newer settings were overwritten"
                stop()
                settings.unlink()

            check("invalid settings reported and preserved", lambda: settings_error("invalid-settings", "{invalid synthetic settings", "settings.json is not valid"))
            check("newer settings reported and preserved", lambda: settings_error("newer-settings", '{"schema_version":999}', "settings.json was written by a newer version of Ibis"))

            def damaged():
                library.write_text((Path(__file__).resolve().parent.parent / "fixtures/bibtex/damaged.bib").read_text(encoding="utf-8"), encoding="utf-8")
                launch("damaged")
                wait_for(lambda: library.name in strings(snapshot()) and " · 4 references · 5 reading notices" in strings(snapshot()),
                         "damaged library shows exact retained-reference and reading-notice counts")
                query("good2", "Loads after the damage")
                stop()
            check("damaged startup library warns and keeps valid records searchable", damaged)

            library.write_text(fixture(100000), encoding="utf-8")
            samples = []
            # Launch 0 is a warm-up held only to the hard ceiling; the gate uses the median of the rest (#464).
            for sample in range(1 + MEASURED_LAUNCHES):
                native.focus_editor()
                phases = {}
                # Retain partial facts if readiness or a native probe fails.
                report.setdefault("startup_phases", []).append(phases)
                startup = measure_startup(lambda: launch("100000-warmup" if sample == 0 else f"100000-{sample}"), snapshot, phases)
                before = time.monotonic()
                query("smoke099999", "Ibis smoke reference 099999")
                first = (time.monotonic() - before) * 1000
                memory = native.memory()
                assert memory > 0, "Native process-tree memory measurement unavailable"
                samples.append({"warmup": sample == 0, "startup_ms": round(startup, 2), "first_result_ms": round(first, 2),
                                "memory_bytes": memory, "startup_phases": phases})
                if system == "Windows":
                    samples[-1]["memory_facts"] = native.memory_facts
                stop()
                phases["library_phases"] = library_phase_diagnostics([diagnostic_logs[-1]])
            report["performance"] = summarize_performance(samples)
            if args.budgets:
                budgets = json.loads(args.budgets.read_text(encoding="utf-8"))[system]
                try:
                    enforce_performance_budgets(report["performance"], budgets)
                finally:
                    print(json.dumps(report["performance"]), flush=True)
            else:
                print(json.dumps(report["performance"]), flush=True)
        report["status"] = "passed"
        exit_code = 0
    except Unavailable as error:
        report.update(status="unavailable", reason=str(error))
        print(f"UNAVAILABLE: {error}", file=sys.stderr)
        exit_code = 2
    except (AssertionError, RuntimeError, ValueError, KeyError, TypeError, subprocess.SubprocessError, OSError) as error:
        report.update(reason=str(error) if isinstance(error, (AssertionError, Unavailable)) else type(error).__name__)
        print(f"FAIL: {report['reason']}", file=sys.stderr)
        if "last_native_state" in report:
            print("LAST NATIVE STATE " + json.dumps(report["last_native_state"]), file=sys.stderr)
        try:
            report["toggle_diagnostics"] = fixed_toggle_diagnostics(diagnostic_logs)
            for line in report["toggle_diagnostics"]:
                print("APP DIAGNOSTIC " + line, file=sys.stderr)
        except OSError:
            print("Native toggle diagnostics unavailable", file=sys.stderr)
        exit_code = 1
    finally:
        if app and app.poll() is None:
            app.terminate()
            try:
                app.wait(timeout=10)
            except subprocess.TimeoutExpired:
                app.kill()
        if log:
            log.close()
        try:
            report["library_phases"] = library_phase_diagnostics(diagnostic_logs)
        except OSError:
            report["library_phases"] = []
        if native:
            try:
                native.close()
            except (OSError, subprocess.SubprocessError, RuntimeError, AssertionError):
                report.update(status="failed", reason="Native editor cleanup failed")
                exit_code = 1
        if owned_settings:
            try:
                clean_settings(owned_settings)
            except OSError:
                report.update(status="failed", reason="Disposable settings cleanup failed")
                exit_code = 1
        report["elapsed_seconds"] = round(time.monotonic() - started, 2)
        (args.output / "smoke.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return exit_code


class ContractTests(unittest.TestCase):
    class CensusNode:
        def __init__(self, pid=0, name="", children=(), value=None):
            self.pid, self.name, self.children, self.value = pid, name, children, value
            self.childCount = len(children)
        def __getitem__(self, index):
            return self.children[index]
        def get_process_id(self):
            return self.pid
        def queryText(self):
            if self.value is None:
                raise NotImplementedError
            return Mock(characterCount=len(self.value), getText=lambda start, end: self.value[start:end])

    def census_commands(self, argv):
        rows = {("ps", "-eo", "pid=,ppid="): "100 1\n200 100\n300 200\n999 1\n",
                ("xprop", "-root", "_NET_CLIENT_LIST"): "_NET_CLIENT_LIST(WINDOW): window id # 0xa\n",
                ("xdotool", "search", "--pid", "100"): "10\n",
                ("xdotool", "search", "--pid", "200"): "11\n",
                ("xdotool", "search", "--pid", "300"): "12\n"}
        if argv[:2] == ["xprop", "-id"]:
            owner = {"10": 100, "11": 200, "12": 300}[argv[2]]
            text = f"_NET_WM_PID(CARDINAL) = {owner}\n_NET_WM_STATE(ATOM) = _NET_WM_STATE_HIDDEN\n"
        elif argv[0] == "xwininfo":
            state = {"10": "IsViewable", "11": "IsUnMapped", "12": "IsUnviewable"}[argv[2]]
            text = f'window 0x1 "PRIVATE_TITLE"\n  Map State: {state}\n'
        else:
            text = rows[tuple(argv)]
        return subprocess.CompletedProcess(argv, 0, text, "")

    def census_fixture(self, count="3 references", *, runner=None, frames=None):
        desktop = self.CensusNode(children=[self.CensusNode(100, "PRIVATE_TITLE", [
            self.CensusNode(name="Search references"), self.CensusNode(value=count)]), self.CensusNode(999, "PRIVATE_OTHER")])
        def bridge():
            self.assertTrue(frames, "process/X facts must precede AT-SPI discovery")
            return Mock(Registry=Mock(getDesktop=lambda _: desktop))
        return linux_census(100, 55, lambda _: True, runner or self.census_commands, bridge, frames.append)

    def test_linux_census_flushes_owned_unmapped_process_x_facts_before_atspi(self):
        frames = []
        facts = self.census_fixture(frames=frames)
        self.assertFalse(frames[0]["atspi_attempted"])
        self.assertEqual((frames[0]["exact_windows"], frames[0]["descendant_windows"]), (1, 2))
        self.assertEqual(facts["descendant_pids"], [200, 300])
        self.assertEqual(facts["map_states"]["descendant"]["IsUnMapped"], 1)
        self.assertTrue(facts["named_search"] and facts["expected_count"] and facts["atspi_complete"])
        self.assertNotIn("PRIVATE", json.dumps(frames))

    def test_linux_census_does_not_accept_thirteen_as_three_references(self):
        for text, expected in (("13 references", False), ("123 references", False), ("3 references", True),
                               ("example.bib · 3 references · updated", True)):
            with self.subTest(text=text):
                self.assertEqual(self.census_fixture(text, frames=[])["expected_count"], expected)

    def test_linux_census_missing_failed_and_empty_queries_are_distinct(self):
        for result, available, complete, empty in ((None, False, False, 0),
                (subprocess.CompletedProcess([], 1, "", "PRIVATE_ERROR"), True, False, 0),
                (subprocess.CompletedProcess([], 1, "", ""), True, True, 3)):
            def runner(argv):
                return result if argv[0] == "xdotool" else self.census_commands(argv)
            facts = self.census_fixture(runner=runner, frames=[])
            self.assertEqual((facts["window_query_available"], facts["window_query_complete"], facts["window_queries_empty"]),
                             (available, complete, empty))
            self.assertNotIn("PRIVATE", json.dumps(facts))

    def test_linux_census_rejects_private_or_wrong_identity_frames_and_retains_timeout_partial(self):
        facts = self.census_fixture(frames=[])
        native = Mock(app=Mock(pid=100), wm=Mock(pid=55))
        for invalid in ({**facts, "PRIVATE_PATH": "PRIVATE_PAYLOAD"}, {**facts, "app_pid": 999},
                        {**facts, "atspi_nodes": "PRIVATE_TEXT"}):
            self.assertIsNone(linux_census_frame(json.dumps(invalid), 100, 55))
        with patch("subprocess.run", side_effect=subprocess.TimeoutExpired([], 4, output=json.dumps(facts).encode(), stderr=b"PRIVATE_ERROR")):
            result = linux_failure_census(native)
        self.assertTrue(result["worker_timed_out"] and result["facts_valid"])
        self.assertEqual(result["facts"], facts)
        self.assertNotIn("PRIVATE", json.dumps(result))

    def test_linux_census_commands_are_available_and_separately_time_bounded(self):
        with patch("shutil.which", return_value=None), patch("subprocess.run") as command:
            self.assertIsNone(linux_census_command(["missing"], time.monotonic() + 3))
            command.assert_not_called()
        with patch("shutil.which", return_value="synthetic-tool"), patch("subprocess.run") as command:
            linux_census_command(["xprop", "-root", "_NET_CLIENT_LIST"], time.monotonic() + 3)
            self.assertLessEqual(command.call_args.kwargs["timeout"], .3)
            self.assertEqual(command.call_args.kwargs["env"]["LC_ALL"], "C")

    def test_linux_census_bounds_cycles_and_rejects_unbounded_text_reads(self):
        node = self.CensusNode(100)
        node.children, node.childCount = [node], 1
        bridge = Mock(Registry=Mock(getDesktop=lambda _: self.CensusNode(children=[node])))
        facts = linux_census(100, 55, lambda _: True, self.census_commands, lambda: bridge, lambda _: None)
        self.assertTrue(facts["atspi_bound_hit"])
        self.assertEqual(facts["atspi_nodes"], CENSUS_NODES)
        read = Mock(characterCount=-1)
        node.children, node.childCount, node.queryText = [], 0, lambda: read
        facts = linux_census(100, 55, lambda _: True, self.census_commands, lambda: bridge, lambda _: None)
        read.getText.assert_not_called()
        self.assertFalse(facts["atspi_complete"])

    def test_linux_census_blocked_desktop_retains_flushed_process_x_frame(self):
        source = ("import runpy,json,time; from types import SimpleNamespace as S; "
                  "m=runpy.run_path(" + repr(str(Path(__file__).resolve())) + "); "
                  "bridge=S(Registry=S(getDesktop=lambda _: time.sleep(30))); "
                  "m['linux_census'](100,55,lambda _: True,lambda _: None,lambda: bridge,lambda f: print(json.dumps(f),flush=True))")
        command = [sys.executable, "-c", source]
        execute = subprocess.run
        with patch("subprocess.run", side_effect=lambda _argv, **options: execute(command, **options)):
            result = linux_failure_census(Mock(app=Mock(pid=100), wm=Mock(pid=55)))
        self.assertTrue(result["worker_timed_out"] and result["facts_valid"])
        self.assertTrue(result["facts"]["atspi_available"])
        self.assertFalse(result["facts"]["atspi_desktop_succeeded"])

    def test_linux_census_errors_keep_the_original_failure_and_teardown(self):
        for error in (RuntimeError("PRIVATE_COLLECTOR_ERROR"), subprocess.TimeoutExpired([], 4)):
            native, app = Mock(), Mock(pid=100)
            app.poll.return_value = None
            class Native:
                def __init__(self):
                    self.wm = Mock(pid=55)
                def close(self):
                    native.close()
            with tempfile.TemporaryDirectory() as temporary:
                folder = Path(temporary)
                executable, output = folder / "synthetic-app", folder / "output"
                executable.touch()
                with patch("sys.argv", ["smoke", "--app", str(executable), "--output", str(output)]), \
                     patch("platform.system", return_value="Linux"), patch(__name__ + ".Linux", Native), \
                     patch("subprocess.Popen", return_value=app), patch("builtins.print"), \
                     patch(__name__ + ".wait_for", side_effect=AssertionError("Timed out: synthetic library startup")), \
                     patch(__name__ + ".linux_failure_census", side_effect=error, create=True) as collector:
                    self.assertEqual(main(), 1)
                report = json.loads((output / "smoke.json").read_text())
                collector.assert_called_once()
                self.assertEqual(report["reason"], "Timed out: synthetic library startup")
                self.assertTrue(report["linux_census"]["collector_failed"])
                self.assertNotIn("PRIVATE", json.dumps(report))
                app.terminate.assert_called_once()
                native.close.assert_called_once()

    def preference_probe(self, failure=None, *, scripted_system=None):
        clock, launches, actions = [0.0], [], []
        # Exercise the Scripted Quit protocol while keeping settings in the
        # existing owned Linux fixture; this is a control-flow contract only.
        native = Mock(spec=Scripted) if scripted_system else Mock()
        if scripted_system:
            native.system = scripted_system
        native.query = ""
        native.query_ready_at = 0
        native.value = ""
        native.visible = True
        native.reopened = False
        native.more_open = False

        def launch(_command, *, env, **_options):
            app = Mock()
            app.returncode = None
            app.poll.side_effect = lambda: app.returncode
            def wait(timeout):
                if failure == "stuck-quit" and app.returncode is None:
                    raise subprocess.TimeoutExpired("owned app", timeout)
                if failure == "late-wait":
                    clock[0] += 11
                return app.returncode
            app.wait.side_effect = wait
            app.terminate.side_effect = lambda: setattr(app, "returncode", -15)
            launches.append(dict(env))
            native.app = app
            native.visible = True
            return app

        def click(name):
            actions.append((name, len(launches)))
            if name == "Typst":
                settings = Path(launches[0]["XDG_CONFIG_HOME"]) / "io.github.ibiscite.ibis/settings.json"
                value = '{"schema_version":2,"copy_format":"typst"}'
                if failure == "wrong-format":
                    value = '{"schema_version":2,"copy_format":"pandoc"}'
                elif failure == "duplicate":
                    value = '{"schema_version":2,"copy_format":"pandoc","copy_format":"typst"}'
                elif failure == "malformed":
                    value = '{invalid'
                elif failure == "wrong-schema":
                    value = '{"schema_version":true,"copy_format":"typst"}'
                elif failure == "retained-style":
                    value = '{"schema_version":2,"copy_format":"typst","citation_style":"apa"}'
                elif failure == "non-json":
                    value = '{"schema_version":2,"copy_format":"typst","invalid":NaN}'
                elif failure == "oversized":
                    value = json.dumps({"schema_version": 2, "copy_format": "typst", "padding": "x" * 65536})
                settings.write_text(value, encoding="utf-8")
            elif name == "Quit Ibis":
                native.more_open = False
                if failure == "late-driver":
                    clock[0] += 11
                if failure != "stuck-quit":
                    native.app.returncode = 7 if failure == "nonzero-quit" else 0
                if failure == "uncertain-quit":
                    raise RuntimeError("Synthetic uncertain activation")
            elif name == "More":
                native.more_open = True

        def key(chord):
            actions.append((chord, len(launches)))
            if chord == "Return":
                self.assertEqual(len(launches), 2, "Copy happened before relaunch")
                if clock[0] < native.query_ready_at:
                    return  # The app rejects Enter while results belong to the old query.
                native.value = "[@smoke000001]" if failure == "wrong-copy" else "@smoke000001"
                if failure == "whitespace-copy":
                    native.value += "\n"
                native.visible = False
            elif chord == "ctrl+shift+y":
                native.visible = failure != "hidden-reopen"
                native.reopened = True

        def snapshot(**_options):
            if failure == "slow-menu-snapshot" and native.more_open:
                clock[0] += 8
            names = ["More", "Quit Ibis"]
            if len(launches) == 1:
                names += ["Choose library to get started", "Typst"]
            else:
                names += ["3 references", "1 result" if clock[0] >= native.query_ready_at else "3 results",
                          "Ibis smoke reference 000001"]
            result_focus = failure == "active-result-reopen" and native.reopened
            return {"visible": native.visible, "foreground": native.visible, "nodes": [
                {"name": name, "role": "button"} for name in names] + [
                {"name": "Search references", "role": "entry", "value": native.query, "focused": not result_focus},
                {"name": "private result payload", "role": "list item", "focused": result_focus}]}

        def text(value):
            native.query = value
            if failure == "delayed-query":
                native.query_ready_at = clock[0] + 0.2

        native.click.side_effect = click
        native.key.side_effect = key
        native.text.side_effect = text
        native.snapshot.side_effect = snapshot
        native.set_clipboard.side_effect = lambda value: setattr(native, "value", value)
        if scripted_system:
            def helper(action, name, *, timeout):
                self.assertEqual(action, "click")
                self.assertGreater(timeout, 0)
                clock[0] += 2
                if name == "Quit Ibis" and failure == "absent-quit":
                    actions.append((name, len(launches)))
                    return {"pressed": False}
                click(name)
                return {"pressed": True}
            native.call.side_effect = helper
        original_clipboard = Linux.clipboard
        native.clipboard.side_effect = lambda **kwargs: original_clipboard(native, **kwargs)
        def clipboard_read(*_args, **_kwargs):
            if failure == "late-copy" and native.value == "@smoke000001":
                clock[0] += 25
            return native.value
        if failure == "cleanup":
            native.close.side_effect = RuntimeError("Synthetic cleanup failure")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            app = root / "synthetic-app"
            app.touch()
            args = argparse.Namespace(app=app, output=root / "report", identifier="io.github.ibiscite.ibis", shortcut=None)
            candidate = root / "preexisting/settings.json"
            candidate.parent.mkdir()
            if failure == "preexisting":
                candidate.write_text("preserve this file", encoding="utf-8")
            elif failure == "symlink":
                try:
                    candidate.symlink_to(root / "missing-target")
                except OSError:
                    self.skipTest("Creating symlinks requires an unavailable native capability")
            original_settings_path = settings_path
            locate = lambda *args, **kwargs: candidate if failure in ("preexisting", "symlink") else original_settings_path(*args, **kwargs)
            with patch("platform.system", return_value="Linux"), patch.object(sys.modules[__name__], "Linux", return_value=native), \
                    patch.object(sys.modules[__name__], "settings_path", side_effect=locate), \
                    patch("subprocess.Popen", side_effect=launch), patch("time.monotonic", side_effect=lambda: clock[0]), \
                    patch("subprocess.check_output", side_effect=clipboard_read), \
                    patch("time.sleep", side_effect=lambda seconds: clock.__setitem__(0, clock[0] + seconds)), \
                    patch.dict(os.environ, {"IBIS_LIBRARY": "/unowned.bib", "IBIS_GALLERY": "1", "IBIS_GALLERY_STATE": "loaded"}), \
                    patch("builtins.print"):
                result = first_run_preferences_main(args)
            report = json.loads((args.output / "smoke.json").read_text(encoding="utf-8"))
            if failure == "preexisting":
                self.assertEqual(candidate.read_text(encoding="utf-8"), "preserve this file")
            elif failure == "symlink":
                self.assertTrue(candidate.is_symlink())
            if launches:
                self.assertFalse(Path(launches[0]["XDG_CONFIG_HOME"]).exists())
        native.close.assert_called_once()
        return result, report, launches, actions

    def test_first_run_preferences_relaunch_uses_saved_typst_before_copy(self):
        result, report, launches, actions = self.preference_probe()
        self.assertEqual((result, report["status"]), (0, "passed"))
        self.assertEqual(len(launches), 2)
        self.assertNotIn("IBIS_LIBRARY", launches[0])
        self.assertIn("IBIS_LIBRARY", launches[1])
        self.assertEqual(launches[0]["XDG_CONFIG_HOME"], launches[1]["XDG_CONFIG_HOME"])
        self.assertTrue(all("IBIS_GALLERY" not in env and "IBIS_GALLERY_STATE" not in env for env in launches))
        self.assertEqual([action for action in actions if action[0] == "Typst"], [("Typst", 1)])
        self.assertEqual([action for action in actions if action[0] == "Quit Ibis"], [("Quit Ibis", 1), ("Quit Ibis", 2)])

    def test_first_run_preferences_failures_cannot_pass_during_cleanup(self):
        for failure in ("wrong-format", "duplicate", "malformed", "wrong-schema", "retained-style", "non-json", "oversized",
                        "late-driver", "late-wait", "nonzero-quit", "stuck-quit", "uncertain-quit", "wrong-copy", "cleanup"):
            with self.subTest(failure=failure):
                result, report, _launches, _actions = self.preference_probe(failure)
                self.assertEqual((result, report["status"]), (1, "failed"))

    def test_first_run_preferences_reject_late_or_nonexact_os_clipboard_reads(self):
        for failure in ("late-copy", "whitespace-copy"):
            with self.subTest(failure=failure):
                result, report, _launches, actions = self.preference_probe(failure)
                self.assertEqual((result, report["status"]), (1, "failed"))
                self.assertEqual([action for action in actions if action[0] == "Return"], [("Return", 2)])

    def test_first_run_preferences_wait_for_results_before_one_return(self):
        result, report, _launches, actions = self.preference_probe("delayed-query")
        self.assertEqual((result, report["status"]), (0, "passed"))
        self.assertEqual([action for action in actions if action[0] == "Return"], [("Return", 2)])

    def test_first_run_preferences_reopen_accepts_focused_active_result(self):
        result, report, _launches, actions = self.preference_probe("active-result-reopen")
        self.assertEqual((result, report["status"]), (0, "passed"))
        self.assertEqual(report["last_native_state"], {
            "visible": True, "foreground": True, "named_search": True,
            "search_focused": False, "result_focused": True,
        })
        self.assertEqual([action for action in actions if action[0] == "Return"], [("Return", 2)])
        self.assertEqual([action for action in actions if action[0] == "Quit Ibis"], [("Quit Ibis", 1), ("Quit Ibis", 2)])

    def test_first_run_preferences_darwin_quit_uses_one_guarded_activation_without_slow_full_tree_read(self):
        result, report, _launches, actions = self.preference_probe("slow-menu-snapshot", scripted_system="Darwin")
        self.assertEqual((result, report["status"]), (0, "passed"))
        self.assertEqual((report["first_quit_seconds"], report["second_quit_seconds"]), (4, 4))
        self.assertEqual([action for action in actions if action[0] == "Return"], [("Return", 2)])
        self.assertEqual([action for action in actions if action[0] == "Quit Ibis"], [("Quit Ibis", 1), ("Quit Ibis", 2)])

    def test_first_run_preferences_darwin_quit_still_rejects_absent_uncertain_or_late_actions(self):
        for failure in ("absent-quit", "uncertain-quit", "late-driver", "late-wait", "nonzero-quit", "stuck-quit"):
            with self.subTest(failure=failure):
                result, report, _launches, actions = self.preference_probe(failure, scripted_system="Darwin")
                self.assertEqual((result, report["status"]), (1, "failed"))
                self.assertEqual([action for action in actions if action[0] == "Quit Ibis"], [("Quit Ibis", 1)])

    def test_first_run_preferences_windows_keeps_its_menu_readiness_observation_in_the_shared_budget(self):
        result, report, _launches, actions = self.preference_probe("slow-menu-snapshot", scripted_system="Windows")
        self.assertEqual((result, report["status"]), (1, "failed"))
        self.assertIn("shared 10-second budget", report["reason"])
        self.assertEqual([action for action in actions if action[0] == "Quit Ibis"], [])

    def test_first_run_preferences_failure_retains_only_observed_boolean_state(self):
        result, report, _launches, actions = self.preference_probe("hidden-reopen")
        self.assertEqual((result, report["status"]), (1, "failed"))
        self.assertEqual(report["last_native_state"], {
            "visible": False, "foreground": False, "named_search": True,
            "search_focused": True, "result_focused": False,
        })
        self.assertTrue(all(type(value) is bool for value in report["last_native_state"].values()))
        self.assertNotIn("private", json.dumps(report))
        self.assertEqual([action for action in actions if action[0] == "Quit Ibis"], [("Quit Ibis", 1)])

    def test_scripted_calls_accept_a_remaining_budget_without_resetting_it(self):
        native = Scripted.__new__(Scripted)
        native.system = "Darwin"
        native.command = ["synthetic-helper"]
        completed = subprocess.CompletedProcess([], 0, stdout="true", stderr="")
        with patch("subprocess.run", return_value=completed) as helper, patch("builtins.print"):
            self.assertIs(native.call("click", "More", timeout=3.5), True)
        self.assertEqual(helper.call_args.kwargs["timeout"], 3.5)

    def test_first_run_preferences_refuse_existing_settings_and_dangling_links(self):
        for failure in ("preexisting", "symlink"):
            with self.subTest(failure=failure):
                result, report, launches, _actions = self.preference_probe(failure)
                self.assertEqual((result, report["status"]), (2, "unavailable"))
                self.assertEqual(launches, [])

    def test_normal_quit_shares_its_budget_with_each_scripted_call_and_wait(self):
        native = Scripted.__new__(Scripted)
        native.system = "Darwin"
        clock = [0.0]
        def helper(action, text="", *, timeout):
            clock[0] += 2
            return {"pressed": True} if action == "click" else {"visible": True, "nodes": [{"name": "Quit Ibis"}]}
        native.call = Mock(side_effect=helper)
        app = Mock()
        def exited(timeout):
            clock[0] += 1
            return 0
        app.wait.side_effect = exited
        with patch("time.monotonic", side_effect=lambda: clock[0]):
            self.assertEqual(quit_normally(native, app), 5)
        self.assertEqual([entry.kwargs["timeout"] for entry in native.call.call_args_list], [10, 8])
        app.wait.assert_called_once_with(timeout=6)
        self.assertEqual([entry.args for entry in native.call.call_args_list], [("click", "More"), ("click", "Quit Ibis")])
        clock[0] = 0
        app.wait.side_effect = lambda timeout: (clock.__setitem__(0, 11), 0)[1]
        with patch("time.monotonic", side_effect=lambda: clock[0]):
            with self.assertRaisesRegex(AssertionError, "shared 10-second budget"):
                quit_normally(native, app)
    def test_taskbar_hints_are_read_from_the_window_manager_state(self):
        self.assertTrue(window_state_skips_taskbar(
            "_NET_WM_STATE(ATOM) = _NET_WM_STATE_FOCUSED, _NET_WM_STATE_SKIP_TASKBAR, _NET_WM_STATE_SKIP_PAGER"))
        for line in ("_NET_WM_STATE(ATOM) = _NET_WM_STATE_SKIP_PAGER", "_NET_WM_STATE:  not found.",
                     "_NET_WM_STATE(ATOM) = ", "_NET_WM_STATE(ATOM) = _NET_WM_STATE_SKIP_TASKBAR_X, _NET_WM_STATE_SKIP_PAGER"):
            self.assertFalse(window_state_skips_taskbar(line), line)

    def test_linux_starts_the_editor_only_once_the_window_manager_is_ready(self):
        events = []
        ready = iter([False, False, True])
        def launch(command, **kwargs):
            events.append(command[0])
            return Mock(poll=Mock(return_value=None))
        def check(native):
            events.append("ready?")
            return next(ready)
        modules = {"pyatspi": Mock(), "gi": Mock(), "gi.repository": Mock()}
        with patch("shutil.which", return_value="/usr/bin/tool"), patch.dict(os.environ, {"DISPLAY": ":99", "DBUS_SESSION_BUS_ADDRESS": "unix:x"}), \
                patch.dict(sys.modules, modules), patch("subprocess.Popen", side_effect=launch), \
                patch.object(Linux, "window_manager_ready", check), patch("time.sleep"):
            Linux()
        self.assertEqual(events, ["openbox", "ready?", "ready?", "ready?", "xterm"])

    def test_a_window_manager_that_exits_is_reported_not_waited_for(self):
        native = Linux.__new__(Linux)
        native.wm = Mock(poll=Mock(return_value=1))
        with self.assertRaisesRegex(RuntimeError, "openbox exited"):
            native.window_manager_ready()

    def test_window_manager_readiness_reads_the_supporting_window(self):
        self.assertEqual(supporting_wm_window("_NET_SUPPORTING_WM_CHECK(WINDOW): window id # 0x20011f"), "0x20011f")
        for line in ("_NET_SUPPORTING_WM_CHECK:  not found.", "_NET_SUPPORTING_WM_CHECK(WINDOW): window id # ",
                     "_NET_SUPPORTING_WM_CHECK(WINDOW): window id # 0x20011f, 0x1"):
            self.assertIsNone(supporting_wm_window(line), line)

    class CopyClock:
        def __init__(self, step=10):
            self.elapsed = 0
            self.step = step

        def monotonic(self):
            return self.elapsed

        def sleep(self, interval):
            self.elapsed += self.step

    @staticmethod
    def copy_snapshot(visible=True, focused=True, error=False):
        return {"visible": visible, "nodes": [
            {"name": "Search references", "role": "AXComboBox", "value": "smoke000001", "focused": focused},
            {"name": "Ibis smoke reference 000001", "role": "AXStaticText"},
            {"name": "1 result", "role": "AXStaticText"},
            {"name": "Nothing was copied. Try again, or use Reload." if error else "", "role": "AXStaticText"},
        ]}

    def test_native_copy_accepts_a_later_equality_poll_without_another_return(self):
        native, report, clock = Mock(), {}, self.CopyClock(19)
        sentinel = "Ibis native smoke clipboard sentinel"
        native.clipboard.side_effect = [sentinel, sentinel, "[@smoke000001]"]
        native.snapshot.return_value = self.copy_snapshot()
        with patch(__name__ + ".time.monotonic", clock.monotonic), patch(__name__ + ".time.sleep", clock.sleep):
            copy_with_diagnostics(native, report)
        self.assertEqual(native.key.call_args_list, [call("Return")])
        self.assertEqual(native.clipboard.call_count, 3)
        self.assertEqual(native.snapshot.call_count, 1)
        self.assertNotIn("copy_failure", report)
        self.assertEqual(native.mock_calls[:4], [call.set_clipboard(sentinel), call.clipboard(), call.snapshot(), call.key("Return")])

    def test_return_waits_until_the_picker_owns_the_keyboard(self):
        native, report, clock = Mock(), {}, self.CopyClock(1)
        sentinel = "Ibis native smoke clipboard sentinel"
        native.clipboard.side_effect = [sentinel, "[@smoke000001]"]
        native.snapshot.return_value = self.copy_snapshot()
        native.picker_key.side_effect = [False, False, True]
        with patch(__name__ + ".time.monotonic", clock.monotonic), patch(__name__ + ".time.sleep", clock.sleep):
            copy_with_diagnostics(native, report, native.picker_key)
        self.assertEqual(native.mock_calls[:7], [call.set_clipboard(sentinel), call.clipboard(), call.snapshot(),
                                                 call.picker_key(), call.picker_key(), call.picker_key(), call.key("Return")])
        self.assertNotIn("copy_failure", report)

    def test_return_is_never_sent_while_another_window_owns_the_keyboard(self):
        native, report, clock = Mock(), {}, self.CopyClock()
        native.clipboard.return_value = "Ibis native smoke clipboard sentinel"
        native.snapshot.return_value = self.copy_snapshot()
        native.picker_key.return_value = False
        with patch(__name__ + ".time.monotonic", clock.monotonic), patch(__name__ + ".time.sleep", clock.sleep), \
             patch("sys.stderr", new_callable=io.StringIO), \
             self.assertRaisesRegex(AssertionError, "Timed out: picker is the key window before Return"):
            copy_with_diagnostics(native, report, native.picker_key)
        native.key.assert_not_called()
        self.assertFalse(report["copy_failure"]["key_window"])
        self.assertTrue(report["copy_failure"]["before_return"]["search_focused"])

    def test_clipboard_timeout_records_key_window_once_and_quietly(self):
        for probe, expected in ((Mock(side_effect=[True, False]), False),
                                (Mock(side_effect=[True, RuntimeError("private helper payload")]), None)):
            with self.subTest(expected=expected):
                native, report, clock = Mock(), {}, self.CopyClock()
                native.clipboard.return_value = "Ibis native smoke clipboard sentinel"
                native.snapshot.return_value = self.copy_snapshot()
                errors = io.StringIO()
                with patch(__name__ + ".time.monotonic", clock.monotonic), patch(__name__ + ".time.sleep", clock.sleep), \
                     patch("sys.stderr", errors), self.assertRaisesRegex(AssertionError, "expected citation on OS clipboard"):
                    copy_with_diagnostics(native, report, probe)
                self.assertIs(report["copy_failure"]["key_window_after_timeout"], expected)
                self.assertEqual(probe.call_count, 2)
                self.assertEqual(native.key.call_args_list, [call("Return")])
                self.assertNotIn("private", errors.getvalue() + json.dumps(report))

    def test_native_copy_timeout_records_fresh_state_without_reading_clipboard_again(self):
        native, report, clock = Mock(), {"last_native_state": {"visible": True}}, self.CopyClock()
        sentinel = "Ibis native smoke clipboard sentinel"
        native.clipboard.side_effect = [sentinel, sentinel, sentinel, "[@smoke000001]"]
        native.snapshot.side_effect = [self.copy_snapshot(), self.copy_snapshot(visible=False, focused=False, error=True)]
        with patch(__name__ + ".time.monotonic", clock.monotonic), patch(__name__ + ".time.sleep", clock.sleep), \
             patch("sys.stderr", new_callable=io.StringIO), self.assertRaisesRegex(AssertionError, "Timed out: expected citation on OS clipboard"):
            copy_with_diagnostics(native, report)
        facts = report["copy_failure"]
        self.assertTrue(facts["before_return"]["visible"])
        self.assertFalse(facts["after_timeout"]["visible"])
        self.assertFalse(facts["after_timeout"]["search_focused"])
        self.assertTrue(facts["after_timeout"]["copy_error_present"])
        self.assertEqual(facts["clipboard"], {"polls": 2, "elapsed_seconds": 20, "expected": False, "sentinel": True, "empty": False})
        self.assertEqual(native.key.call_args_list, [call("Return")])
        self.assertEqual(native.clipboard.call_count, 3, "A late diagnostic read must not replace the failed final poll")

    def test_native_clipboard_read_errors_keep_fresh_private_failure_facts(self):
        private = "SYNTHETIC_PRIVATE_HELPER_CLIPBOARD_EXCEPTION"
        failures = [OSError(private), subprocess.SubprocessError(private),
                    subprocess.CalledProcessError(7, [private], output=private, stderr=private),
                    subprocess.TimeoutExpired([private], 1, output=private, stderr=private)]
        for original in failures:
            for completed in (0, 1):
                with self.subTest(error=type(original).__name__, completed=completed):
                    native, clock = Mock(), self.CopyClock()
                    report = {"copy_failure": {"stale": True}}
                    native.clipboard.side_effect = [CLIPBOARD_SENTINEL] + [CLIPBOARD_SENTINEL] * completed + [original, "[@smoke000001]"]
                    native.snapshot.side_effect = [self.copy_snapshot(), self.copy_snapshot(visible=False, focused=False, error=True)]
                    output, errors = io.StringIO(), io.StringIO()
                    with patch(__name__ + ".time.monotonic", clock.monotonic), patch(__name__ + ".time.sleep", clock.sleep), \
                         patch("sys.stdout", output), patch("sys.stderr", errors), self.assertRaises(type(original)) as caught:
                        copy_with_diagnostics(native, report)
                    self.assertIs(caught.exception, original)
                    facts = report["copy_failure"]
                    self.assertEqual(set(facts), {"before_return", "clipboard", "after_timeout"})
                    self.assertTrue(facts["before_return"]["visible"])
                    self.assertFalse(facts["after_timeout"]["visible"])
                    self.assertFalse(facts["after_timeout"]["search_focused"])
                    self.assertTrue(facts["after_timeout"]["copy_error_present"])
                    self.assertEqual(facts["clipboard"], {"polls": completed, "expected": False,
                                     "sentinel": bool(completed), "empty": False, "elapsed_seconds": completed * 10})
                    self.assertEqual(native.clipboard.call_count, completed + 2, "Never read again after the failed poll")
                    self.assertEqual(native.snapshot.call_count, 2)
                    self.assertEqual(native.key.call_args_list, [call("Return")])
                    self.assertNotIn(private, output.getvalue() + errors.getvalue() + json.dumps(report))
                    self.assertIn("NATIVE COPY FAILURE ", errors.getvalue())

    def test_clipboard_read_error_survives_quiet_failed_optional_probes(self):
        private = "SYNTHETIC_PRIVATE_OPTIONAL_HELPER_PAYLOAD"
        native, report = Mock(), {}
        original = subprocess.CalledProcessError(7, [private], output=private, stderr=private)
        native.clipboard.side_effect = [CLIPBOARD_SENTINEL, original, "[@smoke000001]"]
        def broken_probe():
            print(private)
            print(private, file=sys.stderr)
            raise OSError(private)
        native.snapshot.side_effect = broken_probe
        key_window = Mock()
        key_window.side_effect = lambda: True if key_window.call_count == 1 else broken_probe()
        output, errors = io.StringIO(), io.StringIO()
        with patch("sys.stdout", output), patch("sys.stderr", errors), self.assertRaises(subprocess.CalledProcessError) as caught:
            copy_with_diagnostics(native, report, key_window)
        self.assertIs(caught.exception, original)
        self.assertEqual(report["copy_failure"]["before_return"], {"available": False})
        self.assertEqual(report["copy_failure"]["after_timeout"], {"available": False})
        self.assertIsNone(report["copy_failure"]["key_window_after_timeout"])
        self.assertEqual(key_window.call_count, 2)
        self.assertEqual(native.clipboard.call_count, 2)
        self.assertEqual(native.key.call_args_list, [call("Return")])
        self.assertNotIn(private, output.getvalue() + errors.getvalue() + json.dumps(report))

    def test_copy_keeps_success_from_a_poll_started_before_the_deadline(self):
        native, report, clock = Mock(), {}, self.CopyClock()
        native.snapshot.return_value = self.copy_snapshot()
        reads = iter(["Ibis native smoke clipboard sentinel", "[@smoke000001]"])
        def clipboard():
            value = next(reads)
            if value == "[@smoke000001]":
                clock.elapsed = 25  # The existing wait accepts this completed read.
            return value
        native.clipboard.side_effect = clipboard
        with patch(__name__ + ".time.monotonic", clock.monotonic), patch(__name__ + ".time.sleep", clock.sleep):
            copy_with_diagnostics(native, report)
        self.assertNotIn("copy_failure", report)
        self.assertEqual(native.key.call_args_list, [call("Return")])

    def test_optional_copy_observation_failure_does_not_fail_a_successful_copy(self):
        native, report = Mock(), {}
        native.clipboard.side_effect = ["Ibis native smoke clipboard sentinel", "[@smoke000001]"]
        native.snapshot.side_effect = RuntimeError("private helper failure")
        copy_with_diagnostics(native, report)
        self.assertNotIn("copy_failure", report)
        self.assertEqual(native.key.call_args_list, [call("Return")])

    def test_optional_copy_observations_are_quiet_and_preserve_the_original_failure(self):
        native, report = Mock(), {}
        private = "private clipboard/document/helper payload"
        def broken_snapshot():
            print(private)
            print(private, file=sys.stderr)
            raise RuntimeError(private)
        native.snapshot.side_effect = broken_snapshot
        native.clipboard.return_value = "Ibis native smoke clipboard sentinel"
        original = AssertionError("original copy wait failure")
        output, errors = io.StringIO(), io.StringIO()
        with patch(__name__ + ".wait_for", side_effect=original), patch("sys.stdout", output), patch("sys.stderr", errors), \
             self.assertRaises(AssertionError) as caught:
            copy_with_diagnostics(native, report)
        self.assertIs(caught.exception, original)
        self.assertEqual(report["copy_failure"]["before_return"], {"available": False})
        self.assertEqual(report["copy_failure"]["after_timeout"], {"available": False})
        self.assertNotIn(private, output.getvalue() + errors.getvalue() + json.dumps(report))
        self.assertEqual(native.key.call_args_list, [call("Return")])

    def test_copy_failure_facts_never_record_clipboard_or_accessible_payloads(self):
        for clipboard in ("", "private unexpected clipboard payload"):
            with self.subTest(empty=clipboard == ""):
                native, report, clock = Mock(), {}, self.CopyClock()
                native.clipboard.side_effect = ["Ibis native smoke clipboard sentinel", clipboard, clipboard]
                native.snapshot.return_value = {"visible": True, "nodes": [
                    {"name": "Search references", "role": "AXTextField", "value": "private query", "focused": True},
                    {"name": "private Ibis smoke reference 000001 suffix"},
                    {"name": "21 results"},
                    {"name": "private Nothing was copied. Try again, or use Reload. suffix"},
                ]}
                errors = io.StringIO()
                with patch(__name__ + ".time.monotonic", clock.monotonic), patch(__name__ + ".time.sleep", clock.sleep), \
                     patch("sys.stderr", errors), self.assertRaises(AssertionError):
                    copy_with_diagnostics(native, report)
                facts = report["copy_failure"]
                self.assertFalse(facts["clipboard"]["expected"])
                self.assertFalse(facts["clipboard"]["sentinel"])
                self.assertEqual(facts["clipboard"]["empty"], clipboard == "")
                for stage in ("before_return", "after_timeout"):
                    self.assertFalse(facts[stage]["exact_search_value"])
                    self.assertFalse(facts[stage]["title_present"])
                    self.assertFalse(facts[stage]["one_result"])
                    self.assertFalse(facts[stage]["copy_error_present"])
                    self.assertTrue(all(type(value) is bool for value in facts[stage].values()))
                self.assertNotIn("private", errors.getvalue() + json.dumps(report))

    class InitialPickerModel:
        def __init__(self, visible=True, search_ready=True, accepts_escape=True):
            self.visible, self.key_window = visible, visible
            self.search_ready, self.accepts_escape = search_ready, accepts_escape
            self.events = []

        def snapshot(self):
            return {"visible": self.visible, "nodes": [{
                "name": "Search references", "focused": self.search_ready,
            }]}

        def focus_editor(self):
            # The editor can already be frontmost while the panel owns keys.
            self.events.append(("editor", self.visible))

        def key(self, key):
            self.events.append((key, self.visible))
            if key == "Escape" and self.accepts_escape:
                self.visible = self.key_window = False
            elif key == "ctrl+super+r":
                self.visible = not (self.visible and self.key_window)
                self.key_window = self.visible

    def test_linux_picker_visibility_skips_the_copied_notice_window(self):
        linux = Linux.__new__(Linux)
        names = {"1": NOTICE_TITLE, "2": "Ibis"}
        def fake(*args):
            if args[1] == "search":
                return "1\n2"
            return names[args[2]]
        with patch(f"{__name__}.run", side_effect=fake):
            self.assertEqual(linux.window(42), "2")
        names["2"] = NOTICE_TITLE
        with patch(f"{__name__}.run", side_effect=fake):
            self.assertIsNone(linux.window(42))

    def test_initial_key_panel_requires_hidden_precondition_before_shortcut_show(self):
        old = self.InitialPickerModel()
        old.focus_editor()
        old.key("ctrl+super+r")
        self.assertFalse(old.visible, "Old frontmost-only sequence must reproduce toggle-hide")

        native = self.InitialPickerModel()
        with patch(__name__ + ".wait_for", side_effect=lambda predicate, description: self.assertTrue(predicate(), description)):
            prepare_initial_picker_show(native, native.snapshot)
        native.focus_editor()
        native.key("ctrl+super+r")
        self.assertEqual(native.events, [("Escape", True), ("editor", False), ("ctrl+super+r", False)])
        self.assertTrue(native.visible)

    def test_escape_clears_a_search_before_it_hides(self):
        state = {"visible": True, "value": "query"}
        native = Mock()
        def key(name):
            if state["value"]:
                state["value"] = ""
            else:
                state["visible"] = False
        native.key.side_effect = key
        snapshot = lambda: {"visible": state["visible"], "nodes": [{"name": "Search references", "value": state["value"]}]}
        escape_to_hide(native, snapshot, "hidden")
        self.assertEqual(native.key.call_args_list, [call("Escape"), call("Escape")])
        self.assertFalse(state["visible"])

    def test_escape_presses_again_when_a_cleared_search_is_reported_late(self):
        state = {"visible": True, "presses": 0}
        native = Mock()
        def key(name):
            state["presses"] += 1
            state["visible"] = state["presses"] < 2
        native.key.side_effect = key
        snapshot = lambda: {"visible": state["visible"], "nodes": [{"name": "Search references", "value": "stale"}]}
        with patch(__name__ + ".ESCAPE_STEP_TIMEOUT", 0.05):
            escape_to_hide(native, snapshot, "hidden")
        self.assertEqual(native.key.call_args_list, [call("Escape"), call("Escape")])

    def test_escape_that_never_hides_fails_after_one_spare_press(self):
        native = Mock()
        snapshot = lambda: {"visible": True, "nodes": [{"name": "Search references", "value": "private query"}]}
        with patch(__name__ + ".ESCAPE_STEP_TIMEOUT", 0.05), \
             patch(__name__ + ".wait_for", side_effect=lambda predicate, description: self.assertTrue(predicate(), description)), \
             patch("builtins.print") as printed, self.assertRaises(AssertionError):
            escape_to_hide(native, snapshot, "hidden")
        self.assertEqual(native.key.call_count, 3)
        self.assertNotIn("private query", printed.call_args.args[0])

    def test_hidden_initial_picker_does_not_send_escape_into_the_editor(self):
        native = self.InitialPickerModel(visible=False)
        prepare_initial_picker_show(native, native.snapshot)
        self.assertEqual(native.events, [])

    def test_visible_initial_picker_requires_named_search_before_escape(self):
        native = self.InitialPickerModel(search_ready=False)
        with patch(__name__ + ".wait_for", side_effect=lambda predicate, description: self.assertTrue(predicate(), description)):
            with self.assertRaisesRegex(AssertionError, "initial picker search ready"):
                prepare_initial_picker_show(native, native.snapshot)
        self.assertEqual(native.events, [])

    def test_initial_escape_must_actually_hide_before_editor_focus(self):
        native = self.InitialPickerModel(accepts_escape=False)
        with patch(__name__ + ".wait_for", side_effect=lambda predicate, description: self.assertTrue(predicate(), description)):
            with self.assertRaisesRegex(AssertionError, "initial Escape hides picker"):
                prepare_initial_picker_show(native, native.snapshot)
        self.assertEqual(native.events, [("Escape", True)])

    def test_typed_query_is_retyped_until_the_search_field_reads_back_exactly(self):
        values = iter(["smoke00001", "smoke000001"])
        native = Mock()
        snapshot = lambda: {"nodes": [{"name": "Search references", "role": "AXTextField", "value": next(values)}]}
        with patch(__name__ + ".time") as clock:
            clock.monotonic.side_effect = [0, 0, 10, 0, 0]
            type_query(native, snapshot, "smoke000001", "super+a")
        self.assertEqual(native.mock_calls, [call.key("super+a"), call.text("smoke000001")] * 2)

    def test_typed_query_fails_clearly_after_bounded_attempts(self):
        native = Mock()
        snapshot = lambda: {"nodes": [{"name": "Search references", "role": "AXTextField", "value": "smoke"}]}
        with patch(__name__ + ".time") as clock:
            clock.monotonic.side_effect = [0, 0, 10] * TYPE_ATTEMPTS
            with self.assertRaisesRegex(AssertionError, "did not hold the typed query after 3 attempts"):
                type_query(native, snapshot, "smoke000001", "super+a")
        self.assertEqual(native.text.call_count, TYPE_ATTEMPTS)

    def test_macos_editor_settle_closes_leftovers_and_reports_window_count(self):
        native = Scripted.__new__(Scripted)
        native.editor_path = Path("synthetic.txt")
        actions = []
        def helper(action, text=""):
            actions.append(action)
            return {"close-extra-editor-windows": 1, "editor-owned": False, "editor-state": {"window_count": 2}}[action]
        native.call = helper
        with patch(__name__ + ".wait_for", side_effect=AssertionError("Timed out")):
            with self.assertRaisesRegex(AssertionError, "single synthetic editor window is required; found 2"):
                native.settle_editor_windows()
        self.assertEqual(actions, ["close-extra-editor-windows", "editor-state"])

    def test_query_failure_facts_distinguish_empty_partial_and_exact_editable_values(self):
        for value, exact, empty in (("", False, True), ("priv", False, False), ("private query", True, False)):
            current = {"nodes": [
                {"name": "Search references", "role": "static text", "value": ""},
                {"name": "Search references", "role": "AXComboBox", "value": value},
                {"name": "private result", "value": "11 results"},
            ]}
            self.assertEqual(query_failure_facts(current, "private query", "private result"), {
                "exact_search_value": exact, "search_empty": empty,
                "expected_result": True, "one_result": False,
            })

    def test_failed_query_records_only_last_snapshot_booleans_and_static_description(self):
        states = iter([{"nodes": []}, {"nodes": [
            {"name": "Search references", "role": "AXTextField", "value": "private partial"},
            {"name": "1 result", "value": "private document"},
        ]}])
        report = {}
        def fail(predicate, description):
            self.assertFalse(predicate())
            self.assertFalse(predicate())
            raise AssertionError(f"Timed out: {description}")
        with patch(__name__ + ".wait_for", side_effect=fail), patch("sys.stderr") as stderr, patch("builtins.print") as output:
            with self.assertRaisesRegex(AssertionError, "^Timed out: native typed query$"):
                wait_for_query(lambda: next(states), "private query", "private result", report)
        self.assertEqual(report, {"query_failure": {
            "exact_search_value": False, "search_empty": False,
            "expected_result": False, "one_result": True,
        }})
        self.assertNotIn("private", json.dumps(report) + str(output.call_args))
        self.assertIs(output.call_args.kwargs["file"], stderr)

    def test_successful_query_does_not_emit_failure_facts(self):
        report = {}
        with patch(__name__ + ".wait_for", side_effect=lambda predicate, _description: self.assertTrue(predicate())), patch("builtins.print") as output:
            wait_for_query(lambda: {"nodes": [{"name": "expected private title"}]}, "private query", "expected private title", report)
        self.assertEqual(report, {})
        output.assert_not_called()

    def test_settled_query_requires_all_facts_in_one_observation(self):
        def state(value, title, count):
            return {"nodes": [
                {"name": "Search references", "role": "entry", "value": value},
                {"name": title}, {"name": count},
            ]}
        states = [state("private query", "private title", "3 results"),
                  state("old query", "private title", "1 result"),
                  state("private query", "old title", "1 result"),
                  state("private query", "private title", "1 result")]
        source = Mock(side_effect=states)
        def observe(predicate, _description):
            self.assertFalse(predicate())
            self.assertFalse(predicate())
            self.assertFalse(predicate())
            self.assertTrue(predicate())
        report = {}
        with patch(__name__ + ".wait_for", side_effect=observe):
            wait_for_query(source, "private query", "private title", report, settled=True)
        self.assertEqual(source.call_count, 4)
        self.assertEqual(report, {})

    def test_helper_timeout_emits_only_allowlisted_stages_from_bytes_stderr(self):
        native = Scripted.__new__(Scripted)
        native.system, native.command = "Windows", ["synthetic-helper", "private argument"]
        error = subprocess.TimeoutExpired(native.command, 30, stderr=(
            b'private path and payload\xff\nNATIVE HELPER STAGE windows-input-compile-start\n'
            b'NATIVE HELPER STAGE arbitrary-private-payload\n'))
        with patch("subprocess.run", side_effect=error), patch("sys.stderr") as stderr, patch("builtins.print") as output:
            with self.assertRaisesRegex(AssertionError, "timed out during probe after 30s"):
                native.call("probe")
        error_lines = [call.args[0] for call in output.call_args_list if call.kwargs.get("file") is stderr]
        self.assertEqual(error_lines, ["NATIVE HELPER STAGE windows-input-compile-start"])

    def test_editor_focus_diagnostics_preserve_only_fixed_boolean_ownership_facts(self):
        native = Scripted.__new__(Scripted)
        fields = ("system_events_editor_frontmost", "workspace_editor_frontmost", "workspace_ibis_frontmost",
                  "workspace_other_frontmost", "editor_running_application_active", "editor_window_visible")
        state = {key: key != "workspace_other_frontmost" for key in fields}
        state["private_path"] = "not allowed in evidence"
        with patch.object(native, "call", return_value=state):
            facts = editor_focus_facts(native, "copy-restoration-timeout")
            self.assertEqual(set(facts), set(fields) | {"stage"})
            state["editor_window_visible"] = "true"
            with self.assertRaisesRegex(RuntimeError, "metadata unavailable"):
                editor_focus_facts(native, "copy-restoration-timeout")

    def test_menu_actions_open_more_first(self):
        native = Mock()
        menu_action(native, "Reload")
        self.assertEqual(native.click.call_args_list, [call("More"), call("Reload")])
        with self.assertRaises(ValueError):
            menu_action(native, "Search references")

    class MenuKeyboardModel:
        """Native keys own the input; accessibility can focus its active option."""
        def __init__(self, platform="Linux", failure=None, delay=1):
            self.platform, self.failure, self.delay = platform, failure, delay
            self.original = self.value = "private synthetic query"
            self.menu, self.more, self.search = False, False, True
            self.escaped_reads = self.tab_reads = None
            self.pending, self.delivery_reads = None, 0
            self.selected = False
            self.events = []

        def snapshot(self):
            if self.escaped_reads is not None:
                self.escaped_reads += 1
                self.more = self.escaped_reads >= self.delay and self.failure != "invoker"
            if self.tab_reads is not None:
                self.tab_reads += 1
                self.search = self.tab_reads >= self.delay
            if self.pending is not None:
                self.delivery_reads += 1
                if self.delivery_reads >= self.delay:
                    self.value, self.pending = self.pending, None
            roles = {"Linux": ("combo box", "list item"),
                     "Windows": ("ControlType.ComboBox", "ControlType.ListItem"),
                     "Darwin": ("AXTextField", "AXRow")}
            search_role, result_role = roles[self.platform]
            current = {"visible": True, "foreground": True, "nodes": (
                [{"name": "Reload", "role": "menu item", "focused": True}] if self.menu else [
                    {"name": "More", "role": "button", "focused": self.more},
                    {"name": "Search references", "role": search_role, "value": self.value,
                     "focused": self.search and self.platform == "Darwin"},
                    {"name": "synthetic result", "role": result_role,
                     "focused": self.search and self.platform != "Darwin"}])}
            if self.tab_reads is not None:
                if self.failure in ("visible", "foreground"):
                    current[self.failure] = False
                if (self.failure == "missing-search" or (self.failure == "delayed-search" and self.tab_reads < self.delay)
                        or (self.failure == "delivery-tree" and self.pending is not None)):
                    current["nodes"] = [n for n in current["nodes"] if n["name"] != "Search references"]
            return current

        def key(self, chord):
            self.events.append(("key", chord))
            if chord in ("ctrl+k", "super+k"):
                self.menu, self.search = True, False
            elif chord == "Escape":
                self.menu, self.escaped_reads = False, 0
            elif chord == "space":
                self.menu = self.more and self.failure != "space"
                self.escaped_reads, self.more = None, False
                if not self.menu:
                    self.value += " "
            elif chord == "Tab":
                self.menu = self.failure == "menu"
                self.tab_reads = 0
                if self.failure == "query":
                    self.value += " "
            elif chord in ("ctrl+a", "super+a"):
                self.selected = self.search
                if self.failure == "after-select":
                    self.failure = "foreground"

        def text(self, text):
            self.events.append(("text", text))
            if not self.search or self.failure == "typing" or (self.failure == "restore" and text == self.original):
                return
            self.pending = text if self.selected else self.value + text
            self.delivery_reads = 0
            self.selected = False

    def menu_keyboard_check(self, native, report):
        def wait(predicate, description, *, timeout):
            self.assertEqual(timeout, 5)
            for _ in range(10):
                if predicate():
                    return
            raise AssertionError("Timed out: " + description)
        with patch(__name__ + ".wait_for", side_effect=wait):
            menu_escape_focus(native, native.snapshot, "super+k" if native.platform == "Darwin" else "ctrl+k", report)

    def test_menu_tab_proves_typing_with_virtualized_or_direct_accessibility_focus(self):
        for platform, query in (("Linux", "private synthetic query"), ("Windows", "synthetic 引用"),
                                ("Darwin", ""), ("Linux", "ibis tab probe")):
            with self.subTest(platform=platform, empty=not query):
                native, report = self.MenuKeyboardModel(platform, delay=5), {}
                native.original = native.value = query
                self.menu_keyboard_check(native, report)
                self.assertTrue(all(report["menu_escape_focus"].values()))
                self.assertEqual(native.value, native.original)
                self.assertGreaterEqual(native.tab_reads, 5)
                self.assertEqual(len([event for event in native.events if event[0] == "text"]), 2)
                self.assertTrue(all(type(value) is bool for value in report["menu_escape_focus"].values()))
                if native.original:
                    self.assertNotIn(native.original, json.dumps(report))

    def test_menu_keyboard_checks_reject_wrong_delivery_and_do_not_repair_changed_queries(self):
        for failure in ("invoker", "space", "query", "menu", "visible", "foreground", "after-select", "missing-search", "typing", "restore"):
            with self.subTest(failure=failure):
                native, report = self.MenuKeyboardModel(failure=failure), {}
                with self.assertRaises(AssertionError):
                    self.menu_keyboard_check(native, report)
                facts = report["menu_escape_focus"]
                self.assertFalse(all(facts.values()))
                if failure not in ("typing", "restore"):
                    self.assertFalse(any(event[0] == "text" for event in native.events))
                if failure == "typing":
                    self.assertFalse(facts["tab_search_typed"])
                if failure == "restore":
                    self.assertTrue(facts["tab_search_typed"])
                    self.assertFalse(facts["search_restored"])
                self.assertNotIn(native.original, json.dumps(report))

    def test_menu_tab_waits_for_the_search_tree_before_keyboard_delivery(self):
        for transition in ("delayed-search", "delivery-tree"):
            with self.subTest(transition=transition):
                native, report = self.MenuKeyboardModel(failure=transition, delay=5), {}
                self.menu_keyboard_check(native, report)
                self.assertTrue(all(report["menu_escape_focus"].values()))
                self.assertEqual(native.value, native.original)

    def test_global_macos_hid_event_contracts_without_native_execution(self):
        subprocess.run(["node", str(Path(__file__).with_name("smoke-macos.test.cjs"))], check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=20)

    def test_macos_click_reacquires_a_missing_control_and_stops_after_one_press(self):
        native = Scripted.__new__(Scripted)
        native.system = "Darwin"
        with patch.object(native, "call", side_effect=[{"pressed": False}, {"pressed": True}]) as helper, patch("time.sleep"):
            native.click("Reload")
        self.assertEqual(helper.call_args_list, [(('click', 'Reload'),), (('click', 'Reload'),)])

    def test_windows_click_requires_one_confirmed_activation_and_logs_only_fixed_booleans(self):
        native = Scripted.__new__(Scripted)
        native.system = "Windows"
        state = {"pressed": True, "button_role": True, "toggle_pattern": True, "private": "document payload"}
        with patch.object(native, "call", return_value=state) as helper, patch("builtins.print") as output:
            native.click("Typst")
        helper.assert_called_once_with("click", "Typst")
        self.assertNotIn("private", str(output.call_args))
        self.assertNotIn("payload", str(output.call_args))
        self.assertEqual(json.loads(output.call_args.args[0].removeprefix("NATIVE CONTROL STATE ")),
                         {"pressed": True, "button_role": True, "toggle_pattern": True})
        for invalid in (True, None, {"pressed": "true"}, {"pressed": True, "toggle_pattern": "true"}, {"pressed": False}):
            with self.subTest(invalid=invalid), patch.object(native, "call", return_value=invalid) as helper, \
                    patch("builtins.print"), self.assertRaises((AssertionError, RuntimeError)):
                native.click("Typst")
            helper.assert_called_once_with("click", "Typst")

    def windows_native(self):
        native = Scripted.__new__(Scripted)
        native.system = "Windows"
        native.app = Mock(pid=7)
        return native

    def test_windows_snapshot_rereads_only_a_tree_that_changed_while_being_read(self):
        native = self.windows_native()
        hidden = {"nodes": [], "visible": False, "foreground": False}
        with patch.object(native, "call", side_effect=[{"tree_changed": True}, hidden]) as helper, patch("time.sleep"):
            self.assertEqual(native.snapshot(), hidden)
        self.assertEqual(helper.call_args_list, [call("snapshot", "0"), call("snapshot", "0")])

    def test_windows_snapshot_fails_when_the_tree_never_settles_or_the_helper_fails(self):
        native = self.windows_native()
        clock = iter(range(0, 1000, 3))
        with patch.object(native, "call", return_value={"tree_changed": True}) as helper, patch("time.sleep"), \
                patch("time.monotonic", side_effect=lambda: next(clock)):
            with self.assertRaisesRegex(AssertionError, "^Timed out: native accessibility tree settles$"):
                native.snapshot()
        self.assertEqual(helper.call_count, 3)
        for failure in (subprocess.CalledProcessError(1, ["helper"]), {"tree_changed": False}, {"tree_changed": True, "nodes": []}):
            with patch.object(native, "call", side_effect=[failure]) as helper:
                with self.assertRaises((subprocess.CalledProcessError, RuntimeError)):
                    native.snapshot()
            self.assertEqual(helper.call_count, 1)

    def test_scripted_native_json_preserves_unicode_under_a_legacy_windows_locale(self):
        native = Scripted.__new__(Scripted)
        native.system = "Windows"
        expected = "synthetic 引用.bib"
        payload = json.dumps(expected, ensure_ascii=False).encode("utf-8").hex()
        native.command = [sys.executable, "-c", f"import sys; sys.stdout.buffer.write(bytes.fromhex('{payload}'))"]
        with patch("locale.getencoding", return_value="cp1252"), patch("builtins.print"):
            self.assertEqual(native.call("snapshot"), expected)

    def test_startup_waits_for_the_count_in_the_native_footer_tree(self):
        # The Linux tree as WebKitGTK reported it when the count sat in a plain
        # <span> beside the name: the status kept only the name (#531).
        name = "synthetic 引用.bib"
        lost = {"nodes": [{"role": "status bar", "name": "Library status", "value": ""},
                          {"role": "section", "name": "", "value": name}]}
        self.assertFalse(shows_count(lost, "3 references"))
        # The count in its own element, and the spoken detail in a paragraph.
        kept = {"nodes": lost["nodes"] + [
            {"role": "section", "name": "", "value": " · 3 references"},
            {"role": "paragraph", "name": "", "value": f"\u2068{name}\u2069 · 3 references. checked just now, 6:38 PM"}]}
        self.assertTrue(shows_count(kept, "3 references"))
        self.assertTrue(shows_count({"nodes": kept["nodes"][-1:]}, "3 references"))
        self.assertFalse(shows_count(kept, "4 references"))

    def test_missing_library_requires_both_recovery_message_and_retained_snapshot(self):
        name = "synthetic 引用.bib"
        message = (f"The library could not be opened: {name}. "
                   "the library file is unavailable; keeping the last loaded references. "
                   "Choose it again or reconnect the drive.")
        current = {"nodes": [{"name": message}, {"value": name}, {"value": " · not updated"},
                             {"value": f"\u2068{name}\u2069 · 4 references. checked just now, 12:00 PM · the file could not be read"}]}
        self.assertTrue(all(missing_library_evidence(current, name).values()))
        current["nodes"].pop()
        evidence = missing_library_evidence(current, name)
        self.assertFalse(evidence["retained_snapshot"])
        self.assertTrue(evidence["recovery_action"])
        self.assertTrue(all(type(value) is bool for value in evidence.values()))
        current["nodes"][0]["name"] = message[:100]
        self.assertFalse(missing_library_evidence(current, name)["recovery_action"])

    def test_missing_library_accepts_isolated_separator_nodes_without_losing_requirements(self):
        name = "synthetic 引用.bib"
        message = (f"The library could not be opened: {name}. "
                   "the library file is unavailable; keeping the last loaded references. "
                   "Choose it again or reconnect the drive.")
        current = {"nodes": [{"name": message}, {"value": name}, {"value": " · "},
                             {"value": "not updated"}, {"value": f"{name} · 4 references"}]}
        self.assertTrue(all(missing_library_evidence(current, name).values()))
        current["nodes"][3]["value"] = "no references"
        self.assertFalse(missing_library_evidence(current, name)["retained_snapshot"])
        current["nodes"][3]["value"] = "not updated"
        current["nodes"][-1]["value"] = f"{name} · 3 references"
        self.assertFalse(missing_library_evidence(current, name)["retained_snapshot"])
        current["nodes"][0]["name"] = message[:100]
        self.assertFalse(missing_library_evidence(current, name)["recovery_action"])

    BUDGETS = {"startup_ms": 100, "first_result_ms": 10, "memory_bytes": 2000}

    def test_windows_creation_identity_uses_original_handle_and_integer_microseconds(self):
        import ctypes
        app = Mock(_handle=987)
        app.poll.return_value = None
        ticks = 134369280000000019
        def get_times(handle, created, *_rest):
            self.assertEqual(handle, 987)
            created._obj.dwHighDateTime, created._obj.dwLowDateTime = ticks >> 32, ticks & 0xffffffff
            return True
        api = Mock(GetProcessTimes=Mock(side_effect=get_times))
        with patch.object(ctypes, "WinDLL", return_value=api, create=True):
            self.assertEqual(windows_creation_identity(app), (ticks + 504911232000000000) // 10)
            api.GetProcessTimes.return_value = False
            api.GetProcessTimes.side_effect = None
            with self.assertRaises(RuntimeError):
                windows_creation_identity(app)
            app.poll.return_value = 0
            api.GetProcessTimes.reset_mock()
            with self.assertRaises(RuntimeError):
                windows_creation_identity(app)
            api.GetProcessTimes.assert_not_called()

    def test_windows_memory_requires_pinned_identity_and_bounded_numeric_components(self):
        native = Scripted.__new__(Scripted)
        native.system, native.app = "Windows", Mock()
        facts = {"memory_bytes": 300, "excluded_older": 1, "components": [
            {"index": 0, "parent_index": 0, "depth": 0, "age_us": 0, "memory_bytes": 100},
            {"index": 1, "parent_index": 0, "depth": 1, "age_us": 20, "memory_bytes": 200}]}
        with patch(__name__ + ".windows_creation_identity", return_value=123), patch.object(native, "call", return_value=facts) as helper:
            self.assertEqual(native.memory(), 300)
            helper.assert_called_once_with("memory", "123")
            self.assertEqual(native.memory_facts, facts)
        for invalid in (300, {**facts, "private": "text"}, {**facts, "memory_bytes": 301},
                        {**facts, "excluded_older": True}, {**facts, "components": []},
                        {**facts, "components": facts["components"] * 33},
                        {**facts, "components": [{**facts["components"][0], "age_us": -1}]}):
            with patch(__name__ + ".windows_creation_identity", return_value=123), patch.object(native, "call", return_value=invalid):
                with self.assertRaises(RuntimeError):
                    native.memory()

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell is required to execute the Windows memory identity contract")
    def test_windows_memory_tree_excludes_orphans_and_rejects_identity_races(self):
        script = r'''
param([string] $Source)
$ErrorActionPreference='Stop'
$tokens=$null; $errors=$null
$ast=[System.Management.Automation.Language.Parser]::ParseFile($Source,[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw 'Helper syntax is invalid' }
foreach ($name in @('Get-MemoryIdentity','Get-Memory')) {
 $function=$ast.Find({param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name},$true)
 if ($function) { Invoke-Expression $function.Extent.Text }
}
$switch=$ast.Find({param($node) $node -is [System.Management.Automation.Language.SwitchStatementAst] -and $node.Condition.Extent.Text -eq '$Action'},$true)
$clause=@($switch.Clauses | Where-Object {$_.Item1.Value -eq 'memory'})[0].Item2.Extent.Text
$operation=[scriptblock]::Create($clause.Substring(1,$clause.Length-2))
$created=[DateTime]::new(2026,10,19,0,0,0,[DateTimeKind]::Utc)
$AppId=10; $Text='63927964800000000'
function Row($Id,$Parent,$Date) { [pscustomobject]@{ProcessId=$Id;ParentProcessId=$Parent;CreationDate=$Date} }
function Get-CimInstance {
 [CmdletBinding()] param($ClassName)
 $script:enumerations++
 if ($script:replaceAfter -and $script:enumerations -gt 1) { return @((Row 10 1 $created.AddSeconds(1))) }
 $script:rows
}
function Get-Process {
 [CmdletBinding()] param($Id)
 foreach ($number in $Id) {
  $row=$script:rows | Where-Object {$_.ProcessId -eq $number} | Select-Object -First 1
  $date=$row.CreationDate
  $script:reads++
  if ($script:race -eq $number -and (!$script:afterRead -or $script:reads -gt 1)) { $date=$created.AddSeconds(10) }
  [pscustomobject]@{Id=$number;StartTime=$date;WorkingSet64=100L}
 }
}
$initial=@((Row 10 1 $created),(Row 11 10 $created.AddSeconds(-1)),
 (Row 12 10 $created.AddSeconds(1)),(Row 13 12 $created.AddSeconds(2)),
 (Row 14 12 $created),(Row 15 11 $created.AddSeconds(3)),(Row 16 10 $created),(Row 99 98 $null))
$script:rows=@($initial[7..0]); $script:race=0
$result=& $operation
if ($result.memory_bytes -ne 400 -or $result.components.Count -ne 4 -or $result.excluded_older -ne 2) {
 throw 'Memory attributed an older orphan or lost a genuine descendant'
}
if (($result.components | Measure-Object memory_bytes -Sum).Sum -ne 400 -or
    @($result.components | Where-Object {$_.depth -eq 2}).Count -ne 1) { throw 'Component facts do not explain the sum' }
function Reject {
 $rejected=$false
 try { & $operation | Out-Null } catch { $rejected=$true }
 if (!$rejected) { throw 'Unverified identity produced a memory verdict' }
}
foreach ($invalid in @($null,'2026-10-19',[DateTime]::MinValue)) {
 $script:rows=@((Row 10 1 $invalid)); Reject
 $script:rows=@((Row 10 1 $created),(Row 12 10 $invalid)); Reject
}
$script:rows=@((Row 10 1 $created.AddSeconds(1))); Reject
$script:rows=@(); Reject
$script:rows=@((Row 10 1 $created),(Row 10 1 $created)); Reject
foreach ($number in @(10,12)) { $script:rows=$initial; $script:race=$number; Reject }
$script:rows=$initial; $script:race=10; $script:afterRead=$true; $script:reads=0; Reject
$script:afterRead=$false; $script:race=0
$script:replaceAfter=$true; $script:enumerations=0; Reject
$script:replaceAfter=$false
$script:race=0
# Identity truncation must match CIM's microsecond precision without floating point.
$precise=$created.AddTicks(19); $script:rows=@((Row 10 1 $created.AddTicks(10)))
$Text=(Get-MemoryIdentity $precise).ToString()
$result=& $operation
if ($result.memory_bytes -ne 100) { throw 'Creation timestamp precision mismatch' }
# Huge trees refuse a result rather than returning the first 64 components.
$Text='63927964800000000'; $script:rows=@((Row 10 1 $created))
foreach ($number in 100..163) { $script:rows += Row $number 10 $created }
Reject
# A failed/missing working-set read cannot silently subtract a process.
$script:rows=@((Row 10 1 $created))
function Get-Process { [CmdletBinding()] param($Id) @() }
Reject
'''
        with tempfile.TemporaryDirectory() as temporary:
            test = Path(temporary) / "memory-contract.ps1"
            test.write_text(script, encoding="utf-8")
            result = subprocess.run([shutil.which("pwsh"), "-NoProfile", "-File", str(test),
                                     "-Source", str(Path(__file__).with_name("smoke-windows.ps1"))],
                                    capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


    @staticmethod
    def perf_samples(warmup, *startups, first=5, memory=1000):
        return [{"warmup": True, "startup_ms": warmup, "first_result_ms": first, "memory_bytes": memory}] + [
            {"warmup": False, "startup_ms": startup, "first_result_ms": first, "memory_bytes": memory} for startup in startups]

    def test_performance_budget_gates_the_median_and_ignores_the_warm_up(self):
        performance = summarize_performance(self.perf_samples(140, 90, 140, 60))
        enforce_performance_budgets(performance, self.BUDGETS)
        self.assertEqual(performance["status"], "passed")
        self.assertEqual((performance["startup_ms"], performance["sample_count"], performance["warmup_count"]), (90, 3, 1))
        self.assertEqual(performance["maxima"]["startup_ms"], 140)
        self.assertEqual(performance["hard_ceilings"], {"startup_ms": 150.0, "first_result_ms": 15.0})

    def test_performance_budget_fails_any_launch_above_the_hard_ceiling_warm_up_included(self):
        for samples in (self.perf_samples(151, 50, 50, 50), self.perf_samples(50, 50, 151, 50)):
            performance = summarize_performance(samples)
            with self.assertRaisesRegex(AssertionError, "single-launch startup_ms"):
                enforce_performance_budgets(performance, self.BUDGETS)
            self.assertEqual((performance["status"], performance["failed_metric"]), ("failed", "startup_ms"))
        performance = summarize_performance(self.perf_samples(50, 50, 50, 50, first=5) + [
            {"warmup": False, "startup_ms": 50, "first_result_ms": 16, "memory_bytes": 1000}])
        with self.assertRaisesRegex(AssertionError, "single-launch first_result_ms"):
            enforce_performance_budgets(performance, self.BUDGETS)

    def test_performance_budget_still_fails_a_real_regression_where_every_launch_is_slow(self):
        performance = summarize_performance(self.perf_samples(130, 110, 120, 105))
        self.assertTrue(all(sample["startup_ms"] <= 150 for sample in performance["samples"]))
        with self.assertRaisesRegex(AssertionError, "median startup_ms"):
            enforce_performance_budgets(performance, self.BUDGETS)
        self.assertEqual((performance["status"], performance["failed_metric"]), ("failed", "startup_ms"))

    def test_performance_memory_uses_the_maximum_of_every_launch(self):
        samples = self.perf_samples(50, 50, 50, 50)
        samples[0]["memory_bytes"] = 2001
        performance = summarize_performance(samples)
        self.assertEqual(performance["memory_bytes"], 2001)
        with self.assertRaisesRegex(AssertionError, "maximum memory_bytes"):
            enforce_performance_budgets(performance, self.BUDGETS)

    def test_performance_summary_requires_a_measured_launch(self):
        with self.assertRaises(ValueError):
            summarize_performance(self.perf_samples(50))

    def test_phase_facts_reject_private_extra_fields_and_nonfinite_or_negative_numbers(self):
        valid = dict.fromkeys(PROBE_PHASE_KEYS, 1)
        self.assertEqual(numeric_phase_facts(valid, PROBE_PHASE_KEYS), valid)
        for bad in (True, -1, float('inf'), float('nan'), 10**1000, "private"):
            self.assertIsNone(numeric_phase_facts(dict(valid, tree_ms=bad), PROBE_PHASE_KEYS))
        self.assertIsNone(numeric_phase_facts(dict(valid, path="private"), PROBE_PHASE_KEYS))
        self.assertIsNone(numeric_phase_facts({}, PROBE_PHASE_KEYS))

    def test_library_phase_logs_are_fixed_numeric_and_bounded(self):
        valid = dict.fromkeys(LIBRARY_PHASE_KEYS, 0)
        valid.update(load_ms=12.5, accepted=1, load_ok=1)
        with tempfile.TemporaryDirectory() as folder:
            log = Path(folder) / "synthetic.log"
            log.write_text("private error\nibis-library-phases: " + json.dumps(dict(valid, path="private")) +
                           "\nibis-library-phases: not-json\n" +
                           ("ibis-library-phases: " + json.dumps(valid) + "\n") * 100, encoding="utf-8")
            self.assertEqual(library_phase_diagnostics([log]), [valid] * 64)
            for accepted, load_ok in ((0, 0), (0, 1), (1, 0)):
                valid.update(accepted=accepted, load_ok=load_ok)
                log.write_text("ibis-library-phases: " + json.dumps(valid), encoding="utf-8")
                self.assertEqual(library_phase_diagnostics([log]), [valid])

    def test_unavailable_phase_logs_never_replace_valid_diagnostics(self):
        valid = dict.fromkeys(LIBRARY_PHASE_KEYS, 0)
        line = "ibis-library-phases: " + json.dumps(valid)
        with tempfile.TemporaryDirectory() as folder:
            log, missing = Path(folder) / "synthetic.log", Path(folder) / "missing.log"
            log.write_text(line, encoding="utf-8")
            self.assertEqual(library_phase_diagnostics([missing, log, missing]), [valid])
            with patch.object(Path, "open", side_effect=[PermissionError(), io.StringIO(line)]):
                self.assertEqual(library_phase_diagnostics([missing, log]), [valid])

    def test_nested_phase_json_is_discarded_before_a_valid_record(self):
        valid = dict.fromkeys(LIBRARY_PHASE_KEYS, 0)
        line = "ibis-library-phases: " + json.dumps(valid)
        with tempfile.TemporaryDirectory() as folder:
            log = Path(folder) / "synthetic.log"
            log.write_text("ibis-library-phases: " + "[" * 1000 + "0" + "]" * 1000 + "\n" + line,
                           encoding="utf-8")
            self.assertEqual(library_phase_diagnostics([log]), [valid])

    def test_startup_phases_aggregate_only_fixed_probe_fields(self):
        phases = {}
        current = {"nodes": [{"name": "100,000 references"}],
                   "probe_phases": dict.fromkeys(PROBE_PHASE_KEYS, 2)}
        with patch("time.monotonic", return_value=1):
            measure_startup(lambda: None, lambda: current, phases)
        self.assertEqual(phases["probe_samples"], 1)
        self.assertEqual(phases["probe_phases"], current["probe_phases"])
        phases = {}
        snapshots = iter([dict(current, nodes=[]), current])
        with patch("time.monotonic", return_value=1), patch("time.sleep"):
            measure_startup(lambda: None, lambda: next(snapshots), phases)
        self.assertEqual(phases["snapshot_calls"], 2)
        self.assertEqual(phases["probe_samples"], 2)
        self.assertEqual(phases["probe_phases"], dict.fromkeys(PROBE_PHASE_KEYS, 4))
        phases = {}
        current["probe_phases"]["private"] = "never copied"
        with patch("time.monotonic", return_value=1):
            measure_startup(lambda: None, lambda: current, phases)
        self.assertNotIn("probe_phases", phases)
        self.assertNotIn("probe_samples", phases)

    def test_startup_phases_separate_launch_and_native_probes_without_extra_reads(self):
        clock, phases = self.CopyClock(), {}
        def launch():
            clock.elapsed += 2
        counts = iter(("loading", "100,000 references"))
        def snapshot():
            clock.elapsed += 3
            return {"nodes": [{"name": next(counts)}]}
        with patch(__name__ + ".time.monotonic", clock.monotonic), \
             patch(__name__ + ".time.sleep", lambda interval: setattr(clock, "elapsed", clock.elapsed + interval)):
            startup = measure_startup(launch, snapshot, phases)
        self.assertAlmostEqual(startup, 8100)
        self.assertEqual(phases, {"launch_ms": 2000, "readiness_ms": 6100,
                                 "snapshot_calls": 2, "snapshot_ms": 6000, "snapshot_max_ms": 3000})
        self.assertTrue(all(type(value) in (int, float) for value in phases.values()))

    def test_startup_phases_keep_failed_probe_duration_and_original_error(self):
        clock, phases = self.CopyClock(), {}
        original = OSError("SYNTHETIC_PRIVATE_SNAPSHOT_TEXT")
        def snapshot():
            clock.elapsed += 4
            raise original
        with patch(__name__ + ".time.monotonic", clock.monotonic), self.assertRaises(OSError) as caught:
            measure_startup(lambda: None, snapshot, phases)
        self.assertIs(caught.exception, original)
        self.assertEqual(phases, {"launch_ms": 0, "readiness_ms": 4000,
                                 "snapshot_calls": 1, "snapshot_ms": 4000, "snapshot_max_ms": 4000})
        self.assertNotIn("SYNTHETIC_PRIVATE", json.dumps(phases))

    def test_startup_phases_keep_the_existing_readiness_deadline(self):
        clock, phases = self.CopyClock(), {}
        def snapshot():
            clock.elapsed += 61
            return {"nodes": []}
        with patch(__name__ + ".time.monotonic", clock.monotonic), \
             patch(__name__ + ".time.sleep", lambda interval: setattr(clock, "elapsed", clock.elapsed + interval)), \
             self.assertRaisesRegex(AssertionError, "Timed out: 100,000-reference library load"):
            measure_startup(lambda: None, snapshot, phases)
        self.assertEqual(phases["snapshot_calls"], 2)
        self.assertEqual(phases["snapshot_ms"], 122000)
        self.assertEqual(phases["readiness_ms"], 122200)

    def test_macos_waits_for_new_editor_process_registration(self):
        registrations = iter((0, 12345))
        def helper(native, action, text=""):
            return next(registrations) if action == "editor" else action != "editor-running"

        with patch.object(Scripted, "call", helper), patch("subprocess.run"), patch("os.kill"), patch("time.sleep"):
            native = Scripted("Darwin")
            self.assertEqual(native.editor_pid, 12345)
            native.close()

    def test_macos_editor_launch_uses_physical_path_for_axdocument_ownership(self):
        with tempfile.TemporaryDirectory() as temporary:
            actual = Path(temporary) / "physical"
            actual.mkdir()
            alias = Path(temporary) / "alias"
            alias.symlink_to(actual, target_is_directory=True)
            class Folder:
                name = str(alias)
                def cleanup(self):
                    pass
            def helper(native, action, text=""):
                return 12345 if action == "editor" else action != "editor-running"
            with patch("tempfile.TemporaryDirectory", return_value=Folder()), patch.object(Scripted, "call", helper), patch("subprocess.run"), patch("os.kill"):
                native = Scripted("Darwin")
                try:
                    self.assertEqual(native.editor_path.parent, actual.resolve())
                finally:
                    native.close()

    def test_toggle_diagnostics_accept_only_exact_fixed_lines_from_current_run_logs(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "synthetic.log"
            log.write_text("\n".join(("private-synthetic-library-text", "ibis-picker: toggle visible focused hide",
                                      "ibis-picker: toggle visible unfocused show private-synthetic-detail")), encoding="utf-8")
            self.assertEqual(fixed_toggle_diagnostics([log]), ["ibis-picker: toggle visible focused hide"])

    def test_windows_snapshots_reuse_verified_main_hwnd_for_hide_assertions(self):
        native = Scripted.__new__(Scripted)
        native.system = "Windows"
        native.app = type("App", (), {"pid": 123})()
        visible = {"picker_handle": 456, "visible": True}
        hidden = {"picker_handle": 456, "visible": False}
        with patch.object(native, "call", side_effect=[visible, hidden]) as helper:
            native.snapshot()
            native.snapshot()
            self.assertEqual(helper.call_args_list[0].args, ("snapshot", "0"))
            self.assertEqual(helper.call_args_list[1].args, ("snapshot", "456"))

    def test_windows_click_reuses_verified_picker_when_a_modal_menu_hides_search(self):
        native = Scripted.__new__(Scripted)
        native.system, native.command = "Windows", ["synthetic-helper"]
        native.app = type("App", (), {"pid": 123})()
        native.picker_app_pid, native.picker_handle = 123, 456
        result = subprocess.CompletedProcess([], 0, '{"pressed":true}', "")
        for handle in (456, -2147483647):
            with self.subTest(verified_handle=handle), patch("subprocess.run", return_value=result) as helper:
                native.picker_handle = handle
                native.call("click", "Reload")
                self.assertEqual(helper.call_args.args[0], ["synthetic-helper", "click", "123", "0", "Reload", str(handle)])
        for owner, handle in ((124, 456), (123, 0), (123, True), (123, 1.5)):
            with self.subTest(owner=owner, handle=handle), patch("subprocess.run") as helper:
                native.picker_app_pid, native.picker_handle = owner, handle
                with self.assertRaisesRegex(RuntimeError, "identity"):
                    native.call("click", "Reload")
                helper.assert_not_called()

    def test_windows_snapshots_fail_closed_if_the_verified_picker_identity_changes(self):
        native = Scripted.__new__(Scripted)
        native.system, native.picker_app_pid, native.picker_handle = "Windows", 123, 456
        native.app = type("App", (), {"pid": 123})()
        for snapshot in ({"picker_handle": 789, "visible": True}, {"visible": False}):
            with self.subTest(snapshot=snapshot), patch.object(native, "call", return_value=snapshot):
                with self.assertRaisesRegex(RuntimeError, "identity"):
                    native.snapshot()

    def test_macos_owned_editor_launch_has_no_textedit_apple_events_and_cleans_only_verified_pid(self):
        def helper(native, action, text=""):
            if action == "editor":
                return 12345
            if action == "editor-running":
                return False
            if action == "editor-owned":
                self.assertEqual(Path(text).read_text(encoding="utf-8"), "")
            return True

        def launch(command, **kwargs):
            self.assertEqual(command[:4], ["open", "-n", "-a", "TextEdit"])
            self.assertEqual(command[5:], ["--args", "-ApplePersistenceIgnoreState", "YES"])
            self.assertEqual(Path(command[4]).name, "Ibis synthetic editor.txt")
            self.assertTrue(Path(command[4]).is_file())

        with patch.object(Scripted, "call", helper), patch("subprocess.run", side_effect=launch), patch("os.kill") as terminate:
            native = Scripted("Darwin")
            owned = native.editor_path
            native.close()
            terminate.assert_called_once_with(12345, signal.SIGTERM)
            self.assertFalse(owned.exists())

    def test_macos_existing_editor_refusal_never_launches_or_terminates_a_process(self):
        def helper(native, action, text=""):
            if action == "editor-preflight":
                raise subprocess.CalledProcessError(1, ["osascript"], stderr="Existing TextEdit process")
            return True

        with patch.object(Scripted, "call", helper), patch("subprocess.run") as launch, patch("os.kill") as terminate:
            with self.assertRaisesRegex(RuntimeError, "preflight failed"):
                Scripted("Darwin")
            launch.assert_not_called()
            terminate.assert_not_called()

    def test_macos_cleanup_refuses_an_unverified_editor_identity(self):
        native = Scripted.__new__(Scripted)
        native.system, native.editor_pid = "Darwin", 12345
        native.editor_folder = tempfile.TemporaryDirectory(prefix="ibis editor cleanup test ")
        native.editor_path = Path(native.editor_folder.name) / "owned.txt"
        native.editor_path.write_text("", encoding="utf-8")
        with patch.object(native, "call", return_value=False), patch("os.kill") as terminate:
            with self.assertRaisesRegex(RuntimeError, "ownership"):
                native.close()
            terminate.assert_not_called()
            self.assertFalse(native.editor_path.exists())

    def test_macos_cleanup_waits_for_owned_registration_to_exit_without_repeated_termination(self):
        native = Scripted.__new__(Scripted)
        native.system, native.editor_pid = "Darwin", 12345
        native.editor_folder = tempfile.TemporaryDirectory(prefix="ibis editor exit test ")
        native.editor_path = Path(native.editor_folder.name) / "owned.txt"
        native.editor_path.write_text("", encoding="utf-8")
        running = iter((True, True, False))
        def helper(action, text=""):
            return next(running) if action == "editor-running" else True
        with patch.object(native, "call", side_effect=helper) as calls, patch("os.kill") as terminate, patch("time.sleep"):
            native.close()
            terminate.assert_called_once_with(12345, signal.SIGTERM)
            self.assertEqual(sum(call.args[0] == "editor-running" for call in calls.call_args_list), 3)
            self.assertFalse(native.editor_path.exists())

    def test_helper_timeout_identifies_action_without_printing_command_arguments(self):
        native = Scripted.__new__(Scripted)
        native.system = "Darwin"
        native.command = ["osascript", "private-synthetic-helper-path"]
        with patch("subprocess.run", side_effect=subprocess.TimeoutExpired(native.command + ["private-synthetic-query"], 30)), patch("builtins.print"):
            with self.assertRaisesRegex(AssertionError, r"^Native helper timed out during snapshot after 30s$"):
                native.call("snapshot", "private-synthetic-query")

    def test_windows_editor_operations_require_the_original_process_creation_identity(self):
        native = Scripted.__new__(Scripted)
        native.system, native.command = "Windows", ["pwsh"]
        native.editor_pid, native.editor_started = 12345, 987654321
        metadata = {"owned": True, "window_valid": True, "request_accepted": False, "current_foreground": True}
        results = [subprocess.CompletedProcess(["pwsh"], 0, stdout=json.dumps(metadata), stderr="")]
        results += [subprocess.CompletedProcess(["pwsh"], 0, stdout="true", stderr="")] * 3
        with patch("subprocess.run", side_effect=results) as helper, patch("builtins.print"):
            native.focus_editor()
            native.editor_focused()
            native.close()
            self.assertEqual([call.args[0][-1] for call in helper.call_args_list], ["987654321"] * 4)

    def test_windows_focus_request_requires_fresh_focus_even_when_request_or_snapshot_says_yes(self):
        for accepted, foreground in ((False, False), (False, True), (True, True)):
            native = Scripted.__new__(Scripted)
            native.system = "Windows"
            metadata = {"owned": True, "window_valid": True, "request_accepted": accepted, "current_foreground": foreground}
            actions = []
            def helper(action):
                actions.append(action)
                return metadata if action == "focus-editor" else False
            def no_focus(predicate, description):
                self.assertFalse(predicate())
                raise AssertionError("Actual editor focus did not arrive")
            with patch.object(native, "call", side_effect=helper), patch(__name__ + ".wait_for", side_effect=no_focus):
                with self.assertRaisesRegex(AssertionError, "Actual editor focus did not arrive"):
                    native.focus_editor()
                    actions.extend(["timer", "launch"])
            self.assertEqual(actions, ["focus-editor", "editor-focused"])

    def test_windows_refused_foreground_request_can_only_finish_with_fresh_actual_focus(self):
        native = Scripted.__new__(Scripted)
        native.system = "Windows"
        actions = []
        metadata = {"owned": True, "window_valid": True, "request_accepted": False, "current_foreground": False}
        observations = iter([False, True])
        def helper(action):
            actions.append(action)
            return metadata if action == "focus-editor" else next(observations)
        def focused(predicate, description):
            self.assertFalse(predicate())
            self.assertTrue(predicate())
        with patch.object(native, "call", side_effect=helper), patch(__name__ + ".wait_for", side_effect=focused):
            native.focus_editor()
            actions.extend(["timer", "launch"])
        self.assertEqual(actions, ["focus-editor", "editor-focused", "editor-focused", "timer", "launch"])

    def test_windows_focus_metadata_rejects_bad_identity_window_or_schema_before_waiting(self):
        valid = {"owned": True, "window_valid": True, "request_accepted": False, "current_foreground": True}
        invalid = [True, None, {**valid, "owned": False}, {**valid, "window_valid": False},
                   {**valid, "request_accepted": 1}, {**valid, "current_foreground": 1},
                   {**valid, "payload": "private"}, {key: value for key, value in valid.items() if key != "owned"}]
        native = Scripted.__new__(Scripted)
        native.system = "Windows"
        for metadata in invalid:
            with patch.object(native, "call", return_value=metadata), patch(__name__ + ".wait_for") as wait, patch("builtins.print") as diagnostic:
                with self.assertRaises((AssertionError, RuntimeError)):
                    native.focus_editor()
                wait.assert_not_called()
                diagnostic.assert_not_called()

    def test_macos_editor_focus_behavior_is_unchanged(self):
        native = Scripted.__new__(Scripted)
        native.system = "Darwin"
        with patch.object(native, "call", return_value=True) as call, patch(__name__ + ".wait_for") as wait:
            native.focus_editor()
            call.assert_called_once_with("focus-editor")
            wait.assert_not_called()

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell is required to execute the Windows focus helper contract")
    def test_windows_helper_validates_owned_window_before_a_foreground_request_and_one_unlocked_retry(self):
        script = r'''
param([string] $Source)
$ErrorActionPreference = 'Stop'
$tokens=$null; $errors=$null
$ast=[System.Management.Automation.Language.Parser]::ParseFile($Source,[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw 'Helper syntax is invalid' }
$getter=$ast.Find({param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Get-OwnedEditor'},$true)
Invoke-Expression $getter.Extent.Text
$switch=$ast.Find({param($node) $node -is [System.Management.Automation.Language.SwitchStatementAst] -and $node.Condition.Extent.Text -eq '$Action'},$true)
$clause=@($switch.Clauses | Where-Object {$_.Item1.Value -eq 'focus-editor'})[0].Item2.Extent.Text
$operation=[scriptblock]::Create($clause.Substring(1,$clause.Length-2))
Add-Type @'
using System;
public static class IbisSmoke {
 public static bool Live=true, Accepted=false;
 public static uint Owner=12345, Foreground=0;
 public static int Requests=0, Keys=0;
 public static void Key(ushort vk, ushort scan, uint flags) { if (vk != 0x12) throw new Exception("unexpected key"); Keys++; }
 public static bool IsWindow(IntPtr window) { return Live; }
 public static uint OwnerPid(IntPtr window) { return Owner; }
 public static uint ForegroundPid() { return Foreground; }
 public static bool SetForegroundWindow(IntPtr window) { Requests++; return Accepted; }
}
'@
$EditorId=12345; $Text='987654321'
$script:current=[pscustomobject]@{ProcessName='Notepad';StartTime=[DateTime]::new(987654321,[DateTimeKind]::Utc);MainWindowHandle=[IntPtr]::new(20)}
function Get-Process { param($Id,$ErrorAction) $script:current }
foreach ($accepted in @($false,$true)) {
 [IbisSmoke]::Requests=0; [IbisSmoke]::Keys=0; [IbisSmoke]::Accepted=$accepted
 $result=& $operation
 # A refused request gets one retry after an Alt press and an Alt release.
 $requests=if ($accepted) {1} else {2}; $keys=if ($accepted) {0} else {2}
 if ($result.Count -ne 4 -or !$result.owned -or !$result.window_valid -or
     $result.request_accepted -ne $accepted -or $result.current_foreground -or
     [IbisSmoke]::Requests -ne $requests -or [IbisSmoke]::Keys -ne $keys) { throw 'Request metadata regression' }
}
[IbisSmoke]::Foreground=12345; [IbisSmoke]::Requests=0; [IbisSmoke]::Keys=0
$result=& $operation
if (!$result.current_foreground -or [IbisSmoke]::Requests -ne 0 -or [IbisSmoke]::Keys -ne 0) { throw 'Already focused editor was requested again' }
foreach ($failure in @('missing','identity','zero','invalid','other-owner')) {
 [IbisSmoke]::Requests=0; [IbisSmoke]::Live=$true; [IbisSmoke]::Owner=12345
 $script:current=[pscustomobject]@{ProcessName='Notepad';StartTime=[DateTime]::new(987654321,[DateTimeKind]::Utc);MainWindowHandle=[IntPtr]::new(20)}
 switch ($failure) {
  'missing' {$script:current=$null}
  'identity' {$script:current.StartTime=[DateTime]::new(987654322,[DateTimeKind]::Utc)}
  'zero' {$script:current.MainWindowHandle=[IntPtr]::Zero}
  'invalid' {[IbisSmoke]::Live=$false}
  'other-owner' {[IbisSmoke]::Owner=54321}
 }
 $rejected=$false
 try { & $operation | Out-Null } catch {$rejected=$true}
 if (!$rejected -or [IbisSmoke]::Requests -ne 0 -or [IbisSmoke]::Keys -ne 0) { throw 'Unowned or invalid editor reached a foreground request' }
}
'''
        with tempfile.TemporaryDirectory() as temporary:
            test = Path(temporary) / "focus-contract.ps1"
            test.write_text(script, encoding="utf-8")
            result = subprocess.run([shutil.which("pwsh"), "-NoProfile", "-File", str(test),
                                     "-Source", str(Path(__file__).with_name("smoke-windows.ps1"))],
                                    capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        facts = [json.loads(line.removeprefix("NATIVE EDITOR FOCUS ")) for line in result.stderr.splitlines()]
        self.assertTrue(facts)
        for value in facts:
            self.assertEqual(set(value), {"stage", "owned", "window_valid", "request_accepted", "current_foreground"})
            self.assertIn(value.pop("stage"), ("identity", "window", "foreground-request"))
            self.assertTrue(all(type(item) is bool for item in value.values()))

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell is required to execute the snapshot failure contract")
    def test_windows_snapshot_fatal_phases_and_error_kinds_do_not_expose_payloads(self):
        script = r'''
param([string] $Source, [string] $Scenario)
$ErrorActionPreference='Stop'
$tokens=$null; $errors=$null
$ast=[System.Management.Automation.Language.Parser]::ParseFile($Source,[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw 'Helper syntax is invalid' }
foreach ($name in @('Get-AppWindow','Get-Tree','Get-ErrorKind')) {
 $function=$ast.Find({param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name},$true)
 Invoke-Expression $function.Extent.Text.Replace('System.Windows.Automation.', 'IbisSnapshotContract.')
}
$outer=@($ast.EndBlock.Statements | Where-Object {$_ -is [System.Management.Automation.Language.TryStatementAst]})[0]
$body=$outer.CatchClauses[0].Body.Extent.Text
$fatal=[scriptblock]::Create($body.Substring(1,$body.Length-2))
Add-Type @'
using System;
namespace IbisSnapshotContract {
 public class ElementNotAvailableException : Exception {}
 public enum TreeScope { Children, Descendants }
 public class PropertyCondition { public PropertyCondition(object property, object value) {} }
 public static class Condition { public static object TrueCondition=new object(); }
 public static class ValuePattern { public static object Pattern=new object(); }
 public static class TextPattern { public static object Pattern=new object(); }
 public static class AutomationElement {
  public static object ProcessIdProperty=new object(), NameProperty=new object();
  public static SnapshotRoot RootElement=new SnapshotRoot();
  public static object FromHandle(IntPtr handle) {
   SnapshotFault.Check("from-handle");
   if (SnapshotFault.Scenario=="provider") throw new InvalidOperationException(SnapshotFault.Marker);
   return new SnapshotWindow();
  }
 }
}
public static class SnapshotFault {
 public static string Scenario, Marker="injected-private-clipboard-引用";
 public static System.Collections.Generic.List<string> Reads=new System.Collections.Generic.List<string>();
 public static void Check(string operation) {
  Reads.Add(operation);
  if (Scenario==operation) throw new InvalidOperationException(Marker);
 }
}
public class SnapshotRoot {
 public object[] FindAll(IbisSnapshotContract.TreeScope scope, IbisSnapshotContract.PropertyCondition condition) {
  if (SnapshotFault.Scenario=="window-bound") return new object[21];
  throw new InvalidOperationException(SnapshotFault.Marker);
 }
}
public class SnapshotWindow {
 public SnapshotWindowInfo Current { get { return new SnapshotWindowInfo(); } }
 public object[] FindAll(IbisSnapshotContract.TreeScope scope, object condition) {
  SnapshotFault.Check("descendant-list"); return new object[] {new SnapshotNode()};
 }
}
public class SnapshotWindowInfo {
 public int NativeWindowHandle { get { SnapshotFault.Check("window-handle"); return 7; } }
}
public class SnapshotNode {
 public SnapshotNodeInfo Current { get { SnapshotFault.Check("node-current"); return new SnapshotNodeInfo(); } }
 public bool TryGetCurrentPattern(object kind, out object pattern) {
  bool value=kind==IbisSnapshotContract.ValuePattern.Pattern;
  SnapshotFault.Check(value ? "value-pattern" : "text-pattern");
  bool present=value ? SnapshotFault.Scenario=="value-read" || SnapshotFault.Scenario=="success-value"
   : SnapshotFault.Scenario=="text-read" || SnapshotFault.Scenario=="success-text";
  pattern=present ? (object)new SnapshotPattern() : null; return present;
 }
}
public class SnapshotNodeInfo {
 public SnapshotType ControlType { get { SnapshotFault.Check("node-type"); return new SnapshotType(); } }
 public string Name { get { SnapshotFault.Check("node-name"); return "Synthetic node"; } }
 public bool HasKeyboardFocus { get { SnapshotFault.Check("node-focus"); return true; } }
}
public class SnapshotType { public string ProgrammaticName { get { return "ControlType.Edit"; } } }
public class SnapshotPattern {
 public SnapshotValue Current { get { return new SnapshotValue(); } }
 public SnapshotRange DocumentRange { get { return new SnapshotRange(); } }
}
public class SnapshotValue {
 public string Value { get { SnapshotFault.Check("value-read"); return "Synthetic value"; } }
}
public class SnapshotRange {
 public string GetText(int limit) {
  SnapshotFault.Check("text-read");
  if (limit!=16385) throw new InvalidOperationException(SnapshotFault.Marker);
  return "Synthetic value";
 }
}
public static class IbisSmoke {
 public static bool IsWindow(IntPtr handle) { return SnapshotFault.Scenario!="ownership"; }
 public static uint OwnerPid(IntPtr handle) { return 42; }
 public static bool IsWindowVisible(IntPtr handle) { return true; }
 public static uint ForegroundPid() { return 42; }
}
'@
[SnapshotFault]::Scenario=$Scenario
$Action='snapshot'; $AppId=42; $Text='7'
$completed=$false
try {
 switch ($Scenario) {
  'discovery' {$Text='0'}
  'window-bound' {$Text='0'}
  'handle-parse' {$Text=[SnapshotFault]::Marker}
  'unknown-phase' {$script:SnapshotPhase=[SnapshotFault]::Marker; throw [SnapshotFault]::Marker}
  'case-variant' {$script:SnapshotPhase='DiScOvErY'; throw [SnapshotFault]::Marker}
  'unknown-operation' {$script:SnapshotPhase='provider'; $script:SnapshotOperation=[SnapshotFault]::Marker; throw [SnapshotFault]::Marker}
  'case-operation' {$script:SnapshotPhase='provider'; $script:SnapshotOperation='NoDe-NaMe'; throw [SnapshotFault]::Marker}
 }
 $tree=Get-Tree
 $completed=$true
 if ($Scenario -like 'success-*') {
  ConvertTo-Json -InputObject @{tree=$tree;reads=@([SnapshotFault]::Reads)} -Depth 10 -Compress
  exit 0
 }
 throw 'Expected snapshot rejection'
} catch {
 # A getter fault must abort immediately, not reach a later read or the sentinel.
 if ([SnapshotFault]::Reads.Contains($Scenario) -and
     ($completed -or [SnapshotFault]::Reads[-1] -cne $Scenario)) {
  [Console]::Error.WriteLine('Native contract provider fault continued')
  exit 3
 }
 & $fatal
}
'''
        cases = (
            ("ownership", "ownership", "RuntimeException", "unknown"),
            ("handle-parse", "handle-parse", "other", "unknown"),
            ("discovery", "discovery", "InvalidOperationException", "unknown"),
            ("provider", "provider", "InvalidOperationException", "from-handle"),
            ("window-bound", "window-bound", "RuntimeException", "unknown"),
            ("unknown-phase", "unknown", "RuntimeException", "unknown"),
            ("case-variant", "unknown", "RuntimeException", "unknown"),
            ("unknown-operation", "provider", "RuntimeException", "unknown"),
            ("case-operation", "provider", "RuntimeException", "unknown"),
            *((operation, "provider", "InvalidOperationException", operation) for operation in (
                "from-handle", "window-handle", "descendant-list", "node-current", "value-pattern",
                "value-read", "text-pattern", "text-read", "node-type", "node-name", "node-focus")),
        )
        with tempfile.TemporaryDirectory() as temporary:
            test = Path(temporary) / "snapshot-failure-contract.ps1"
            test.write_text(script, encoding="utf-8")
            for scenario, phase, kind, operation in cases:
                with self.subTest(scenario=scenario):
                    result = subprocess.run([shutil.which("pwsh"), "-NoProfile", "-File", str(test),
                                             "-Source", str(Path(__file__).with_name("smoke-windows.ps1")),
                                             "-Scenario", scenario], capture_output=True, text=True, timeout=20)
                    self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                    facts = [json.loads(line.removeprefix("NATIVE SNAPSHOT FAILURE "))
                             for line in result.stderr.splitlines() if line.startswith("NATIVE SNAPSHOT FAILURE ")]
                    self.assertEqual(facts, [{"phase": phase, "kind": kind, "operation": operation}])
                    self.assertIn(f"NATIVE HELPER ERROR {kind}", result.stderr)
                    self.assertEqual(result.stdout, "")
                    self.assertNotIn("injected-private-clipboard", result.stderr)
            # The successful output and native property-read order stay unchanged.
            for pattern in ("value", "text"):
                with self.subTest(success=pattern):
                    result = subprocess.run([shutil.which("pwsh"), "-NoProfile", "-File", str(test),
                                             "-Source", str(Path(__file__).with_name("smoke-windows.ps1")),
                                             "-Scenario", f"success-{pattern}"], capture_output=True, text=True, timeout=20)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    facts = json.loads(result.stdout)
                    self.assertEqual(facts["tree"], {"nodes": [{"role": "ControlType.Edit", "name": "Synthetic node",
                                                           "value": "Synthetic value", "focused": True}],
                                                     "visible": True, "foreground": True, "picker_handle": 7})
                    reads = ["from-handle", "window-handle", "descendant-list", "node-current", "value-pattern"]
                    reads += ["value-read"] if pattern == "value" else ["text-pattern", "text-read"]
                    self.assertEqual(facts["reads"], reads + ["node-type", "node-name", "node-focus"])
                    self.assertEqual(result.stderr, "")

    @unittest.skipUnless(platform.system() == "Windows" and shutil.which("pwsh"),
                         "Windows UI Automation is required to execute the real snapshot failure paths")
    def test_real_windows_snapshot_rejections_preserve_failure_and_redact_handle_input(self):
        for text, phase, kind in (("-1", "ownership", "RuntimeException"),
                                  ("injected-private-clipboard-引用", "handle-parse", "other")):
            with self.subTest(phase=phase):
                result = subprocess.run([shutil.which("pwsh"), "-NoProfile", "-File",
                                         str(Path(__file__).with_name("smoke-windows.ps1")),
                                         "-Action", "snapshot", "-AppId", "0", "-EditorId", "0", "-Text", text],
                                        capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                facts = [json.loads(line.removeprefix("NATIVE SNAPSHOT FAILURE "))
                         for line in result.stderr.splitlines() if line.startswith("NATIVE SNAPSHOT FAILURE ")]
                self.assertEqual(facts, [{"phase": phase, "kind": kind, "operation": "unknown"}])
                self.assertEqual(result.stdout, "")
                self.assertNotIn("injected-private-clipboard", result.stderr)

    def test_windows_click_contract_timeout_preserves_only_bounded_fixed_phases(self):
        method = ContractTests.test_windows_click_activates_one_exact_owned_button_with_its_native_pattern
        method = getattr(method, "__wrapped__", method)
        phases = ["WINDOWS CLICK CONTRACT parse-start", "WINDOWS CLICK CONTRACT compile-start"]
        dirty = "\n".join([phases[0], "private-output", phases[1], phases[0], "WINDOWS CLICK CONTRACT private-command"])
        for stderr, expected in [(dirty, phases), (dirty.encode() + b'\n\xffprivate', phases), (None, []),
                                 (phases[0] + '\n' + 'private' * 100000 + '\nWINDOWS CLICK CONTRACT complete', phases[:1])]:
            error = subprocess.TimeoutExpired(["private-command"], 20, stderr=stderr)
            with patch("shutil.which", return_value="synthetic-pwsh"), patch("subprocess.run", side_effect=error) as helper, patch("builtins.print") as output:
                with self.assertRaises(subprocess.TimeoutExpired) as caught:
                    method(self)
                self.assertIs(caught.exception, error)
                self.assertIs(error.stderr, stderr)
                self.assertEqual(helper.call_count, 1)
                self.assertEqual(helper.call_args.kwargs["timeout"], 20)
                self.assertEqual([entry.args[0] for entry in output.call_args_list], expected)
                self.assertTrue(all(entry.kwargs.get("file") is sys.stderr for entry in output.call_args_list))

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell is required to execute the Windows click helper contract")
    def test_windows_click_activates_one_exact_owned_button_with_its_native_pattern(self):
        script = r'''
param([string] $Source)
$ErrorActionPreference='Stop'
[Console]::Error.WriteLine('WINDOWS CLICK CONTRACT parse-start')
$tokens=$null; $errors=$null
$ast=[System.Management.Automation.Language.Parser]::ParseFile($Source,[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw 'Helper syntax is invalid' }
$switch=$ast.Find({param($node) $node -is [System.Management.Automation.Language.SwitchStatementAst] -and $node.Condition.Extent.Text -eq '$Action'},$true)
$clause=@($switch.Clauses | Where-Object {$_.Item1.Value -eq 'click'})[0].Item2.Extent.Text
# Real Windows UIA types may already be loaded. Every extracted type and all
# fake provider types must share this isolated namespace, including enums.
$clause=$clause.Replace('[System.Windows.Automation.', '[IbisClickContract.')
$operation=[scriptblock]::Create($clause.Substring(1,$clause.Length-2))
[Console]::Error.WriteLine('WINDOWS CLICK CONTRACT parse-ready')
[Console]::Error.WriteLine('WINDOWS CLICK CONTRACT compile-start')
Add-Type @'
using System;
public static class IbisSmoke {
 public static bool Valid=true, Visible=true; public static uint Owner=42;
 public static long Handle=7;
 public static bool IsWindow(IntPtr handle) { return Valid && handle.ToInt64()==Handle; }
 public static bool IsWindowVisible(IntPtr handle) { return Visible; }
 public static uint OwnerPid(IntPtr handle) { return Owner; }
}
namespace IbisClickContract {
 public enum TreeScope { Descendants }
 public class AutomationElement {
  public static string NameProperty="name"; public static ClickWindow Window;
  public static ClickWindow FromHandle(IntPtr handle) { return Window; }
 }
 public class PropertyCondition { public string Value; public PropertyCondition(string property,string value) { Value=value; } }
 public class ControlType { public static string Button="button", MenuItem="menu", Text="text"; }
 public class InvokePattern { public static int Pattern=1; }
 public class ExpandCollapsePattern { public static int Pattern=2; }
 public class TogglePattern { public static int Pattern=3; }
public class ClickPattern {
 public int Activations; public bool Fail;
 public void Invoke() { Activate(); } public void Expand() { Activate(); } public void Toggle() { Activate(); }
 private void Activate() { Activations++; if(Fail) throw new Exception("uncertain activation"); }
}
public class ClickElement {
 public string Name="Typst"; public ClickElement Current { get { return this; } }
 public string ControlType="button"; public int Supported=3; public ClickPattern Action=new ClickPattern();
 public bool TryGetCurrentPattern(int pattern,ref object value) { value=pattern==Supported ? Action : null; return value!=null; }
}
public class ClickWindow {
 public ClickElement[] Elements=new ClickElement[0];
 public ClickElement[] FindAll(TreeScope scope,PropertyCondition condition) {
  return Array.FindAll(Elements,element=>element.Name==condition.Value);
 }
}
}
'@
[Console]::Error.WriteLine('WINDOWS CLICK CONTRACT compile-ready')
$AppId=42; $PickerHandle=7; $Text='Typst'; $script:window=[IbisClickContract.ClickWindow]::new()
[IbisClickContract.AutomationElement]::Window=$script:window
# This window has no Search control: only the opened modal menu's target.
[Console]::Error.WriteLine('WINDOWS CLICK CONTRACT pattern-cases')
foreach ($nativeHandle in @(7,-2147483647)) {
 [IbisSmoke]::Handle=$nativeHandle; $PickerHandle=$nativeHandle
 foreach ($pattern in @(1,2,3)) {
  $element=[IbisClickContract.ClickElement]::new(); $element.Supported=$pattern; $script:window.Elements=@($element)
  $result=& $operation
  if(!$result.pressed -or !$result.button_role -or $element.Action.Activations -ne 1 -or
     $result.invoke_pattern -ne ($pattern -eq 1) -or $result.expand_collapse_pattern -ne ($pattern -eq 2) -or
     $result.toggle_pattern -ne ($pattern -eq 3)) {throw 'Native pattern did not activate once'}
 }
}
[IbisSmoke]::Handle=7; $PickerHandle=7
[Console]::Error.WriteLine('WINDOWS CLICK CONTRACT ownership-cases')
foreach ($failure in @('stale','foreign','hidden','missing-handle')) {
 $element=[IbisClickContract.ClickElement]::new(); $script:window.Elements=@($element)
 [IbisSmoke]::Valid=$failure -ne 'stale'; [IbisSmoke]::Visible=$failure -ne 'hidden'
 [IbisSmoke]::Owner=if($failure -eq 'foreign') {43} else {42}
 $PickerHandle=if($failure -eq 'missing-handle') {0} else {7}
 $rejected=$false; try {& $operation | Out-Null} catch {$rejected=$true}
 if(!$rejected -or $element.Action.Activations -ne 0) {throw 'Unverified picker reached activation'}
}
[IbisSmoke]::Valid=$true; [IbisSmoke]::Visible=$true; [IbisSmoke]::Owner=42; $PickerHandle=7
[Console]::Error.WriteLine('WINDOWS CLICK CONTRACT rejection-cases')
foreach ($failure in @('missing','wrong-label','text-toggle','menu-toggle','duplicate','unsupported','uncertain','bound')) {
 $element=[IbisClickContract.ClickElement]::new(); $script:window.Elements=@($element)
 switch($failure) {
  'missing' {$script:window.Elements=@()}
  'wrong-label' {$element.Name='private Typst suffix'}
  'text-toggle' {$element.ControlType='text'}
  'menu-toggle' {$element.ControlType='menu'}
  'duplicate' {$script:window.Elements=@($element,[IbisClickContract.ClickElement]::new())}
  'unsupported' {$element.Supported=0}
  'uncertain' {$element.Action.Fail=$true}
  'bound' {$script:window.Elements=@($element)*4001}
 }
 $rejected=$false; try {& $operation | Out-Null} catch {$rejected=$true}
 $expected=if($failure -eq 'uncertain') {1} else {0}
 if(!$rejected -or $element.Action.Activations -ne $expected) {throw 'Unsafe or repeated activation'}
}
[IbisClickContract.AutomationElement]::Window=$null
$rejected=$false; try {& $operation | Out-Null} catch {$rejected=$true}
if(!$rejected) {throw 'Missing owned window accepted'}
[Console]::Error.WriteLine('WINDOWS CLICK CONTRACT complete')
'''
        with tempfile.TemporaryDirectory() as temporary:
            test = Path(temporary) / "click-contract.ps1"
            test.write_text(script, encoding="utf-8")
            try:
                result = subprocess.run([shutil.which("pwsh"), "-NoProfile", "-File", str(test),
                                         "-Source", str(Path(__file__).with_name("smoke-windows.ps1"))],
                                        capture_output=True, text=True, timeout=20)
            except subprocess.TimeoutExpired as error:
                allowed = {f"WINDOWS CLICK CONTRACT {phase}" for phase in (
                    "parse-start", "parse-ready", "compile-start", "compile-ready", "pattern-cases",
                    "ownership-cases", "rejection-cases", "complete",
                )}
                captured = error.stderr or ""
                captured = captured[:4096].decode("utf-8", errors="replace") if isinstance(captured, bytes) else captured[:4096]
                seen = set()
                for line in captured.splitlines():
                    if line in allowed and line not in seen:
                        print(line, file=sys.stderr)
                        seen.add(line)
                raise
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_windows_editor_preflight_rejects_a_pid_without_creation_identity(self):
        with patch.object(Scripted, "call", side_effect=[True, 12345]):
            with self.assertRaisesRegex(RuntimeError, "identity unavailable"):
                Scripted("Windows")

    def test_native_child_rejects_inherited_gallery_modes(self):
        inherited = {"IBIS_GALLERY": "1", "IBIS_GALLERY_STATE": "loaded", "UNCHANGED": "kept"}
        env = real_app_environment(inherited, Path("synthetic.bib"), Path("config"))
        self.assertNotIn("IBIS_GALLERY", env)
        self.assertNotIn("IBIS_GALLERY_STATE", env)
        self.assertEqual(env["UNCHANGED"], "kept")
        self.assertEqual(inherited["IBIS_GALLERY"], "1")

    def test_generic_macos_invalid_index_is_failed_preflight_not_permission_denial(self):
        error = subprocess.CalledProcessError(1, ["osascript"], stderr="System Events got an error: Invalid index. (-1719)")
        with patch.object(Scripted, "call", side_effect=error):
            with self.assertRaisesRegex(RuntimeError, "preflight failed"):
                Scripted("Darwin")

    def test_explicit_macos_permission_denials_remain_unavailable(self):
        for details in ("not allowed assistive access. (-1719)", "not authorized to send Apple events. (-1743)",
                        "TCC event posting is not granted; native smoke unavailable"):
            with self.subTest(details=details):
                error = subprocess.CalledProcessError(1, ["osascript"], stderr=details)
                with patch.object(Scripted, "call", side_effect=error):
                    with self.assertRaises(Unavailable):
                        Scripted("Darwin")

    def test_observed_macos_invalid_class_is_failed_preflight(self):
        error = subprocess.CalledProcessError(1, ["osascript"], stderr="UIElementsEnabled is not a valid class for application System Events (-2700)")
        with patch.object(Scripted, "call", side_effect=error):
            with self.assertRaisesRegex(RuntimeError, "preflight failed"):
                Scripted("Darwin")

    def test_copy_preflight_rejects_unchanged_clipboard_from_a_prior_run(self):
        class BrokenClipboard:
            def set_clipboard(self, text):
                pass

            def clipboard(self):
                return "[@smoke000001]"

        with self.assertRaisesRegex(AssertionError, "sentinel"):
            prime_clipboard(BrokenClipboard())

    def test_windows_settings_use_native_known_folder_and_preserve_existing_files(self):
        class Native:
            def call(self, action):
                self.action = action
                return str(known)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            known = root / "native roaming"
            redirected = root / "ignored APPDATA override"
            native = Native()
            with patch.dict(os.environ, {"APPDATA": str(redirected)}):
                actual = settings_path("Windows", root, native)
            self.assertEqual(native.action, "config-dir")
            self.assertEqual(actual, known / "io.github.ibiscite.ibis/settings.json")
            actual.parent.mkdir(parents=True)
            backup = actual.with_suffix(".json.bak")
            backup.write_text("existing synthetic preferences", encoding="utf-8")
            with self.assertRaisesRegex(Unavailable, "existing Ibis settings"):
                claim_settings(actual)
            self.assertEqual(backup.read_text(encoding="utf-8"), "existing synthetic preferences")
            self.assertFalse(actual.exists())

    def test_nightly_settings_and_the_update_question_answered_in_advance(self):
        with tempfile.TemporaryDirectory() as temporary:
            settings = settings_path("Linux", Path(temporary), identifier="io.github.ibiscite.ibis.nightly")
            self.assertEqual(settings, Path(temporary) / "config/io.github.ibiscite.ibis.nightly/settings.json")
            answer_update_question(settings)
            self.assertEqual(json.loads(settings.read_text(encoding="utf-8")), {"schema_version": 2, "update_checks": False})
            settings.write_text("{invalid synthetic settings", encoding="utf-8")
            answer_update_question(settings)
            self.assertEqual(settings.read_text(encoding="utf-8"), "{invalid synthetic settings")

    def test_settings_cleanup_removes_only_files_claimed_in_a_fresh_account(self):
        with tempfile.TemporaryDirectory() as temporary:
            candidate = Path(temporary) / "settings.json"
            owned = claim_settings(candidate)
            for suffix in (".json", ".json.bak", ".json.tmp"):
                owned.with_suffix(suffix).write_text("synthetic", encoding="utf-8")
            unrelated = owned.parent / "unrelated.txt"
            unrelated.write_text("preserved", encoding="utf-8")
            clean_settings(owned)
            self.assertEqual(list(owned.parent.iterdir()), [unrelated])

    def test_tree_rejects_names_without_editable_search_and_list_roles(self):
        bad = {"nodes": [{"role": "label", "name": n} for n in ("Search references", "Search results", "Results")]}
        with self.assertRaisesRegex(AssertionError, "editable"):
            require_tree(bad)

    def test_fixture_has_exact_synthetic_keys(self):
        self.assertEqual(fixture(100000).count("@article{"), 100000)
        self.assertIn("@article{smoke099999,", fixture(100000))

    def test_static_text_cannot_impersonate_an_editable_search(self):
        tree = {"nodes": [{"role": "static text", "name": "Search references"}, {"role": "status", "name": "Search results"}, {"role": "list", "name": "Results"}]}
        with self.assertRaisesRegex(AssertionError, "editable"):
            require_tree(tree)


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        unittest.main(argv=[sys.argv[0]])
    elif sys.argv[1:2] == ["--linux-failure-census"]:
        try:
            if platform.system() != "Linux" or len(sys.argv) != 4:
                raise ValueError("Private census mode unavailable")
            linux_census_worker(int(sys.argv[2]), int(sys.argv[3]))
        except Exception:
            sys.exit(1)
    else:
        sys.exit(main())
