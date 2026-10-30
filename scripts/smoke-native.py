#!/usr/bin/env python3
"""Exercise the embedded Linux native picker; never substitute a browser or IPC mock.

Exit 1 means a failed assertion; exit 2 means the session lacks a required native
capability. Only synthetic test text is recorded, never the accessibility tree or
clipboard contents. See ../CONTRIBUTING.md before running on a personal desktop.
"""
import argparse
from contextlib import contextmanager, redirect_stderr, redirect_stdout
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
# Contract subprocesses also load this file with runpy, outside Python's script path.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from native_test_settings import clean_settings, settings_exist
from native_evidence import (ACTIVE, BundleError, Evidence, collect_metadata, failure,
                             load_bundle, observe_native, recording, unavailable)


class Unavailable(RuntimeError):
    def __init__(self, message, code="native-session"):
        super().__init__(message)
        self.code = code


class NativeSetupFailure(RuntimeError):
    """A synthetic desktop precondition failed before Ibis was launched."""


class WindowObservationFailure(RuntimeError):
    """Window identity is unknown; it is never positive evidence of hiding."""

    def __init__(self, branch="unspecified"):
        self.branch = branch
        super().__init__(f"X11 window identity could not be established ({branch}).")


def native_session():
    try:
        native = Linux()
    except AssertionError:
        # Setup waits assert too, but no product was exercised yet. Keep the
        # failure typed and fixed; desktop diagnostics never enter evidence.
        raise NativeSetupFailure("Native test setup could not be established.") from None
    return observe_native(native)


# The short Copied notice is a second visible window of the app (#496); it is
# never the picker, so picker visibility checks skip it.
NOTICE_TITLE = "Ibis notice"
PICKER_TITLE = "Ibis"
EDITOR_TITLE = "Ibis synthetic editor"

# Execution hang guard; C# compilation happens separately before timed calls.
CONTRACT_TIMEOUT = 120


def run(*args, timeout=20):
    return subprocess.check_output(args, text=True, stderr=subprocess.PIPE, timeout=timeout).strip()


def fixture(count=3):
    return "\n".join(
        f"@article{{smoke{i:06d}, title={{Ibis smoke reference {i:06d}}}, "
        "author={Example, Alex}, year={2026}}" for i in range(count)
    )


def real_app_environment(inherited, library, config):
    env = dict(inherited, IBIS_LIBRARY=str(library), XDG_CONFIG_HOME=str(config),
               NO_AT_BRIDGE="0", GTK_MODULES="gail:atk-bridge")
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


def retained_snapshot_fragments(snapshot):
    """Fixed predicates only, from one bounded native snapshot; never echo text."""
    # Native traversal order is not reading order (Linux walks a LIFO stack).
    # Match complete atomic wording or one composite text node, never invent a
    # phrase by joining neighboring elements from different native boundaries.
    values = [" ".join(str(node.get(key, "")).split())
              for node in snapshot["nodes"] for key in ("name", "value")]
    def contains(fragment, ending=r"(?!\w)"):
        return any(re.search(r"(?:^|^· | · )" + re.escape(fragment) + ending, value) is not None
                   for value in values)
    # The count is a complete clause: either its own node/summary, or followed
    # by the spoken detail's check time or failure. A word boundary alone would
    # also accept "4 references missing; 3 references retained".
    count_ending = r"(?=$|\. (?:checked |The library could not be opened: |the file could not be read$|Zotero could not be reached$))"
    return {"not_updated": contains("not updated"),
            "four_references": contains("4 references", count_ending)}


def missing_library_evidence(snapshot, name):
    text = strings(snapshot)
    # Bidi isolates can expose the separator and state as separate native
    # text nodes. Their boundaries are whitespace, not missing status words.
    retained = retained_snapshot_fragments(snapshot)
    return {key: fragment if type(fragment) is bool else fragment in text for key, fragment in {
        "failed_source_named": f"The library could not be opened: {name}",
        "retention_explained": "the library file is unavailable; keeping the last loaded references",
        "recovery_action": "Choose it again or reconnect the drive.",
        # The footer says the library was not updated; its detail, read by
        # screen readers, keeps the retained count. Names are isolated (U+2068/9).
        "retained_snapshot": all(retained.values()),
    }.items()}




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
        except (AssertionError, RuntimeError, subprocess.CalledProcessError) as error:
            if isinstance(native, Linux) and isinstance(error, native.accessibility_error):
                raise
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
            unknown = False
            while time.monotonic() < deadline:
                try:
                    last.update(snapshot())
                except WindowObservationFailure:
                    # Withdrawal can race the read. Keep this action's deadline
                    # and require positive hiding evidence before returning.
                    unknown = True
                    time.sleep(min(.1, max(0, deadline - time.monotonic())))
                    continue
                unknown = False
                if not last["visible"]:
                    return
                if press == 0 and not has_search_text(last):
                    break
                time.sleep(0.1)
            if unknown:
                raise AssertionError("Timed out: picker visibility unavailable after Escape")
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
            # AT-SPI may publish focused Search before X11 routes keys to
            # the app. Do not send Escape into the previous native window.
            return current["visible"] and search_focused(current) and native.picker_has_input_focus()
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


def verify_reopened_search(native, snapshot, text):
    """Prove Search owns keys and the previous query is selected on every show.

    AT-SPI may expose aria-activedescendant as focused after a query. Typing
    once and reading back a different query proves the design's focus/selection
    contract even then. Never click, select-all or retry to repair app focus.
    """
    def values():
        current = snapshot()
        if not current["visible"] or not current["foreground"]:
            return []
        return [node.get("value") for node in named(current, "Search references")
                if node.get("role", "").lower().replace(" ", "") in EDITABLE_ROLES]

    wait_for(values, "reopened picker visible, foreground and editable search")
    native.text(text)

    def replaced():
        current = values()
        return bool(current) and all(value == text for value in current)

    wait_for(replaced, "reopened search receives typing and replaces the previous query")


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


def settings_path(system, temporary, native=None, identifier="com.ibiscite.ibis"):
    return temporary / "config" / identifier / "settings.json"


def answer_update_question(settings):
    """Release builds ask about update checks at first launch; answer "Not Now"
    in advance so the question never takes focus from the picker."""
    if not settings.exists():
        settings.parent.mkdir(parents=True, exist_ok=True)
        settings.write_text(json.dumps({"schema_version": 2, "update_checks": False}), encoding="utf-8")


def claim_settings(candidate):
    if settings_exist(candidate):
        raise Unavailable("Use a disposable desktop account without existing Ibis settings", "disposable-settings")
    return candidate


CLIPBOARD_SENTINEL = "Ibis native smoke clipboard sentinel"


def prime_clipboard(native):
    native.set_clipboard(CLIPBOARD_SENTINEL)
    assert native.clipboard() == CLIPBOARD_SENTINEL, "Native clipboard sentinel could not be established"


def copy_picker_observation(native):
    """One existing native probe, reduced to fixed facts; never a readiness wait."""
    try:
        # Optional probes must not disclose native errors or replace the
        # actual copy-test failure.
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


TIMED_METRICS = ("startup_ms", "first_result_ms")
MEASURED_LAUNCHES = 3


def summarize_performance(samples):
    measured = [sample for sample in samples if not sample.get("warmup")]
    if not measured:
        raise ValueError("No measured 100k launches")
    return {"status": "diagnostic-only", "release_evidence": False, "references": 100000,
            "sample_count": len(measured), "warmup_count": len(samples) - len(measured), "samples": samples,
            "summary": "timings: median of fresh measured launches after one discarded warm-up; memory: maximum of all launches; "
                       "OS file caches persist",
            **{metric: statistics.median(sample[metric] for sample in measured) for metric in TIMED_METRICS},
            "memory_bytes": max(sample["memory_bytes"] for sample in samples),
            "maxima": {metric: max(sample[metric] for sample in samples) for metric in TIMED_METRICS},
            "memory_kind": "summed current process-tree RSS/working set; shared pages may be counted more than once"}


MEASURED_OPENS = 20
OPEN_TARGET_MS = 100


def measure_picker_open(native, snapshot, shortcut, text=None, *, report=None):
    """Shortcut to visible, foreground, focused Search on the retained snapshot.

    On X11, wait only for mapping before typing, never for input/Search focus.
    Earlier keys still belong to the previous window; Ibis cannot buffer them.
    Never retry typing or select-all: either would conceal dropped opening keys.
    Native injection and accessibility probe costs remain inside the timer.
    """
    started = time.monotonic()
    last = {"nodes": []}

    def interactive():
        nonlocal last
        last = current = snapshot()
        # AT-SPI can expose aria-activedescendant as focused instead of the
        # combobox. Warm/cold native typing below proves keyboard ownership.
        editable = any(node.get("role", "").lower().replace(" ", "") in EDITABLE_ROLES
                       for node in named(current, "Search references"))
        focused = search_focused(current) or any(node.get("focused") and
            node.get("role", "").lower().replace(" ", "") in ("option", "listitem", "controltype.listitem")
            for node in current["nodes"])
        return current["visible"] and current["foreground"] and editable and focused

    try:
        native.key(shortcut)
        if text is not None:
            wait_for(native.picker_mapped, "picker mapped before opening typing", interval=.005)
            native.text(text)
        wait_for(interactive, "shortcut opens interactive picker", interval=.005)
        elapsed = (time.monotonic() - started) * 1000
        if text is not None:
            def retained():
                nonlocal last
                last = current = snapshot()
                values = [node.get("value") for node in named(current, "Search references")
                          if node.get("role", "").lower().replace(" ", "") in EDITABLE_ROLES]
                return current["visible"] and current["foreground"] and bool(values) and all(value == text for value in values)
            wait_for(retained, "opening keystrokes retained exactly")
    except AssertionError:
        if text is not None:
            # Only this synthetic Search and our disposable editor are recorded.
            # Keep the final field observation instead of making a late read.
            failure = {"expected_text": text, "search_values": [node.get("value")
                       for node in named(last, "Search references")
                       if node.get("role", "").lower().replace(" ", "") in EDITABLE_ROLES]}
            try:
                failure["editor"] = native.editor_received()
            except (OSError, subprocess.SubprocessError):
                failure["editor"] = {"available": False}
            if report is not None:
                report["opening_text_failure"] = failure
            print("OPENING TEXT FAILURE " + json.dumps(failure), file=sys.stderr)
        raise
    return elapsed


def summarize_picker_opens(samples):
    if len(samples) < MEASURED_OPENS:
        raise ValueError("At least 20 picker opens are required for p95")
    if any(type(value) not in (int, float) or not math.isfinite(value) or value < 0 for value in samples):
        raise ValueError("Picker open times must be finite nonnegative numbers")
    p95 = sorted(samples)[math.ceil(.95 * len(samples)) - 1]
    return {"status": "report-only", "platform": "Linux", "gate_enforced": False,
            "sample_count": len(samples), "samples_ms": samples,
            "median_ms": statistics.median(samples), "p95_ms": p95, "target_p95_ms": OPEN_TARGET_MS,
            "summary": "Timing is reported on Linux only. The 100 ms p95 gate is enforced nowhere yet; "
                       "macOS/Windows open time is judged in maintainer hand test #79.",
            "percentile": "nearest rank; no discarded opens",
            "measurement": "shortcut injection through visible, foreground picker with Search/active-descendant focus; includes native probe overhead",
            "reason": "Linux uses Xvfb/Openbox, full AT-SPI tree traversal and xdotool subprocesses; "
                      "runner and probe latency cannot reliably resolve a 100 ms app budget"}


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


def managed_client_windows(xprop_line):
    """EWMH managed clients, independently of the state being asserted."""
    match = re.fullmatch(r"_NET_CLIENT_LIST\(WINDOW\): window id #\s*(.*)", xprop_line.strip())
    if not match:
        return None
    values = match[1].split(", ") if match[1] else []
    if len(values) > CENSUS_WINDOWS or any(not re.fullmatch(r"0x[0-9a-fA-F]{1,8}", value) for value in values):
        return None
    return {int(value, 16) for value in values}


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


# Disable terminal line buffering so a lost prefix is observable without Return.
# ECHO stays enabled, making this the same visible synthetic editor as cat.
SYNTHETIC_EDITOR_SCRIPT = """import os, sys, tty
with open(sys.argv[1], "ab", buffering=0) as received:
    tty.setcbreak(sys.stdin.fileno())
    while chunk := os.read(sys.stdin.fileno(), 4096):
        received.write(chunk)
"""


class Linux:
    def __init__(self):
        for program in ("xdotool", "xclip", "xterm", "openbox", "xprop"):
            if not shutil.which(program):
                raise Unavailable(f"Linux native smoke needs {program}", "native-driver")
        if not os.environ.get("DISPLAY") or not os.environ.get("DBUS_SESSION_BUS_ADDRESS"):
            raise Unavailable("Linux smoke needs X11 and a D-Bus session (use xvfb-run and dbus-run-session)", "linux-x11")
        try:
            import pyatspi
            from gi.repository import GLib
            self.atspi = pyatspi
            self.accessibility_error = GLib.Error
        except (ImportError, ValueError) as error:
            raise Unavailable("Use /usr/bin/python3 with distro python3-pyatspi installed", "accessibility-driver") from error
        self.wm = subprocess.Popen(["openbox"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        # A window mapped before the window manager runs keeps no EWMH state:
        # GTK sends later hint changes such as skip-taskbar to the manager
        # instead of writing them. Desktops start their manager first; so must we.
        wait_for(self.window_manager_ready, "openbox manages the display")
        self.editor_input = tempfile.NamedTemporaryFile(prefix="ibis-synthetic-editor-")
        self.editor = subprocess.Popen(["xterm", "-title", EDITOR_TITLE, "-e",
            sys.executable, "-c", SYNTHETIC_EDITOR_SCRIPT, self.editor_input.name],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.app = None
        self.last_window_state = "not read"
        # The previous app must settle before Ibis launches. A late xterm map
        # can otherwise steal initial focus from the already-visible picker.
        def editor_ready():
            if self.editor.poll() is not None:
                raise Unavailable("Synthetic editor exited before app launch")
            try:
                return self.window(self.editor.pid, EDITOR_TITLE) is not None and self.editor_focused()
            except subprocess.CalledProcessError:
                return False
        try:
            wait_for(editor_ready, "synthetic editor mapped and focused before app launch")
        except BaseException:
            self.close()
            raise

    def window(self, pid, title):
        # Same-process GTK auxiliaries can be viewable without being the app's
        # managed picker. Bind purpose and ownership before observing its hints.
        deadline = time.monotonic() + 20
        def query(branch, *args):
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise WindowObservationFailure("query-deadline")
            try:
                return run(*args, timeout=remaining)
            except subprocess.CalledProcessError as error:
                # xdotool documents exit 1 for a completed search with no match.
                # A failed command/display query with diagnostics is uncertainty.
                if branch == "search-query" and error.returncode == 1 and not (error.output or "").strip() and not (error.stderr or "").strip():
                    return ""
                raise WindowObservationFailure(branch) from None
            except (subprocess.TimeoutExpired, OSError):
                raise WindowObservationFailure(branch) from None

        def observe():
            windows = query("search-query", "xdotool", "search", "--onlyvisible", "--pid", str(pid)).splitlines()
            if not windows:
                return None
            managed = managed_client_windows(query("managed-query", "xprop", "-root", "_NET_CLIENT_LIST"))
            if managed is None:
                raise WindowObservationFailure("managed-property")
            if len(windows) > CENSUS_WINDOWS:
                raise WindowObservationFailure("window-bound")
            candidates = set()
            for window in windows:
                if not re.fullmatch(r"[1-9][0-9]{0,9}", window):
                    raise WindowObservationFailure("window-id")
                if query("owner-query", "xdotool", "getwindowpid", window) != str(pid):
                    raise WindowObservationFailure("owner-mismatch")
                name = query("title-query", "xdotool", "getwindowname", window)
                if int(window) not in managed:
                    if name == title:
                        raise WindowObservationFailure("title-not-managed")
                    continue
                if name == title:
                    candidates.add(window)
                elif name != NOTICE_TITLE:
                    raise WindowObservationFailure("unexpected-managed-title")
            if not candidates:
                return None
            if len(candidates) != 1:
                raise WindowObservationFailure("ambiguous-candidates")
            window = candidates.pop()
            # Refuse a destroyed/replaced candidate; the next observation
            # re-enumerates, so a closed window never supplies cached identity.
            if query("candidate-owner-query", "xdotool", "getwindowpid", window) != str(pid):
                raise WindowObservationFailure("candidate-owner-changed")
            if query("candidate-title-query", "xdotool", "getwindowname", window) != title:
                raise WindowObservationFailure("candidate-title-changed")
            return window

        # X search, WM membership and properties are separate server reads.
        # Mapping/withdrawal may cross them even for a direct snapshot caller.
        # Retry the whole observation within its original budget, never treat
        # an uncertain read as absence or carry a candidate into the next read.
        reported = set()
        while True:
            try:
                return observe()
            except WindowObservationFailure as error:
                if error.branch not in reported:
                    print("LINUX WINDOW REOBSERVE " + error.branch, file=sys.stderr, flush=True)
                    reported.add(error.branch)
                remaining = deadline - time.monotonic()
                if remaining > 0:
                    time.sleep(min(.1, remaining))
                if time.monotonic() >= deadline:
                    # Fixed labels only: never include titles, XIDs or stderr.
                    # Print here as a polling caller may replace the exception
                    # with its own timeout, losing the failed branch again.
                    print("LINUX WINDOW OBSERVATION FAILURE " + error.branch, file=sys.stderr, flush=True)
                    raise

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
        window = self.window(self.app.pid, PICKER_TITLE)
        try:
            active = run("xdotool", "getactivewindow")
        except subprocess.CalledProcessError:
            active = ""
        return {"nodes": nodes, "visible": bool(window), "foreground": window == active if window else False}

    def picker_mapped(self):
        # --onlyvisible checks X11 map state, without querying or waiting for focus.
        return self.window(self.app.pid, PICKER_TITLE) is not None

    def picker_has_input_focus(self):
        # Like the editor check, xdotool resolves a child without WM_CLASS to
        # its native ancestor. EWMH activation and AT-SPI focus alone can lag
        # actual X11 keyboard ownership during the initial presentation.
        try:
            focused = run("xdotool", "getwindowfocus")
            return run("xdotool", "getwindowpid", focused) == str(self.app.pid)
        except subprocess.CalledProcessError:
            return False

    def editor_received(self):
        if self.editor.poll() is not None:
            return {"available": False}
        self.editor_input.seek(0)
        data = self.editor_input.read(4097)
        return {"available": True, "text": data[:4096].decode("utf-8", errors="replace"),
                "truncated": len(data) > 4096}

    def key(self, chord):
        run("xdotool", "key", "--clearmodifiers", chord)

    def text(self, text):
        run("xdotool", "type", "--clearmodifiers", "--delay", "0", text)

    def click(self, name):
        def children(parent, *, owned=False):
            try:
                count = parent.childCount
            except self.accessibility_error:
                return
            for index in range(count):
                try:
                    child = parent[index]
                except IndexError:
                    break  # The collection shrank after childCount was read.
                except self.accessibility_error:
                    continue
                try:
                    if child and (not owned or child.get_process_id() == self.app.pid):
                        yield child
                except self.accessibility_error:
                    continue

        desktop = self.atspi.Registry.getDesktop(0)
        stack = list(children(desktop, owned=True))
        while stack:
            item = stack.pop()
            action = None
            try:
                if item.name == name:
                    action = item.queryAction()
                    if not action.nActions:
                        action = None
            except self.accessibility_error:
                continue
            except NotImplementedError:
                action = None
            # An action error can mean it already ran. Never rediscover/retry it.
            if action is not None and action.doAction(0):
                return
            stack.extend(children(item))
        raise AssertionError(f"No invokable accessibility control {name!r}")

    def notice_visible(self):
        try:
            return bool(run("xdotool", "search", "--onlyvisible", "--pid", str(self.app.pid), "--name", f"^{NOTICE_TITLE}$"))
        except subprocess.CalledProcessError:
            return False

    def focus_editor(self):
        window = self.window(self.editor.pid, EDITOR_TITLE)
        if not window:
            raise Unavailable("Synthetic terminal editor has no X11 window", "synthetic-editor")
        run("xdotool", "windowactivate", "--sync", window)

    def editor_active(self):
        return self.window(self.editor.pid, EDITOR_TITLE) == run("xdotool", "getactivewindow")

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
        window = self.window(self.app.pid, PICKER_TITLE)
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
        self.editor_input.close()
        self.wm.terminate()


def wait_for(predicate, description, timeout=20, *, interval=.1):
    # Source locations are stable IDs within the recorded harness hash. Never
    # serialize descriptions: callers may interpolate desktop observations.
    with recording("request", f"wait-{sys._getframe(1).f_lineno}"):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                if predicate():
                    return
            except WindowObservationFailure:
                # Mapping/withdrawal can race a query. Unknown is not a positive
                # condition, including when the caller is waiting for hiding.
                pass
            time.sleep(interval)
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
        native.click(name)
        remaining()
    def ready():
        current = native.snapshot()
        remaining()
        return current["visible"] and bool(named(current, "Quit Ibis"))
    activate("More")
    wait_for(ready, "Quit appears in More", timeout=remaining())
    activate("Quit Ibis")  # One activation; never retry an uncertain native action.
    assert app.wait(timeout=remaining()) == 0, "Normal Quit did not exit zero"
    # Use the same final sample for acceptance and elapsed-time evidence.
    return round(10 - remaining(), 3)


NO_MATCH = "zzqx no such reference"


def first_run_preferences_main(args):
    return evidence_main(args, _first_run_preferences_main, "first-run-preferences")


def _first_run_preferences_main(args):
    args.output.mkdir(parents=True, exist_ok=True)
    system = platform.system()
    report = {"platform": system, "scenario": "first-run-preferences", "status": "failed", "checks": []}
    native = app = log = owned_settings = temporary = None
    exit_code = 1
    try:
        if not args.app.is_file():
            raise Unavailable("Build the embedded app on Linux/X11", "app-build")
        if system != "Linux":
            raise Unavailable("Build the embedded app on Linux/X11", "linux-x11")
        native = native_session()
        temporary = tempfile.TemporaryDirectory(prefix="ibis native preferences ")
        folder = Path(temporary.name)
        library = folder / "synthetic 引用.bib"
        library.write_text(fixture(), encoding="utf-8")
        settings = settings_path(system, folder, native, args.identifier)
        owned_settings = claim_settings(settings)
        env = real_app_environment(os.environ, library, folder / "config")
        shortcut = args.shortcut or ("ctrl+shift+y")

        def launch(label, with_library=False):
            nonlocal app, log
            if log:
                log.close()
            log = (args.output / f"startup-{label}.log").open("w", encoding="utf-8")
            child = dict(env)
            if not with_library:
                child.pop("IBIS_LIBRARY", None)
            answer_update_question(settings)
            with recording("action", "launch-" + label):
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
        wait_for(lambda: visible_text("Where do your references live?"), "native first-run setup")
        report["first_quit_seconds"] = quit_normally(native, app)
        passed("settled first-run More/Quit exits zero within ten seconds")
        launch("relaunch", with_library=True)
        wait_for(lambda: visible_text("3 references"), "synthetic references after relaunch")
        wait_for(lambda: picker_ready(typing=True), "restarted native search focus")
        # The writing question needs a library chosen in the app; Copy as sets the same preference.
        # The header has no stable native identifier; keep its action in the accessible name.
        # With no match there is no footer copy action to invoke accidentally.
        type_query(native, snapshot, NO_MATCH, "ctrl+a")
        wait_for(lambda: visible_text("No matching references"), "no highlighted result to copy")
        native.click("Copy as Pandoc key")
        wait_for(lambda: bool(named(snapshot(), "Typst key")), "Copy as lists Typst key")
        # Pick with the filter: accessibility exposes the menu's radio items
        # without a pattern the exact-name click helper accepts.
        # Not every platform reports focus on the filter; the filtered list proves where typing went.
        native.text("Typst")
        # "\cite" names only that menu item; "Pandoc" is also the header's visible text.
        wait_for(lambda: not named(snapshot(), "\\cite"), "Copy as filter leaves only Typst")
        native.key("Down")
        native.key("Return")
        wait_for(lambda: saved_typst(settings), "Typst preferences saved before copying")
        passed("Typst choice is saved before any copy")
        type_query(native, snapshot, "smoke000001", "ctrl+a")
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
        passed("copy uses the saved Typst format")
        wait_for(lambda: not snapshot()["visible"], "Typst copy hides picker")
        native.key(shortcut)
        # AT-SPI can focus the active result while the combobox retains input.
        # More/Quit needs the same foreground/named-search state as reopen().
        wait_for(picker_ready, "picker available for normal Quit")
        report["second_quit_seconds"] = quit_normally(native, app)
        passed("restarted More/Quit exits zero within ten seconds")
        assert saved_typst(settings), "Normal Quit lost the saved Typst choice"
        report["status"], exit_code = "passed", 0
    except Unavailable as error:
        unavailable(error.code)
        report.update(status="unavailable", reason=str(error))
        exit_code = 2
    except (AssertionError, RuntimeError, ValueError, KeyError, TypeError, subprocess.SubprocessError, OSError) as error:
        failure(error)
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
            except (OSError, subprocess.SubprocessError, RuntimeError, AssertionError) as error:
                failure(error, "cleanup")
                report.update(status="failed", reason="Owned native preference cleanup failed")
                exit_code = 1
        print("NATIVE FIRST-RUN PREFERENCES " + json.dumps(report), flush=True)
        (args.output / "smoke.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return exit_code


def evidence_main(args, scenario, scenario_id):
    """Always persist the closed manifest, independently of legacy diagnostics."""
    try:
        replay = getattr(args, "replay_bundle", None)
        options = {"identifier": args.identifier, "shortcut": args.shortcut or "ctrl+shift+y",
                   "skip_performance": bool(getattr(args, "skip_performance", False)),
                   "seed": getattr(args, "seed", 0)}
        fixtures = {"small": fixture().encode(), "initial-document": b"",
                    "initial-settings": b'{"schema_version": 2, "update_checks": false}'}
        if scenario_id == "native-smoke":
            fixtures.update(four=fixture(4).encode(), empty=b"",
                            damaged=(Path(__file__).resolve().parent.parent / "fixtures/bibtex/damaged.bib").read_bytes())
            if not options["skip_performance"]:
                fixtures["large"] = fixture(100000).encode()
        recorder = Evidence(scenario_id, options, collect_metadata(args.app, fixtures), replay)
        recorder.finish(2)  # Validate all metadata/options before any native action.
    except BundleError as error:
        print(str(error), file=sys.stderr)
        return 1
    token = ACTIVE.set(recorder)
    exit_code = 1
    try:
        exit_code = scenario(args)
    except (KeyboardInterrupt, SystemExit) as error:
        # Interrupted waits record cancellation, and still leave a bundle.
        recorder.failure(error)
        exit_code = 1
    except Exception as error:
        # Includes setup/cleanup failures outside the legacy scenario handlers.
        recorder.failure(error)
        exit_code = 1
    finally:
        ACTIVE.reset(token)
    try:
        bundle = recorder.finish(exit_code)
        recorder.write(args.output, bundle)
        return {"pass": 0, "fail": 1, "unavailable": 2}[bundle["result"]["outcome"]]
    except BundleError as error:
        print(str(error), file=sys.stderr)
        return 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--identifier", help="Bundle identifier, which names the settings folder")
    parser.add_argument("--shortcut", help="xdotool-style chord; defaults to Ctrl+Shift+Y")
    parser.add_argument("--skip-performance", action="store_true", default=None, help="Keep correctness checks but skip the four 100,000-reference launches")
    parser.add_argument("--first-run-preferences", action="store_true", default=None, help="Exercise first-run format persistence and settled More/Quit only")
    parser.add_argument("--seed", type=int, help="Recorded seed (the current scenarios are deterministic)")
    parser.add_argument("--replay", type=Path, help="Replay a version-1 evidence manifest against the same build")
    args = parser.parse_args()
    try:
        args.replay_bundle = load_bundle(args.replay) if args.replay else None
        defaults = (args.replay_bundle["options"] if args.replay_bundle else
                    {"identifier": "com.ibiscite.ibis", "shortcut": "ctrl+shift+y", "skip_performance": False, "seed": 0})
        for key, value in defaults.items():
            if getattr(args, key) is None:
                setattr(args, key, value)
        if args.first_run_preferences is None and args.replay_bundle:
            args.first_run_preferences = args.replay_bundle["scenario"] == "first-run-preferences"
    except BundleError as error:
        print(str(error), file=sys.stderr)
        return 1
    if args.first_run_preferences:
        return first_run_preferences_main(args)
    return evidence_main(args, smoke_main, "native-smoke")


def smoke_main(args):
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
            raise Unavailable("Build the embedded native app before running smoke", "app-build")
        if system != "Linux":
            raise Unavailable("Unsupported native smoke platform", "linux-x11")
        native = native_session()
        with tempfile.TemporaryDirectory(prefix="ibis native smoke ") as temporary:
            folder = Path(temporary)
            library = folder / "synthetic 引用.bib"
            library.write_text(fixture(), encoding="utf-8")
            env = real_app_environment(os.environ, library, folder / "config")
            # Isolate preferences in this disposable run.
            settings = settings_path(system, folder, native, args.identifier)

            def launch(label):
                nonlocal app, log
                log_path = args.output / f"startup-{label}.log"
                log = log_path.open("w", encoding="utf-8")
                diagnostic_logs.append(log_path)
                answer_update_question(settings)
                with recording("action", "launch-" + label):
                    app = subprocess.Popen([str(args.app.resolve())], env=env, stdout=log, stderr=log)
                native.app = app

            def stop():
                nonlocal app, log
                with recording("action", "stop"):
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

            def check(name, action):
                try:
                    with recording("request", f"check-{sys._getframe(1).f_lineno}"):
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
                native.key("ctrl+a")
                native.text(text)
                wait_for_query(snapshot, text, expected, report)

            shortcut = args.shortcut or ("ctrl+shift+y")
            # Map the picker before observing its footer: an unmapped WebKitGTK
            # window exposes only the AT-SPI application root, not its contents.
            launch("small")
            check("startup", lambda: wait_for(lambda: shows_count(snapshot(), "3 references"), "synthetic library startup"))
            def first_shortcut_open():
                # This is the first shortcut after a normal visible startup,
                # not a never-mapped login window's first presentation.
                prepare_initial_picker_show(native, snapshot)
                native.focus_editor()
                wait_for(native.editor_focused, "editor focused before first picker open")
                measure_picker_open(native, snapshot, shortcut, "smoke000000", report=report)
                wait_for_query(snapshot, "smoke000000", "Ibis smoke reference 000000", report, settled=True)
            check("first shortcut after startup keeps keystrokes from mapping", first_shortcut_open)
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
                native.key(shortcut)
                focus_state("after-first-shortcut")
                verify_reopened_search(native, snapshot, "smoke000001")
                escape_to_hide(native, snapshot, "Escape hides picker")
                native.focus_editor()
                wait_for(native.editor_focused, "previous editor focused")
                focus_state("before-second-shortcut")
                native.key(shortcut)
                focus_state("after-second-shortcut")
                verify_reopened_search(native, snapshot, "smoke000000")
            check("global shortcut shows foreground picker with focused search", show)
            if system == "Linux":
                check("shown picker stays out of the taskbar and window switcher", skips_taskbar)

            def second_instance():
                escape_to_hide(native, snapshot, "picker hidden before second launch")
                native.focus_editor()
                wait_for(native.editor_focused, "editor focused before second launch")
                with (args.output / "startup-second-instance.log").open("w", encoding="utf-8") as second_log:
                    with recording("action", "launch-second-instance"):
                        second = subprocess.Popen([str(args.app.resolve())], env=env, stdout=second_log, stderr=second_log)
                    try:
                        assert second.wait(timeout=20) == 0, "Second instance did not exit successfully"
                    finally:
                        if second.poll() is None:
                            second.terminate()
                            second.wait(timeout=10)
                verify_reopened_search(native, snapshot, "smoke000002")
                assert app.poll() is None, "Primary instance exited during second launch"
            check("second launch exits and activates existing picker", second_instance)
            check("native typing and accessible result count", lambda: query("smoke000001", "1 result"))
            assert "Ibis smoke reference 000001" in strings(snapshot()), "Expected native result missing"
            check("shortcut menu Escape returns to invoker", lambda: menu_escape_focus(
                native, snapshot, "ctrl+k", report))
            # Restoring the query can schedule another search; do not copy the
            # temporary probe's results while that request is still pending.
            wait_for_query(snapshot, "smoke000001", "Ibis smoke reference 000001", report, settled=True)

            def copy():
                # Installed release builds never report the non-activating panel as the
                # key window although Return still reaches it (#454), so the
                # key-window wait stays out of the native copy check.
                copy_with_diagnostics(native, report)
                wait_for(lambda: not snapshot()["visible"], "copy hides picker")
                try:
                    wait_for(native.editor_focused, "copy restores focus to previous editor")
                except AssertionError:
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
                with recording("action", "settings-live-edit"):
                    settings.write_text(json.dumps(external), encoding="utf-8")
                external_bytes = settings.read_bytes()
                try:
                    copy()
                    saved = json.loads(settings.read_text(encoding="utf-8"))
                    assert saved.get("shortcut") == external["shortcut"], "Native copy lost a live external settings edit"
                    assert settings.read_bytes() == external_bytes, "Native copy changed live settings"
                finally:
                    with recording("action", "settings-restore"):
                        for path, content in prior.items():
                            if content is None:
                                path.unlink(missing_ok=True)
                            else:
                                path.write_bytes(content)
            check("Enter copies, hides, restores editor focus and preserves live settings edits", live_settings_copy)

            def reopen():
                measure_picker_open(native, snapshot, shortcut, "smoke000002", report=report)
                wait_for_query(snapshot, "smoke000002", "Ibis smoke reference 000002", report, settled=True)
            check("reopened picker selects previous search", reopen)

            # Keep the existing performance JSON line intact. Opening is a
            # separate metric and also runs for installed --skip-performance
            # builds, where only the costly 100k startup experiment is skipped.
            def picker_opens():
                samples = []
                report["picker_open"] = {"status": "measuring", "samples_ms": samples}
                for _ in range(MEASURED_OPENS):
                    escape_to_hide(native, snapshot, "picker hidden before timed open")
                    native.focus_editor()
                    wait_for(native.editor_focused, "editor focused before timed open")
                    samples.append(measure_picker_open(native, snapshot, shortcut))
                summary = summarize_picker_opens(samples)
                report["picker_open"] = summary
                print("NATIVE PICKER OPEN " + json.dumps(summary), flush=True)
            check("picker shortcut-to-interactive timing reported (Linux only; 100 ms p95 gate not enforced)", picker_opens)

            def reload():
                with recording("action", "fixture-four"):
                    library.write_text(fixture(4), encoding="utf-8")
                wait_for(lambda: shows_count(snapshot(), "4 references"), "file watcher updates reference count")
                query("smoke000003", "Ibis smoke reference 000003")
            check("edited library reloads through native watcher", reload)

            def missing():
                with recording("action", "fixture-missing"):
                    library.unlink()
                try:
                    menu_action(native, "Reload")
                finally:
                    pass
                retained_fragments = {"not_updated": False, "four_references": False}
                def explained():
                    nonlocal retained_fragments
                    current = snapshot()
                    report["missing_library_evidence"] = missing_library_evidence(current, library.name)
                    retained_fragments = retained_snapshot_fragments(current)
                    return all(report["missing_library_evidence"].values())
                try:
                    wait_for(explained, "missing library explains recovery and retained reference count")
                except AssertionError:
                    print("MISSING LIBRARY EVIDENCE " + json.dumps(report.get("missing_library_evidence", {})), flush=True)
                    print("MISSING LIBRARY RETAINED FRAGMENTS " + json.dumps(retained_fragments), flush=True)
                    raise
            check("missing library shows an error", missing)

            def recover():
                with recording("action", "fixture-empty"):
                    library.write_text("", encoding="utf-8")
                menu_action(native, "Reload")
                wait_for(lambda: " · no references" in strings(snapshot()), "empty library count")
                with recording("action", "fixture-small"):
                    library.write_text(fixture(), encoding="utf-8")
                menu_action(native, "Reload")
                wait_for(lambda: shows_count(snapshot(), "3 references"), "reload recovers after error")
            check("empty library and recovery after error", recover)
            stop()

            def damaged():
                with recording("action", "fixture-damaged"):
                    library.write_text((Path(__file__).resolve().parent.parent / "fixtures/bibtex/damaged.bib").read_text(encoding="utf-8"), encoding="utf-8")
                launch("damaged")
                wait_for(lambda: library.name in strings(snapshot()) and "reading notice" in strings(snapshot()),
                         "damaged library starts with a reading notice")
                query("good2", "Loads after the damage")
                stop()
            check("damaged startup library warns and keeps valid records searchable", damaged)

            if args.skip_performance:
                report["performance"] = {"status": "skipped"}
            else:
                with recording("action", "fixture-large"):
                    library.write_text(fixture(100000), encoding="utf-8")
                samples = []
                # Startup medians remain separate from shortcut-to-interactive reporting.
                # Launch 0 warms the OS file cache and is discarded from those medians.
                # Debug launches and accessibility probes diagnose startup; release
                # shortcut/idle limits use named-machine evidence (#1369).
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
                    stop()
                    phases["library_phases"] = library_phase_diagnostics([diagnostic_logs[-1]])
                report["performance"] = summarize_performance(samples)
                print(json.dumps(report["performance"]), flush=True)
        report["status"] = "passed"
        exit_code = 0
    except Unavailable as error:
        unavailable(error.code)
        report.update(status="unavailable", reason=str(error))
        print(f"UNAVAILABLE: {error}", file=sys.stderr)
        exit_code = 2
    except (AssertionError, RuntimeError, ValueError, KeyError, TypeError, subprocess.SubprocessError, OSError) as error:
        failure(error)
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
        if native and hasattr(native, "foreground_trace"):
            report["foreground_trace"] = native.foreground_trace
            report["foreground_trace_capped"] = native.foreground_trace_capped
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
            except (OSError, subprocess.SubprocessError, RuntimeError, AssertionError) as error:
                failure(error, "cleanup")
                report.update(status="failed", reason="Native editor cleanup failed")
                exit_code = 1
        if owned_settings:
            try:
                clean_settings(owned_settings)
            except OSError as error:
                failure(error, "cleanup")
                report.update(status="failed", reason="Disposable settings cleanup failed")
                exit_code = 1
        report["elapsed_seconds"] = round(time.monotonic() - started, 2)
        (args.output / "smoke.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return exit_code


class ContractTests(unittest.TestCase):
    def setUp(self):
        # Metadata must not launch subprocesses through a mocked app launcher,
        # or inspect the developer's desktop during portable contract tests.
        probe = patch("native_evidence._probe", return_value=b"")
        probe.start()
        self.addCleanup(probe.stop)


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

    def linux_click_fixture(self):
        native = Linux.__new__(Linux)
        native.app = Mock(pid=100)
        native.accessibility_error = type("AccessibilityError", (RuntimeError,), {})
        target = self.CensusNode(name="Reload")
        target.queryAction = Mock(return_value=Mock(nActions=1, doAction=Mock(return_value=True)))
        foreign = self.CensusNode(999, "Reload", [target])
        foreign.queryAction = Mock()
        owned = self.CensusNode(100, children=[target])
        desktop = self.CensusNode(children=[foreign, owned])
        native.atspi = Mock(Registry=Mock(getDesktop=lambda _: desktop))
        return native, desktop, owned, target, foreign

    def test_linux_click_skips_stale_discovery_before_owned_target(self):
        from contextlib import nullcontext
        from unittest.mock import PropertyMock
        for point in ("owner", "name", "queryAction", "nActions", "children", "childCount", "entry"):
            with self.subTest(point=point):
                native, desktop, owned, target, foreign = self.linux_click_fixture()
                stale = self.CensusNode(100, "Reload")
                stale.queryAction = Mock(return_value=Mock(nActions=1))
                error = native.accessibility_error("stale accessible")
                attribute = point if point in ("name", "childCount") else "children"
                def read(node):
                    if node is stale:
                        raise error
                    return node.__dict__[attribute]
                if point == "owner":
                    stale.get_process_id = Mock(side_effect=error)
                    desktop.children.insert(0, stale)
                    desktop.childCount += 1
                else:
                    owned.children.append(stale)  # LIFO visits stale before target.
                    owned.childCount += 1
                if point == "queryAction":
                    stale.queryAction.side_effect = error
                if point == "nActions":
                    action = stale.queryAction.return_value
                    type(action).nActions = PropertyMock(side_effect=error)
                if point in ("children", "childCount"):
                    stale.name = "unrelated"
                    stale.childCount = 1
                if point == "entry":
                    original_getitem = self.CensusNode.__getitem__
                    def getitem(node, index):
                        if node is owned and index == 1:
                            raise error
                        return original_getitem(node, index)
                context = patch.object(self.CensusNode, attribute, property(read), create=True) if point in ("name", "children", "childCount") else nullcontext()
                with context, patch.object(self.CensusNode, "__getitem__", getitem) if point == "entry" else nullcontext():
                    native.click("Reload")
                target.queryAction.return_value.doAction.assert_called_once_with(0)
                foreign.queryAction.assert_not_called()

    def test_linux_click_action_metadata_not_implemented_does_not_activate(self):
        from unittest.mock import PropertyMock
        native, desktop, owned, target, foreign = self.linux_click_fixture()
        stale = self.CensusNode(name="Reload")
        stale.queryAction = Mock(return_value=Mock())
        type(stale.queryAction.return_value).nActions = PropertyMock(side_effect=NotImplementedError)
        owned.children.append(stale)
        owned.childCount += 1
        native.click("Reload")
        stale.queryAction.return_value.doAction.assert_not_called()
        target.queryAction.return_value.doAction.assert_called_once_with(0)

    def test_linux_click_rejects_foreign_namesake_without_owned_target(self):
        native, desktop, owned, target, foreign = self.linux_click_fixture()
        owned.children, owned.childCount = [], 0
        for entries in ([foreign, owned], []):
            desktop.children, desktop.childCount = entries, len(entries)
            with self.assertRaisesRegex(AssertionError, "No invokable accessibility control"):
                native.click("Reload")
        target.queryAction.assert_not_called()
        foreign.queryAction.assert_not_called()

    def test_linux_click_keeps_discovered_targets_when_children_shrink(self):
        for level in ("desktop", "owned"):
            with self.subTest(level=level):
                native, desktop, owned, target, foreign = self.linux_click_fixture()
                desktop.children = [owned, foreign]
                owned.children.append(self.CensusNode())
                owned.childCount += 1
                parent = desktop if level == "desktop" else owned
                original_getitem = self.CensusNode.__getitem__
                def getitem(node, index):
                    if node is parent and index == 1:
                        raise IndexError("collection shrank")
                    return original_getitem(node, index)
                with patch.object(self.CensusNode, "__getitem__", getitem):
                    native.click("Reload")
                target.queryAction.return_value.doAction.assert_called_once_with(0)

    def test_linux_menu_action_does_not_retry_uncertain_activation(self):
        native, desktop, owned, target, foreign = self.linux_click_fixture()
        more = self.CensusNode(name="More")
        more.queryAction = Mock(return_value=Mock(nActions=1, doAction=Mock(return_value=True)))
        owned.children.append(more)
        owned.childCount += 1
        target.queryAction.return_value.doAction.side_effect = native.accessibility_error("uncertain activation")
        with self.assertRaises(native.accessibility_error):
            menu_action(native, "Reload")
        more.queryAction.return_value.doAction.assert_called_once_with(0)
        target.queryAction.return_value.doAction.assert_called_once_with(0)
        foreign.queryAction.assert_not_called()

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

    def test_startup_maps_the_picker_before_waiting_for_its_accessible_count(self):
        # WebKitGTK exposes only the application root until the native window
        # is mapped. An --at-login launch cannot satisfy the footer count wait.
        native = Mock(spec=["snapshot", "focus_editor", "editor_focused", "key", "text", "close", "app", "picker_mapped", "picker_has_input_focus"])
        app = Mock(pid=100)
        app.poll.return_value = None
        visible, query, clock, events = [False], [""], [0.0], []

        def launch(command, **_options):
            visible[0] = "--at-login" not in command
            events.append(("launch", visible[0]))
            return app

        def snapshot():
            events.append(("snapshot", visible[0]))
            nodes = [{"role": "application"}]
            if visible[0]:
                nodes += [{"role": "combo box", "name": "Search references", "value": query[0], "focused": True},
                          {"role": "status", "name": "3 references"}]
                if query[0]:
                    nodes += [{"role": "status", "name": "1 result"},
                              {"role": "list item", "name": "Ibis smoke reference 000000"}]
            return {"visible": visible[0], "foreground": visible[0], "nodes": nodes}

        def key(chord):
            events.append(("key", chord))
            visible[0] = chord != "Escape"

        def text(value):
            events.append(("text", value))
            query[0] = value

        native.snapshot.side_effect = snapshot
        native.picker_mapped.side_effect = lambda: events.append(("mapped", visible[0])) or visible[0]
        native.picker_has_input_focus.side_effect = lambda: visible[0]
        native.key.side_effect = key
        native.text.side_effect = text
        native.editor_focused.return_value = True
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            executable, output = folder / "synthetic-app", folder / "output"
            executable.touch()
            with patch("sys.argv", ["smoke", "--app", str(executable), "--output", str(output)]), \
                 patch("platform.system", return_value="Linux"), patch(__name__ + ".Linux", return_value=native), \
                 patch("subprocess.Popen", side_effect=launch), patch("builtins.print"), \
                 patch("time.monotonic", side_effect=lambda: clock[0]), \
                 patch("time.sleep", side_effect=lambda interval: clock.__setitem__(0, clock[0] + interval)), \
                 patch(__name__ + ".require_tree", side_effect=AssertionError("synthetic stop after first shortcut")), \
                 patch(__name__ + ".linux_failure_census", return_value={}):
                self.assertEqual(main(), 1)
            report = json.loads((output / "smoke.json").read_text())
        self.assertEqual(report["reason"], "synthetic stop after first shortcut")
        self.assertEqual([check["name"] for check in report["checks"]],
                         ["startup", "first shortcut after startup keeps keystrokes from mapping"])
        self.assertEqual(events[:2], [("launch", True), ("snapshot", True)])
        shortcut = events.index(("key", "ctrl+shift+y"))
        self.assertIn(("key", "Escape"), events[:shortcut])
        self.assertEqual(events[shortcut + 1:shortcut + 3], [("mapped", True), ("text", "smoke000000")])
        self.assertNotIn("picker_open", report)  # The 20-open measurement runs later.
        app.terminate.assert_called_once()
        native.close.assert_called_once()

    def shortcut_focus_probe(self, *, loses_focus=False, loses_selection=False, failure_at=2):
        native = Mock(spec=["snapshot", "focus_editor", "editor_focused", "editor_active",
                            "editor_has_input_focus", "key", "text", "close", "app", "skips_taskbar", "picker_mapped", "picker_has_input_focus"])
        app, second = Mock(pid=100), Mock(pid=101)
        app.poll.return_value = None
        second.wait.return_value = 0
        second.poll.return_value = 0
        clock, events = self.CopyClock(), []
        state = {"visible": True, "foreground": True, "query": "", "saved": "",
                 "selected": False, "search_owns_keys": True, "opens": 0}

        def show():
            state.update(visible=True, foreground=True)
            state["query"] = state["query"] or state["saved"]
            state["opens"] += 1
            state["selected"] = not (loses_selection and state["opens"] >= failure_at)
            state["search_owns_keys"] = not (loses_focus and state["opens"] >= failure_at)

        def launch(command, **_options):
            if events:
                events.append(("second-launch",))
                show()
                return second
            events.append(("launch",))
            return app

        def snapshot():
            query = state["query"]
            nodes = [{"role": "combo box", "name": "Search references", "value": query,
                      "focused": not query},
                     {"role": "status", "name": "3 references"},
                     {"role": "list box", "name": "Search results"},
                     {"role": "list", "name": "Results"}]
            if query:
                nodes += [{"role": "status", "name": "1 result"},
                          {"role": "list item", "name": "Ibis smoke reference " + query.removeprefix("smoke"),
                           "focused": True}]
            return {"visible": state["visible"], "foreground": state["foreground"], "nodes": nodes}

        def focus_editor():
            events.append(("focus-editor",))
            state["foreground"] = False

        def key(chord):
            events.append(("key", chord))
            if chord == "ctrl+shift+y":
                show()
            elif chord == "Escape":
                if state["query"]:
                    state["saved"], state["query"] = state["query"], ""
                else:
                    state["visible"] = False
            elif chord == "ctrl+a":
                state["selected"] = True

        def text(value):
            events.append(("text", value))
            if state["visible"] and state["foreground"] and state["search_owns_keys"]:
                state["query"] = value if state["selected"] else state["query"] + value
                state["selected"] = False

        native.snapshot.side_effect = snapshot
        native.picker_mapped.side_effect = lambda: state["visible"]
        native.picker_has_input_focus.side_effect = lambda: state["visible"] and state["foreground"]
        native.focus_editor.side_effect = focus_editor
        native.editor_focused.return_value = True
        native.editor_active.side_effect = lambda: not state["foreground"]
        native.editor_has_input_focus.side_effect = lambda: not state["foreground"]
        native.key.side_effect = key
        native.text.side_effect = text
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            executable, output = folder / "synthetic-app", folder / "output"
            executable.touch()
            with patch("sys.argv", ["smoke", "--app", str(executable), "--output", str(output)]), \
                 patch("platform.system", return_value="Linux"), patch(__name__ + ".Linux", return_value=native), \
                 patch("subprocess.Popen", side_effect=launch), patch("builtins.print"), \
                 patch("time.monotonic", clock.monotonic), \
                 patch("time.sleep", lambda interval: setattr(clock, "elapsed", clock.elapsed + interval)), \
                 patch(__name__ + ".menu_escape_focus", side_effect=AssertionError("synthetic stop after second launch")), \
                 patch(__name__ + ".linux_failure_census", return_value={}):
                self.assertEqual(main(), 1)
            report = json.loads((output / "smoke.json").read_text())
        return report, events

    def test_shortcuts_and_second_launch_prove_search_ownership_with_virtual_result_focus(self):
        report, events = self.shortcut_focus_probe()
        self.assertEqual(report["reason"], "synthetic stop after second launch")
        checks = [check["name"] for check in report["checks"]]
        self.assertIn("global shortcut shows foreground picker with focused search", checks)
        self.assertIn("second launch exits and activates existing picker", checks)
        self.assertFalse(report["last_native_state"]["search_focused"])
        self.assertEqual(report["last_native_state"]["focused_roles"], ["list item"])
        second_launch = events.index(("second-launch",))
        self.assertNotIn(("key", "ctrl+a"), events[:second_launch])
        self.assertEqual([event for event in events[:second_launch] if event[0] == "text"],
                         [("text", "smoke000000"), ("text", "smoke000001"), ("text", "smoke000000")])
        self.assertEqual(events[second_launch + 1], ("text", "smoke000002"))

    def test_reopen_with_real_result_focus_or_unselected_query_still_fails(self):
        for failure in ("loses_focus", "loses_selection"):
            for failure_at in (2, 3, 4):  # Visible reactivation, hidden reopen, second launch.
                with self.subTest(failure=failure, failure_at=failure_at):
                    report, events = self.shortcut_focus_probe(**{failure: True}, failure_at=failure_at)
                    self.assertIn("reopened search receives typing and replaces the previous query", report["reason"])
                    self.assertNotIn("second launch exits and activates existing picker",
                                     [check["name"] for check in report["checks"]])
                    self.assertEqual([event for event in events if event[0] == "text"],
                                     [("text", text) for text in ("smoke000000", "smoke000001", "smoke000000",
                                                                 "smoke000002")[:failure_at]])
                    self.assertNotIn(("key", "ctrl+a"), events)

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


    def preference_probe(self, failure=None, replay=None):
        clock, launches, actions = [0.0], [], []
        native = Mock()
        native.query = ""
        native.query_ready_at = 0
        native.value = ""
        native.visible = True
        native.reopened = False
        native.more_open = False
        native.format_filter = None

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

        def choose(name):
            if name == "Typst":
                settings = Path(launches[0]["XDG_CONFIG_HOME"]) / "com.ibiscite.ibis/settings.json"
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

        def click(name):
            actions.append((name, len(launches)))
            if name == "Quit Ibis":
                native.more_open = False
                if failure == "late-driver":
                    clock[0] += 11
                if failure != "stuck-quit":
                    native.app.returncode = 7 if failure == "nonzero-quit" else 0
                if failure == "uncertain-quit":
                    raise RuntimeError("Synthetic uncertain activation")
            elif name == "More":
                native.more_open = True
            elif name == "Copy as Pandoc key":
                self.assertEqual(native.query, NO_MATCH, "Format selection needs no highlighted result")
                native.format_filter = ""
            else:
                raise AssertionError(f"No invokable accessibility control {name!r}")

        def key(chord):
            if chord == "Return" and native.format_filter is not None:
                actions.append(("Return in Copy as", len(launches)))
                actions.append((native.format_filter, len(launches)))
                choose(native.format_filter)
                native.format_filter = None
                return
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
                names += ["Where do your references live?"]
            else:
                names += ["Copy as Pandoc key", "3 references"]
                if native.format_filter is not None:
                    names += ["Typst key"]
                names += ["No matching references"] if native.query == NO_MATCH else [
                    "Copy as Pandoc", "1 result" if clock[0] >= native.query_ready_at else "3 results", "Ibis smoke reference 000001"]
            result_focus = failure == "active-result-reopen" and native.reopened
            menu_open = native.format_filter is not None
            return {"visible": native.visible, "foreground": native.visible, "nodes": [
                {"name": name, "role": "button"} for name in names] + [
                {"name": "\\cite", "role": "menu item"}] * (native.format_filter == "") + [
                {"name": "Search references", "role": "entry", "value": native.query, "focused": not result_focus and not menu_open},
                {"name": "private result payload", "role": "list item", "focused": result_focus}]}

        def text(value):
            if native.format_filter is not None:
                native.format_filter = value
                return
            native.query = value
            if failure == "delayed-query":
                native.query_ready_at = clock[0] + 0.2

        native.click.side_effect = click
        native.key.side_effect = key
        native.text.side_effect = text
        native.snapshot.side_effect = snapshot
        native.set_clipboard.side_effect = lambda value: setattr(native, "value", value)
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
            args = argparse.Namespace(app=app, output=root / "report", identifier="com.ibiscite.ibis", shortcut=None,
                                      replay_bundle=replay)
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
                    patch.dict(os.environ, {"IBIS_LIBRARY": "/unowned.bib"}), \
                    patch("builtins.print"):
                result = first_run_preferences_main(args)
            report = json.loads((args.output / "smoke.json").read_text(encoding="utf-8"))
            manifests = list(args.output.glob("evidence-*/manifest.json"))
            self.assertEqual(len(manifests), 1)
            report["evidence"] = load_bundle(manifests[0])
            self.assertEqual(report["evidence"]["result"]["outcome"], {0: "pass", 1: "fail", 2: "unavailable"}[result])
            self.assertNotIn("private result payload", json.dumps(report["evidence"]))
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
        self.assertEqual([action for action in actions if action[0] in ("Copy as Pandoc key", "Typst")], [("Copy as Pandoc key", 2), ("Typst", 2)])
        pick = actions.index(("Typst", 2))
        self.assertEqual(actions[pick - 2:pick], [("Down", 2), ("Return in Copy as", 2)], "Typst was not picked from the filter")
        self.assertEqual([action for action in actions if action[0] == "Quit Ibis"], [("Quit Ibis", 1), ("Quit Ibis", 2)])

    def test_first_run_bundle_replays_actions_and_failed_invariants(self):
        for error in (None, "wrong-copy", "delayed-query"):
            with self.subTest(error=error):
                result, report, _, actions = self.preference_probe(error)
                replay_result, replay_report, _, replay_actions = self.preference_probe(error, report["evidence"])
                self.assertEqual(replay_result, result)
                self.assertEqual(replay_report["evidence"]["result"], report["evidence"]["result"])
                self.assertEqual(replay_actions, actions)
                self.assertEqual(replay_report["evidence"]["replay_of"], report["evidence"]["run_id"])

    def test_cli_restores_replay_options_and_refuses_old_format_before_native_work(self):
        _, report, _, _ = self.preference_probe()
        bundle = report["evidence"]
        bundle["options"].update(seed=37, shortcut="ctrl+super+r", identifier="com.ibiscite.ibis.nightly")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / "manifest.json"
            manifest.write_text(json.dumps(bundle), encoding="utf-8")
            command = ["smoke", "--app", str(root / "synthetic-app"), "--output", str(root / "output"),
                       "--replay", str(manifest)]
            with patch("sys.argv", command), patch(__name__ + ".first_run_preferences_main", return_value=0) as scenario:
                self.assertEqual(main(), 0)
                restored = scenario.call_args.args[0]
                self.assertEqual(restored.seed, 37)
                self.assertEqual(restored.shortcut, "ctrl+super+r")
                self.assertEqual(restored.identifier, "com.ibiscite.ibis.nightly")
            bundle["schema_version"] = 0
            manifest.write_text(json.dumps(bundle), encoding="utf-8")
            message = io.StringIO()
            with patch("sys.argv", command), patch(__name__ + ".first_run_preferences_main") as scenario, redirect_stderr(message):
                self.assertEqual(main(), 1)
                scenario.assert_not_called()
            self.assertIn("Unsupported evidence schema version", message.getvalue())

    def test_cli_native_setup_refusal_is_a_replayable_harness_failure(self):
        for first_run in (False, True):
            with self.subTest(first_run=first_run), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                app = root / "synthetic-app"
                app.touch()
                original = None
                manifest = None
                for name in ("original", "replay"):
                    output = root / name
                    command = ["smoke", "--app", str(app), "--output", str(output), "--skip-performance"]
                    if first_run:
                        command.append("--first-run-preferences")
                    if manifest:
                        command += ["--replay", str(manifest)]
                    with patch("sys.argv", command), patch("platform.system", return_value="Linux"), \
                            patch(__name__ + ".Linux", side_effect=AssertionError("PRIVATE synthetic setup failure")), \
                            patch("subprocess.Popen") as launch, patch("builtins.print") as printed:
                        self.assertEqual(main(), 1)
                        launch.assert_not_called()
                    manifest = next(output.glob("evidence-*/manifest.json"))
                    bundle = load_bundle(manifest)
                    self.assertEqual(bundle["result"], {"outcome": "fail", "failure_kind": "harness",
                                                       "reason": "harness-failed", "failed_invariant": "scenario"})
                    self.assertEqual(bundle["events"], [])
                    self.assertEqual(bundle["missing_capabilities"], [])
                    self.assertNotIn("PRIVATE", json.dumps(bundle) + str(printed.call_args_list))
                    if original:
                        self.assertEqual(bundle["result"], original["result"])
                        self.assertEqual(bundle["replay_of"], original["run_id"])
                    else:
                        original = bundle

    def test_post_launch_product_assertion_keeps_product_classification_and_cleanup(self):
        result, report, launches, _actions = self.preference_probe("wrong-copy")
        self.assertEqual(result, 1)
        self.assertEqual(len(launches), 2)
        self.assertEqual(report["evidence"]["result"]["failure_kind"], "product")

    def test_cli_missing_build_emits_and_replays_unavailable_bundle(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            command = ["smoke", "--app", str(root / "missing-app"), "--output", str(root / "output")]
            with patch("sys.argv", command), patch("platform.system", return_value="Linux"), \
                    patch(__name__ + ".Linux") as driver, patch("builtins.print"):
                self.assertEqual(main(), 2)
                driver.assert_not_called()
            manifest = next((root / "output").glob("evidence-*/manifest.json"))
            bundle = load_bundle(manifest)
            self.assertEqual(bundle["result"]["outcome"], "unavailable")
            self.assertEqual(bundle["missing_capabilities"], ["app-build"])
            command[4] = str(root / "replay")
            with patch("sys.argv", command + ["--replay", str(manifest)]), \
                    patch("platform.system", return_value="Linux"), patch(__name__ + ".Linux") as driver, patch("builtins.print"):
                self.assertEqual(main(), 2)
                driver.assert_not_called()
            replay = load_bundle(next((root / "replay").glob("evidence-*/manifest.json")))
            self.assertEqual(replay["result"], bundle["result"])

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


    def test_first_run_preferences_refuse_existing_settings_and_dangling_links(self):
        for failure in ("preexisting", "symlink"):
            with self.subTest(failure=failure):
                result, report, launches, _actions = self.preference_probe(failure)
                self.assertEqual((result, report["status"]), (2, "unavailable"))
                self.assertEqual(launches, [])

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
                patch.object(Linux, "window_manager_ready", check), patch.object(Linux, "window", return_value="42"), \
                patch.object(Linux, "editor_focused", return_value=True), patch("time.sleep"):
            native = Linux()
            self.addCleanup(native.close)
        self.assertEqual(events, ["openbox", "ready?", "ready?", "ready?", "xterm"])

    def test_editor_records_opening_keys_without_waiting_for_a_newline(self):
        import pty
        import termios
        master, slave = pty.openpty()
        with tempfile.NamedTemporaryFile() as received:
            editor = subprocess.Popen([sys.executable, "-c", SYNTHETIC_EDITOR_SCRIPT, received.name],
                                      stdin=slave, stdout=slave, stderr=slave)
            try:
                wait_for(lambda: not termios.tcgetattr(slave)[3] & termios.ICANON,
                         "synthetic editor reads individual keys", timeout=5, interval=.005)
                os.write(master, "s引用".encode("utf-8"))
                native = Linux.__new__(Linux)
                native.editor, native.editor_input = editor, received
                expected = {"available": True, "text": "s引用", "truncated": False}
                wait_for(lambda: native.editor_received() == expected,
                         "synthetic editor records keys without Return", timeout=5, interval=.005)
            finally:
                editor.terminate()
                editor.wait(timeout=5)
                os.close(master)
                os.close(slave)

    def test_editor_diagnostics_are_bounded_and_unavailable_after_exit(self):
        native = Linux.__new__(Linux)
        native.editor = Mock(poll=Mock(return_value=None))
        native.editor_input = io.BytesIO(b"s" * 5000)
        self.assertEqual(native.editor_received(), {"available": True, "text": "s" * 4096, "truncated": True})
        native.editor.poll.return_value = 0
        self.assertEqual(native.editor_received(), {"available": False})

    def test_mapping_probe_checks_only_a_viewable_picker_window(self):
        native = Linux.__new__(Linux)
        native.app = Mock(pid=100)
        native.window = Mock(side_effect=[None, "123"])
        self.assertFalse(native.picker_mapped())
        self.assertTrue(native.picker_mapped())
        self.assertEqual(native.window.call_args_list, [call(100, PICKER_TITLE), call(100, PICKER_TITLE)])

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

    def copy_setup(self, step=10):
        native = Mock()
        native.snapshot.return_value = self.copy_snapshot()
        return native, {}, self.CopyClock(step)

    def copy_time(self, clock):
        return patch.multiple(time, monotonic=clock.monotonic, sleep=clock.sleep)

    def test_native_copy_accepts_a_later_equality_poll_without_another_return(self):
        native, report, clock = self.copy_setup(19)
        sentinel = "Ibis native smoke clipboard sentinel"
        native.clipboard.side_effect = [sentinel, sentinel, "[@smoke000001]"]
        with self.copy_time(clock):
            copy_with_diagnostics(native, report)
        self.assertEqual(native.key.call_args_list, [call("Return")])
        self.assertEqual(native.clipboard.call_count, 3)
        self.assertEqual(native.snapshot.call_count, 1)
        self.assertNotIn("copy_failure", report)
        self.assertEqual(native.mock_calls[:4], [call.set_clipboard(sentinel), call.clipboard(), call.snapshot(), call.key("Return")])

    def test_return_waits_until_the_picker_owns_the_keyboard(self):
        native, report, clock = self.copy_setup(1)
        sentinel = "Ibis native smoke clipboard sentinel"
        native.clipboard.side_effect = [sentinel, "[@smoke000001]"]
        native.picker_key.side_effect = [False, False, True]
        with self.copy_time(clock):
            copy_with_diagnostics(native, report, native.picker_key)
        self.assertEqual(native.mock_calls[:7], [call.set_clipboard(sentinel), call.clipboard(), call.snapshot(),
                                                 call.picker_key(), call.picker_key(), call.picker_key(), call.key("Return")])
        self.assertNotIn("copy_failure", report)

    def test_return_is_never_sent_while_another_window_owns_the_keyboard(self):
        native, report, clock = self.copy_setup()
        native.clipboard.return_value = "Ibis native smoke clipboard sentinel"
        native.picker_key.return_value = False
        with self.copy_time(clock), \
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
                native, report, clock = self.copy_setup()
                native.clipboard.return_value = "Ibis native smoke clipboard sentinel"
                errors = io.StringIO()
                with self.copy_time(clock), \
                     patch("sys.stderr", errors), self.assertRaisesRegex(AssertionError, "expected citation on OS clipboard"):
                    copy_with_diagnostics(native, report, probe)
                self.assertIs(report["copy_failure"]["key_window_after_timeout"], expected)
                self.assertEqual(probe.call_count, 2)
                self.assertEqual(native.key.call_args_list, [call("Return")])
                self.assertNotIn("private", errors.getvalue() + json.dumps(report))

    def test_native_copy_timeout_records_fresh_state_without_reading_clipboard_again(self):
        native, report, clock = self.copy_setup()
        report["last_native_state"] = {"visible": True}
        sentinel = "Ibis native smoke clipboard sentinel"
        native.clipboard.side_effect = [sentinel, sentinel, sentinel, "[@smoke000001]"]
        native.snapshot.side_effect = [self.copy_snapshot(), self.copy_snapshot(visible=False, focused=False, error=True)]
        with self.copy_time(clock), \
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
                    with self.copy_time(clock), \
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
        native, report, clock = self.copy_setup()
        reads = iter(["Ibis native smoke clipboard sentinel", "[@smoke000001]"])
        def clipboard():
            value = next(reads)
            if value == "[@smoke000001]":
                clock.elapsed = 25  # The existing wait accepts this completed read.
            return value
        native.clipboard.side_effect = clipboard
        with self.copy_time(clock):
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
                native, report, clock = self.copy_setup()
                native.clipboard.side_effect = ["Ibis native smoke clipboard sentinel", clipboard, clipboard]
                native.snapshot.return_value = {"visible": True, "nodes": [
                    {"name": "Search references", "role": "AXTextField", "value": "private query", "focused": True},
                    {"name": "private Ibis smoke reference 000001 suffix"},
                    {"name": "21 results"},
                    {"name": "private Nothing was copied. Try again, or use Reload. suffix"},
                ]}
                errors = io.StringIO()
                with self.copy_time(clock), \
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
        def __init__(self, visible=True, search_ready=True, accepts_escape=True, owns_keys=True):
            self.visible, self.key_window = visible, visible and owns_keys
            self.search_ready, self.accepts_escape = search_ready, accepts_escape
            self.events = []

        def snapshot(self):
            return {"visible": self.visible, "nodes": [{
                "name": "Search references", "focused": self.search_ready,
            }]}

        def picker_has_input_focus(self):
            return self.key_window

        def focus_editor(self):
            # The editor can already be frontmost while the panel owns keys.
            self.events.append(("editor", self.visible))

        def key(self, key):
            self.events.append((key, self.visible))
            if key == "Escape" and self.accepts_escape and self.key_window:
                self.visible = self.key_window = False
            elif key == "ctrl+super+r":
                self.visible = not (self.visible and self.key_window)
                self.key_window = self.visible

    def linux_window_query(self, *, visible="1\n2\n3", managed="0x2, 0x3", names=None, owners=None, fail=None):
        names = names or {"1": "", "2": "Ibis", "3": NOTICE_TITLE}
        owners = owners or {"1": "42", "2": "42", "3": "42"}
        def command(*args, timeout=20):
            if fail and fail(args):
                raise subprocess.CalledProcessError(1, args, stderr="private provider failure")
            if args == ("xprop", "-root", "_NET_CLIENT_LIST"):
                return "_NET_CLIENT_LIST(WINDOW): window id # " + managed
            if args[:2] == ("xdotool", "search"):
                return visible
            if args[:2] == ("xdotool", "getwindowname"):
                return names[args[2]]
            if args[:2] == ("xdotool", "getwindowpid"):
                return owners[args[2]]
            self.fail("Unexpected native query: " + repr(args))
        return command

    def test_linux_window_ignores_auxiliary_and_notice_regardless_of_order(self):
        linux = Linux.__new__(Linux)
        for visible in ("1\n2\n3", "3\n1\n2", "2\n3\n1"):
            with self.subTest(visible=visible), patch(__name__ + ".run", side_effect=self.linux_window_query(visible=visible)):
                self.assertEqual(linux.window(42, "Ibis"), "2")

    def linux_window_transition(self, first, second):
        # A new search starts a new observation, never completes the old one.
        searches = []
        def command(*args, timeout=20):
            if args[:2] == ("xdotool", "search"):
                searches.append(args)
            return (first if len(searches) == 1 else second)(*args, timeout=timeout)
        return command, searches

    @contextmanager
    def linux_window_deadline(self):
        clock = self.CopyClock()
        with patch("time.monotonic", clock.monotonic), patch("time.sleep", clock.sleep):
            yield clock

    def test_linux_window_reobserves_not_yet_managed_title_without_using_stale_identity(self):
        linux = Linux.__new__(Linux)
        linux.app = Mock(pid=42)
        linux.atspi = Mock()
        linux.atspi.Registry.getDesktop.return_value = []
        first = self.linux_window_query(visible="2", managed="0x3", names={"2": PICKER_TITLE})
        second = self.linux_window_query(visible="4", managed="0x4", names={"4": PICKER_TITLE}, owners={"4": "42"})
        command, searches = self.linux_window_transition(first, second)
        def active(*args, timeout=20):
            if args == ("xdotool", "getactivewindow"):
                return "4"
            return command(*args, timeout=timeout)
        with self.linux_window_deadline() as clock, patch(__name__ + ".run", side_effect=active) as queries:
            current = linux.snapshot()  # A direct caller, outside wait_for.
        self.assertTrue(current["visible"] and current["foreground"])
        self.assertEqual(len(searches), 2)
        self.assertEqual(clock.elapsed, 10)
        self.assertEqual([entry.kwargs["timeout"] for entry in queries.call_args_list
                          if entry.args[:2] == ("xdotool", "search")], [20, 10])

    def test_linux_window_reobserves_an_auxiliary_title_changing_during_mapping(self):
        linux = Linux.__new__(Linux)
        first = self.linux_window_query(names={"1": "", "2": "Synthetic mapping title", "3": NOTICE_TITLE})
        command, searches = self.linux_window_transition(first, self.linux_window_query())
        with patch(__name__ + ".run", side_effect=command), patch("time.sleep"):
            self.assertEqual(linux.window(42, PICKER_TITLE), "2")
        self.assertEqual(len(searches), 2)

    def test_linux_window_reenumerates_after_close_and_reopen_between_queries(self):
        linux = Linux.__new__(Linux)
        for point in ("owner", "title", "recheck-owner", "recheck-title", "changed-title"):
            calls = {"getwindowpid": 0, "getwindowname": 0}
            original = self.linux_window_query(visible="2", managed="0x2")
            def closed(*args, timeout=20):
                if args[:2] in (("xdotool", "getwindowpid"), ("xdotool", "getwindowname")):
                    calls[args[1]] += 1
                    operation = "getwindowpid" if point.endswith("owner") else "getwindowname"
                    if point != "changed-title" and args[1] == operation and calls[operation] == (2 if point.startswith("recheck") else 1):
                        raise subprocess.CalledProcessError(1, args, stderr="Synthetic destroyed window")
                    if point == "changed-title" and args[1] == "getwindowname" and calls[args[1]] == 2:
                        return "Synthetic changed title"
                return original(*args, timeout=timeout)
            reopened = self.linux_window_query(visible="4", managed="0x4", names={"4": PICKER_TITLE}, owners={"4": "42"})
            command, searches = self.linux_window_transition(closed, reopened)
            with self.subTest(point=point), self.linux_window_deadline(), patch(__name__ + ".run", side_effect=command):
                self.assertEqual(linux.window(42, PICKER_TITLE), "4")
                self.assertEqual(len(searches), 2)

    def test_linux_window_persistent_mismatch_names_the_branch_without_desktop_text(self):
        linux = Linux.__new__(Linux)
        clock = self.CopyClock()
        command = self.linux_window_query(visible="2", managed="0x2", names={"2": "PRIVATE_TITLE"})
        with patch("time.monotonic", clock.monotonic), patch("time.sleep", clock.sleep), \
             patch(__name__ + ".run", side_effect=command) as queries, \
             patch("sys.stderr", new_callable=io.StringIO) as output, self.assertRaises(WindowObservationFailure):
            linux.window(42, PICKER_TITLE)
        self.assertEqual(clock.elapsed, 20)
        self.assertEqual(sum(args[0][:2] == ("xdotool", "search") for args in queries.call_args_list), 2)
        self.assertIn("LINUX WINDOW OBSERVATION FAILURE unexpected-managed-title", output.getvalue())
        self.assertNotIn("PRIVATE", output.getvalue())

    def test_linux_window_failed_queries_name_fixed_branches(self):
        linux = Linux.__new__(Linux)
        for failed, branch in ((("xdotool", "search", "--onlyvisible", "--pid", "42"), "search-query"),
                (("xprop", "-root", "_NET_CLIENT_LIST"), "managed-query"),
                (("xdotool", "getwindowpid", "2"), "owner-query"),
                (("xdotool", "getwindowname", "2"), "title-query")):
            clock = self.CopyClock()
            with self.subTest(branch=branch), patch("time.monotonic", clock.monotonic), patch("time.sleep", clock.sleep), \
                 patch(__name__ + ".run", side_effect=self.linux_window_query(fail=lambda args: args == failed)), \
                 patch("sys.stderr", new_callable=io.StringIO) as output, self.assertRaises(WindowObservationFailure):
                linux.window(42, PICKER_TITLE)
            self.assertIn("LINUX WINDOW OBSERVATION FAILURE " + branch, output.getvalue())
            self.assertNotIn("private", output.getvalue())

    def test_linux_window_refuses_wrong_owner_unmanaged_or_ambiguous_clients(self):
        linux = Linux.__new__(Linux)
        cases = [
            {"owners": {"1": "42", "2": "99", "3": "42"}},
            {"managed": "0x1, 0x3"},
            {"names": {"1": "", "2": "Ibis", "3": "Ibis"}},
            {"names": {"1": "", "2": "", "3": NOTICE_TITLE}},
            {"visible": "2", "managed": "0x3", "names": {"2": "Ibis"}},
            {"visible": "2", "managed": "0x2", "names": {"2": "Unexpected title"}},
        ]
        for case in cases:
            with self.subTest(case=case), self.linux_window_deadline(), \
                 patch(__name__ + ".run", side_effect=self.linux_window_query(**case)), self.assertRaises(WindowObservationFailure):
                linux.window(42, "Ibis")

    def test_linux_window_refuses_io_uncertainty_and_does_not_cache_closed_identity(self):
        linux = Linux.__new__(Linux)
        for failed in (("xprop", "-root", "_NET_CLIENT_LIST"), ("xdotool", "getwindowname", "2"),
                       ("xdotool", "getwindowpid", "2")):
            with self.subTest(failed=failed), self.linux_window_deadline(), \
                 patch(__name__ + ".run", side_effect=self.linux_window_query(fail=lambda args: args == failed)), self.assertRaises(WindowObservationFailure):
                linux.window(42, "Ibis")
        with patch(__name__ + ".run", side_effect=self.linux_window_query()):
            self.assertEqual(linux.window(42, "Ibis"), "2")
        with patch(__name__ + ".run", side_effect=self.linux_window_query(visible="3", managed="0x3")):
            self.assertIsNone(linux.window(42, "Ibis"))
        with patch(__name__ + ".run", side_effect=self.linux_window_query(visible="4", managed="0x4", names={"4": "Ibis"}, owners={"4": "42"})):
            self.assertEqual(linux.window(42, "Ibis"), "4")

    def test_linux_unknown_window_cannot_satisfy_the_real_escape_hidden_check(self):
        linux = Linux.__new__(Linux)
        linux.app = Mock(pid=42)
        linux.atspi = Mock()
        linux.atspi.Registry.getDesktop.return_value = []
        linux.key = Mock()  # Escape does not hide either live managed client.
        command = self.linux_window_query(names={"1": "", "2": "Ibis", "3": "Ibis"})
        def active(*args, timeout=20):
            if args == ("xdotool", "getactivewindow"):
                return "2"
            return command(*args, timeout=timeout)
        with self.linux_window_deadline(), patch(__name__ + ".run", side_effect=active), self.assertRaises(WindowObservationFailure):
            escape_to_hide(linux, linux.snapshot, "ambiguous picker must not count as hidden")
        linux.key.assert_not_called()

    def test_linux_completed_absence_is_distinct_from_uncertain_search(self):
        linux = Linux.__new__(Linux)
        for error in (subprocess.CalledProcessError(1, "synthetic", output="", stderr=""), None):
            with patch(__name__ + ".run", side_effect=error, return_value=""):
                self.assertIsNone(linux.window(42, "Ibis"))
        for error in (subprocess.CalledProcessError(1, "synthetic", output="", stderr="private display failure"),
                      subprocess.CalledProcessError(2, "synthetic", output="", stderr="")):
            with self.linux_window_deadline(), patch(__name__ + ".run", side_effect=error), self.assertRaises(WindowObservationFailure) as caught:
                linux.window(42, "Ibis")
            self.assertNotIn("private", str(caught.exception))

    def test_linux_unknown_visibility_waits_for_evidence_without_passing_hide(self):
        clock = self.CopyClock()
        clock.step = 0
        def sleep(interval):
            clock.elapsed += interval
        unknown = WindowObservationFailure()
        with patch("time.monotonic", clock.monotonic), patch("time.sleep", side_effect=sleep):
            probe = Mock(side_effect=[unknown, True])
            wait_for(probe, "observed after mapping", timeout=1)
            self.assertEqual(probe.call_count, 2)
            probe = Mock(side_effect=unknown)
            with self.assertRaisesRegex(AssertionError, "Timed out: hiding remains unproved"):
                wait_for(probe, "hiding remains unproved", timeout=.2)
            self.assertGreaterEqual(probe.call_count, 1)

    def test_linux_window_title_is_explicit_for_synthetic_editor(self):
        linux = Linux.__new__(Linux)
        command = self.linux_window_query(names={"1": "", "2": "Ibis synthetic editor", "3": NOTICE_TITLE})
        with patch(__name__ + ".run", side_effect=command):
            self.assertEqual(linux.window(42, "Ibis synthetic editor"), "2")

    def test_linux_managed_client_property_is_strict_and_bounded(self):
        prefix = "_NET_CLIENT_LIST(WINDOW): window id # "
        self.assertEqual(managed_client_windows(prefix + "0x2, 0x00000003"), {2, 3})
        self.assertEqual(managed_client_windows(prefix), set())
        for line in ("_NET_CLIENT_LIST: not found.", prefix + "0x2, private", prefix + "0x100000000",
                     prefix + ", ".join(hex(n) for n in range(CENSUS_WINDOWS + 1))):
            self.assertIsNone(managed_client_windows(line))

    def test_linux_window_queries_share_one_deadline(self):
        linux = Linux.__new__(Linux)
        remaining = []
        command = self.linux_window_query()
        def bounded(*args, timeout):
            remaining.append(timeout)
            return command(*args, timeout=timeout)
        clock = iter([0, 0, 5, 10, 19])
        with patch("time.monotonic", side_effect=lambda: next(clock, 20)), \
             patch(__name__ + ".run", side_effect=bounded), self.assertRaises(WindowObservationFailure):
            linux.window(42, "Ibis")
        self.assertEqual(remaining, [20, 15, 10, 1])

    def test_linux_window_refuses_replaced_owner_and_subprocess_timeout(self):
        linux = Linux.__new__(Linux)
        command = self.linux_window_query()
        owners = iter(["42"])
        def replaced(*args, timeout):
            if args == ("xdotool", "getwindowpid", "2"):
                return next(owners, "99")
            return command(*args, timeout=timeout)
        with self.linux_window_deadline(), patch(__name__ + ".run", side_effect=replaced), self.assertRaises(WindowObservationFailure):
            linux.window(42, "Ibis")
        for error in (subprocess.TimeoutExpired("synthetic", 20), OSError("private native failure")):
            with self.subTest(error=type(error).__name__), self.linux_window_deadline(), \
                 patch(__name__ + ".run", side_effect=error), self.assertRaises(WindowObservationFailure):
                linux.window(42, "Ibis")

    def test_linux_window_selection_does_not_depend_on_the_expected_hints(self):
        linux = Linux.__new__(Linux)
        linux.app = Mock(pid=42)
        command = self.linux_window_query()
        def missing_hint(*args, timeout=20):
            if args == ("xprop", "-id", "2", "_NET_WM_STATE"):
                return "_NET_WM_STATE: not found."
            return command(*args, timeout=timeout)
        with patch(__name__ + ".run", side_effect=missing_hint):
            self.assertFalse(linux.skips_taskbar())
        self.assertEqual(linux.last_window_state, "2: _NET_WM_STATE: not found.")

    def linux_editor_startup(self, *, never_ready=False):
        events, state = [], {"mapped": never_ready, "focused": False}
        wm, editor = Mock(pid=55), Mock(pid=100)
        wm.poll.return_value = editor.poll.return_value = None
        editor_input = Mock(name="synthetic-editor-input")
        bridge = Mock()
        bridge.GLib.Error = RuntimeError
        def spawn(command, **_options):
            if command[0] == "openbox":
                return wm
            if command[0] == "xterm":
                events.append("editor-launch")
                return editor
            self.assertTrue(state["mapped"] and state["focused"], "Ibis launched before its synthetic editor settled")
            events.append("app-launch")
            return Mock()
        def mapped(_pid, title):
            self.assertEqual(title, EDITOR_TITLE)
            events.append("editor-map-query")
            return "42" if state["mapped"] else None
        def focused():
            events.append("editor-focus-query")
            return state["focused"]
        def wait(predicate, description):
            for _ in range(3):
                if predicate():
                    if state["focused"]:
                        events.append("editor-ready")
                    return
                if not never_ready:
                    state["focused"], state["mapped"] = state["mapped"], True
            raise AssertionError("Timed out: " + description)
        with patch("shutil.which", return_value="synthetic-tool"), \
             patch.dict(os.environ, {"DISPLAY": ":synthetic", "DBUS_SESSION_BUS_ADDRESS": "synthetic"}), \
             patch.dict(sys.modules, {"pyatspi": Mock(), "gi": Mock(), "gi.repository": bridge}), \
             patch("subprocess.Popen", side_effect=spawn), patch("tempfile.NamedTemporaryFile", return_value=editor_input), \
             patch.object(Linux, "window_manager_ready", return_value=True), \
             patch.object(Linux, "window", side_effect=mapped), patch.object(Linux, "editor_focused", side_effect=focused), \
             patch(__name__ + ".wait_for", side_effect=wait):
            if never_ready:
                with self.assertRaisesRegex(AssertionError, "synthetic editor mapped and focused"):
                    Linux()
            else:
                native = Linux()
                subprocess.Popen(["synthetic-app"])
                native.close()
        return events, wm, editor, editor_input

    def test_linux_editor_settles_before_the_app_launches(self):
        events, _, _, _ = self.linux_editor_startup()
        self.assertLess(events.index("editor-ready"), events.index("app-launch"))
        self.assertEqual(events.count("editor-launch"), 1)
        self.assertGreaterEqual(events.count("editor-map-query"), 2)
        self.assertGreaterEqual(events.count("editor-focus-query"), 2)

    def test_linux_missing_editor_ownership_refuses_app_launch_and_cleans_up(self):
        events, wm, editor, editor_input = self.linux_editor_startup(never_ready=True)
        self.assertNotIn("app-launch", events)
        wm.terminate.assert_called_once()
        editor.terminate.assert_called_once()
        editor_input.close.assert_called_once()

    def test_linux_initial_focus_queries_native_owner_instead_of_accessible_focus(self):
        native = Linux.__new__(Linux)
        native.app = Mock(pid=100)
        for owner, expected in (("100", True), ("200", False)):
            with self.subTest(owner=owner), patch(__name__ + ".run", side_effect=["42", owner]) as query:
                self.assertEqual(native.picker_has_input_focus(), expected)
                self.assertEqual(query.call_args_list, [call("xdotool", "getwindowfocus"),
                                                       call("xdotool", "getwindowpid", "42")])

    def test_linux_initial_focus_query_failure_never_claims_keyboard_ownership(self):
        native = Linux.__new__(Linux)
        native.app = Mock(pid=100)
        for replies in ([subprocess.CalledProcessError(1, "xdotool")],
                        ["42", subprocess.CalledProcessError(1, "xdotool")]):
            with self.subTest(replies=replies), patch(__name__ + ".run", side_effect=replies):
                self.assertFalse(native.picker_has_input_focus())

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

    def test_escape_waits_through_withdrawal_uncertainty_for_positive_hidden_evidence(self):
        visible = {"visible": True, "nodes": [{"name": "Search references", "value": "synthetic"}]}
        hidden = {"visible": False, "nodes": []}
        native = Mock()
        snapshot = Mock(side_effect=[visible, visible, WindowObservationFailure(), hidden])
        with patch("time.sleep"):
            escape_to_hide(native, snapshot, "synthetic picker hidden")
        self.assertEqual(snapshot.call_count, 4)
        native.key.assert_called_once_with("Escape")

    def test_persistent_post_escape_uncertainty_exhausts_one_deadline_without_another_action(self):
        visible = {"visible": True, "nodes": [{"name": "Search references", "value": "synthetic"}]}
        native, clock = Mock(), self.CopyClock()
        reads = 0
        def snapshot():
            nonlocal reads
            reads += 1
            if reads <= 2:
                return visible
            raise WindowObservationFailure()
        def sleep(seconds):
            clock.elapsed += seconds
        with patch("time.monotonic", clock.monotonic), patch("time.sleep", side_effect=sleep), \
                self.assertRaisesRegex(AssertionError, "Timed out.*visibility"):
            escape_to_hide(native, snapshot, "synthetic picker hidden")
        self.assertEqual(clock.elapsed, ESCAPE_STEP_TIMEOUT)
        native.key.assert_called_once_with("Escape")

    def test_pre_key_uncertainty_and_non_window_failures_still_refuse(self):
        visible = {"visible": True, "nodes": [{"name": "Search references", "value": "synthetic"}]}
        cleared = {"visible": True, "nodes": [{"name": "Search references", "value": ""}]}
        for observations, error, actions in (([visible, WindowObservationFailure()], WindowObservationFailure, 0),
                ([visible, visible, cleared, WindowObservationFailure()], WindowObservationFailure, 1),
                ([visible, visible, RuntimeError("synthetic failure")], RuntimeError, 1)):
            native = Mock()
            with self.subTest(actions=actions, error=error.__name__), self.assertRaises(error):
                escape_to_hide(native, Mock(side_effect=observations), "synthetic picker hidden")
            self.assertEqual(native.key.call_count, actions)

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

    def test_initial_escape_waits_for_native_keyboard_owner_after_accessible_focus(self):
        native = self.InitialPickerModel(owns_keys=False)
        def settled(predicate, description):
            if predicate():
                return
            # Native activation can follow the accessible focused flag. No key
            # may be sent to the previous app while ownership is still pending.
            self.assertEqual(native.events, [])
            native.key_window = True
            self.assertTrue(predicate(), description)
        with patch(__name__ + ".wait_for", side_effect=settled):
            prepare_initial_picker_show(native, native.snapshot)
        self.assertFalse(native.visible)
        self.assertEqual(native.events, [("Escape", True)])

    def test_initial_escape_refuses_accessible_focus_without_native_keyboard_owner(self):
        native = self.InitialPickerModel(owns_keys=False)
        with patch(__name__ + ".wait_for", side_effect=lambda predicate, description: self.assertTrue(predicate(), description)):
            with self.assertRaisesRegex(AssertionError, "initial picker search ready"):
                prepare_initial_picker_show(native, native.snapshot)
        self.assertEqual(native.events, [])

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


    def test_menu_actions_open_more_first(self):
        native = Mock()
        menu_action(native, "Reload")
        self.assertEqual(native.click.call_args_list, [call("More"), call("Reload")])
        with self.assertRaises(ValueError):
            menu_action(native, "Search references")
        for error in (AssertionError(), RuntimeError(), subprocess.CalledProcessError(1, "synthetic")):
            native.click.reset_mock()
            native.click.side_effect = [None, error, None]
            with patch("time.sleep"):
                menu_action(native, "Reload")
            self.assertEqual(native.click.call_args_list, [call("More"), call("Reload"), call("Reload")])

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

    def test_retained_snapshot_diagnostics_separate_fragments_without_echoing_input(self):
        cases = [(" · not updated", True, False), (" · 4 references", False, True),
                 (" · not updated · 4 references", True, True),
                 (" · not updated · 3 references", True, False)]
        for text, updated, count in cases:
            with self.subTest(text=text):
                diagnostic = retained_snapshot_fragments({"nodes": [{"name": "Synthetic test app"}, {"value": text},
                    {"value": "synthetic-private-sentinel"}]})
                self.assertEqual(diagnostic, {"not_updated": updated, "four_references": count})
                self.assertTrue(all(type(value) is bool for value in diagnostic.values()))
                self.assertNotIn("synthetic-private-sentinel", json.dumps(diagnostic))

    def test_retained_snapshot_accepts_native_nodes_independent_of_sibling_order(self):
        for nodes in [
                [{"value": "not updated"}, {"value": " · "}, {"value": "synthetic.bib · 4 references"}],
                [{"value": "4 references"}, {"value": " · "}, {"value": "not updated"}],
                [{"value": "not updated"}, {"value": "synthetic.bib · 4 references. checked just now, 12:00 PM"}],
                [{"value": "synthetic.bib · 4 references. the file could not be read"}, {"value": "not updated"}],
                [{"value": "not updated"}, {"value": "Zotero · 4 references. Zotero could not be reached"}],
                [{"value": "synthetic.bib · 4 references. The library could not be opened: synthetic.bib."}, {"value": "not updated"}],
                [{"value": "synthetic.bib · not updated. synthetic.bib · 4 references"}]]:
            with self.subTest(nodes=nodes):
                self.assertEqual(retained_snapshot_fragments({"nodes": nodes}),
                                 {"not_updated": True, "four_references": True})
        for nodes in [
                [{"value": " · "}, {"value": "synthetic.bib · 4 references"}],
                [{"value": "not updated"}, {"value": "synthetic.bib · 3 references"}],
                [{"value": "not updatedish"}, {"value": "synthetic.bib · 4 references"}],
                [{"value": "not"}, {"value": "updated"}, {"value": "4 references"}]]:
            with self.subTest(nodes=nodes):
                self.assertFalse(all(retained_snapshot_fragments({"nodes": nodes}).values()))

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

    def test_missing_library_rejects_counts_that_only_prefix_other_wording(self):
        name = "synthetic.bib"
        message = (f"The library could not be opened: {name}. "
                   "the library file is unavailable; keeping the last loaded references. "
                   "Choose it again or reconnect the drive.")
        for count in ("4 references missing; 3 references retained", "4 references missing",
                      "4 references retained", "4 references · unrelated status",
                      "4 references. unrelated status", "4 references. checkedness"):
            with self.subTest(count=count):
                current = {"nodes": [{"name": message}, {"value": "not updated"},
                                     {"value": f"{name} · {count}"}]}
                evidence = missing_library_evidence(current, name)
                self.assertTrue(evidence["failed_source_named"])
                self.assertTrue(evidence["retention_explained"])
                self.assertTrue(evidence["recovery_action"])
                self.assertFalse(evidence["retained_snapshot"])


    def test_picker_open_p95_is_nearest_rank_over_all_twenty_opens(self):
        samples = list(range(20, 0, -1))
        summary = summarize_picker_opens(samples)
        self.assertEqual(summary["sample_count"], 20)
        self.assertEqual(summary["median_ms"], 10.5)
        self.assertEqual(summary["p95_ms"], 19)
        self.assertEqual(summary["samples_ms"], samples)
        for invalid in ([], [1] * 19, [1] * 19 + [float("nan")], [1] * 19 + [float("inf")],
                        [1] * 19 + [-1], [1] * 19 + [True]):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                summarize_picker_opens(invalid)

    def test_picker_open_timing_is_linux_only_and_never_enforces_the_target(self):
        for p95 in (99, 100, 100.001, 200):
            with self.subTest(p95=p95):
                summary = summarize_picker_opens([1] * 18 + [p95, 999])
                self.assertEqual(summary["status"], "report-only")
                self.assertEqual(summary["platform"], "Linux")
                self.assertFalse(summary["gate_enforced"])
                self.assertEqual(summary["target_p95_ms"], 100)
                self.assertEqual(summary["p95_ms"], p95)
                self.assertIn("Timing is reported on Linux only", summary["summary"])
                self.assertIn("100 ms p95 gate is enforced nowhere yet", summary["summary"])
                self.assertIn("maintainer hand test #79", summary["summary"])
                self.assertIn("AT-SPI", summary["reason"])

    def picker_open_probe(self, *, value="smoke", typed="smoke", readiness=None, focus_role=None, mapped=None):
        clock, events = self.CopyClock(), []
        native = Mock(spec=["key", "text", "picker_mapped", "editor_received"])
        native.key.side_effect = lambda chord: (events.append(("key", chord)), setattr(clock, "elapsed", clock.elapsed + .002))
        native.text.side_effect = lambda text: (events.append(("text", text)), setattr(clock, "elapsed", clock.elapsed + .003))
        maps = iter(mapped or [True])
        def picker_mapped():
            events.append(("mapped",))
            clock.elapsed += .001
            return next(maps, True)
        native.picker_mapped.side_effect = picker_mapped
        native.editor_received.return_value = {"available": True, "text": "", "truncated": False}
        states = iter(readiness or [(True, True, True)])
        def snapshot():
            events.append(("snapshot",))
            clock.elapsed += .007
            visible, foreground, focused = next(states, (True, True, True))
            nodes = [{"name": "Search references", "role": "combo box", "value": value,
                      "focused": focused and focus_role is None}]
            if focus_role is not None:
                nodes.append({"role": focus_role, "focused": focused})
            return {"visible": visible, "foreground": foreground, "nodes": nodes}
        with patch("time.monotonic", clock.monotonic), \
             patch("time.sleep", lambda interval: setattr(clock, "elapsed", clock.elapsed + interval)):
            elapsed = measure_picker_open(native, snapshot, "ctrl+shift+y", typed)
        return elapsed, events

    def test_opening_text_is_injected_once_after_mapping_before_any_focus_probe(self):
        elapsed, events = self.picker_open_probe()
        self.assertEqual(events, [("key", "ctrl+shift+y"), ("mapped",), ("text", "smoke"), ("snapshot",), ("snapshot",)])
        # Mapping, injection and readiness count; the later readback does not.
        self.assertAlmostEqual(elapsed, 13)

    def test_opening_typing_waits_for_mapping_but_not_foreground_or_search_focus(self):
        elapsed, events = self.picker_open_probe(mapped=[False, False, True],
            readiness=[(True, False, False), (True, True, True)])
        self.assertEqual(events, [("key", "ctrl+shift+y")] + [("mapped",)] * 3 +
                         [("text", "smoke")] + [("snapshot",)] * 3)
        self.assertAlmostEqual(elapsed, 37)

    def test_unmapped_picker_never_receives_opening_text_or_a_search_probe(self):
        native = Mock(spec=["key", "text", "picker_mapped", "editor_received"])
        native.picker_mapped.return_value = False
        native.editor_received.return_value = {"available": False}
        snapshot, report = Mock(), {}
        def wait(predicate, description, **_options):
            self.assertFalse(predicate())
            raise AssertionError("Timed out: " + description)
        with patch(__name__ + ".wait_for", side_effect=wait), patch("sys.stderr", io.StringIO()), \
             self.assertRaisesRegex(AssertionError, "Timed out: picker mapped before opening typing"):
            measure_picker_open(native, snapshot, "ctrl+shift+y", "smoke", report=report)
        native.text.assert_not_called()
        snapshot.assert_not_called()
        self.assertEqual(report["opening_text_failure"], {
            "expected_text": "smoke", "search_values": [], "editor": {"available": False}})

    def test_picker_open_timer_requires_visibility_foreground_and_search_focus(self):
        elapsed, events = self.picker_open_probe(typed=None, readiness=[
            (False, True, True), (True, False, True), (True, True, False), (True, True, True)])
        self.assertEqual(events, [("key", "ctrl+shift+y")] + [("snapshot",)] * 4)
        self.assertAlmostEqual(elapsed, 45)  # 2 ms injection + 4*7 ms probes + 3*5 ms polls.

    def test_picker_open_accepts_virtual_result_focus_but_not_a_focused_button(self):
        for role in ("option", "list item", "ControlType.ListItem"):
            with self.subTest(role=role):
                elapsed, _ = self.picker_open_probe(focus_role=role)
                self.assertAlmostEqual(elapsed, 13)
        with self.assertRaisesRegex(AssertionError, "shortcut opens interactive picker"):
            self.picker_open_probe(focus_role="button")

    def test_opening_typing_rejects_dropped_extra_and_unreplaced_text_without_retry(self):
        for value in ("", "moke", "smok", "ssmoke", "previoussmoke"):
            with self.subTest(value=value), patch("sys.stderr", io.StringIO()), \
                 self.assertRaisesRegex(AssertionError, "opening keystrokes retained exactly"):
                self.picker_open_probe(value=value)

    def test_unavailable_editor_diagnostic_preserves_the_opening_failure(self):
        native = Mock(spec=["key", "text", "picker_mapped", "editor_received"])
        native.editor_received.side_effect = OSError("synthetic editor already gone")
        report = {}
        with patch(__name__ + ".wait_for", side_effect=AssertionError("synthetic opening failure")), \
             patch("sys.stderr", io.StringIO()), self.assertRaisesRegex(AssertionError, "^synthetic opening failure$"):
            measure_picker_open(native, Mock(), "ctrl+shift+y", "smoke", report=report)
        self.assertEqual(report["opening_text_failure"]["editor"], {"available": False})

    def test_opening_failure_reports_last_observed_field_and_editor_without_retry(self):
        native = Mock(spec=["key", "text", "picker_mapped", "editor_received"])
        native.picker_mapped.return_value = True
        native.editor_received.return_value = {"available": True, "text": "s", "truncated": False}
        snapshots = iter({"visible": True, "foreground": True, "nodes": [
            {"name": "Search references", "role": "combo box", "value": value, "focused": True}]}
            for value in ("", "mok", "moke"))
        snapshot = Mock(side_effect=lambda: next(snapshots))
        def wait(predicate, description, **_options):
            if description == "opening keystrokes retained exactly":
                self.assertFalse(predicate())
                self.assertFalse(predicate())
                raise AssertionError("Timed out: " + description)
            self.assertTrue(predicate())
        report, errors = {}, io.StringIO()
        with patch(__name__ + ".wait_for", side_effect=wait), patch("sys.stderr", errors), \
             self.assertRaisesRegex(AssertionError, "Timed out: opening keystrokes retained exactly"):
            measure_picker_open(native, snapshot, "ctrl+shift+y", "smoke", report=report)
        self.assertEqual(report["opening_text_failure"], {
            "expected_text": "smoke", "search_values": ["moke"],
            "editor": {"available": True, "text": "s", "truncated": False}})
        self.assertEqual(json.loads(errors.getvalue().removeprefix("OPENING TEXT FAILURE ")),
                         report["opening_text_failure"])
        self.assertEqual(snapshot.call_count, 3, "Keep the failed poll, without a late native read")
        native.key.assert_called_once_with("ctrl+shift+y")
        native.text.assert_called_once_with("smoke")
        native.editor_received.assert_called_once_with()

    def test_existing_performance_json_line_keeps_its_summary_fields(self):
        # This checkout has no external smoke log parser; retain the existing
        # unprefixed performance JSON fields when adding the separate open line,
        # including main's diagnostic-only status for debug launch timings.
        summary = json.loads(json.dumps(summarize_performance(self.perf_samples(50, 20, 40, 30))))
        self.assertEqual(summary["status"], "diagnostic-only")
        self.assertEqual(summary["startup_ms"], 30)
        self.assertEqual(summary["first_result_ms"], 5)
        self.assertEqual(summary["sample_count"], 3)
        self.assertIsInstance(summary["summary"], str)

    @staticmethod
    def perf_samples(warmup, *startups, first=5, memory=1000):
        return [{"warmup": True, "startup_ms": warmup, "first_result_ms": first, "memory_bytes": memory}] + [
            {"warmup": False, "startup_ms": startup, "first_result_ms": first, "memory_bytes": memory} for startup in startups]


    def test_performance_summary_requires_a_measured_launch(self):
        with self.assertRaises(ValueError):
            summarize_performance(self.perf_samples(50))

    def test_debug_launch_timings_do_not_claim_release_budget_evidence(self):
        report = summarize_performance(self.perf_samples(100, 200, 300, 400))
        self.assertEqual(report["status"], "diagnostic-only")
        self.assertFalse(report["release_evidence"])
        self.assertEqual(report["sample_count"], 3)

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


    def test_toggle_diagnostics_accept_only_exact_fixed_lines_from_current_run_logs(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "synthetic.log"
            log.write_text("\n".join(("private-synthetic-library-text", "ibis-picker: toggle visible focused hide",
                                      "ibis-picker: toggle visible unfocused show private-synthetic-detail")), encoding="utf-8")
            self.assertEqual(fixed_toggle_diagnostics([log]), ["ibis-picker: toggle visible focused hide"])


    def test_copy_preflight_rejects_unchanged_clipboard_from_a_prior_run(self):
        class BrokenClipboard:
            def set_clipboard(self, text):
                pass

            def clipboard(self):
                return "[@smoke000001]"

        with self.assertRaisesRegex(AssertionError, "sentinel"):
            prime_clipboard(BrokenClipboard())


    def test_nightly_settings_and_the_update_question_answered_in_advance(self):
        with tempfile.TemporaryDirectory() as temporary:
            settings = settings_path("Linux", Path(temporary), identifier="com.ibiscite.ibis.nightly")
            self.assertEqual(settings, Path(temporary) / "config/com.ibiscite.ibis.nightly/settings.json")
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
