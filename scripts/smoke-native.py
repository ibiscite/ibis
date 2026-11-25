#!/usr/bin/env python3
"""Exercise the embedded native picker (Linux X11; the macOS and Windows journey with --journey); never substitute a browser or IPC mock.

Exit 1 means a failed assertion; exit 2 means the session lacks a required native
capability. Only synthetic test text is recorded, never the accessibility tree or
clipboard contents. See ../CONTRIBUTING.md before running on a personal desktop.

Journeys (daily/manual CI and Preview run them on the installed nightly):

`scripts/smoke-native.py` drives the real desktop app with invented data.
Daily/manual CI runs installed nightly journeys; Preview checks package
install/start and offline observation.
Hosted runner coverage is listed in [compatibility](compatibility.md#platforms-tested-and-targeted).

- **Linux/X11:** Xvfb/Openbox, one display and the default layout. Checks
  shortcut/focus, search, citation clipboard text, hide/reopen, settings, reload
  and recovery. After Enter copies, it checks the reason, explicitly pastes
  with Shift+Insert into its xterm editor and reads the citation back.
  Require passing `smoke.json`; 100k launch/memory figures are diagnostics.
- **macOS journey:** `bash scripts/hands-on-macos.sh --journey` installs a
  nightly in an isolated profile and invokes the harness. From TextEdit,
  selected text becomes the search; Enter must replace it with the expected
  citation exactly once, verified through Accessibility. A safe copy gets
  one retry. A Terminal collector must receive no keys. Missing terminal
  Accessibility permission or an already open TextEdit makes it unavailable.
  Daily/manual CI runs this journey on the installed nightly.
- **Windows journey:** `smoke-native.py --journey` uses Notepad and an inert
  console collector. It types the search, checks selection replacement once
  through `WM_GETTEXT`, and checks that the console gets no automatic input.
  `journey.json` records the editor and console host. A locked/disconnected
  desktop is unavailable. `scripts/windows-journey.ps1` supplies native control.

The separate Windows paste primitive test bypasses the picker. Foreground
denial can skip it. macOS/Windows notice text is checked only when native
accessibility exposes it. These journeys do not establish every editor,
keyboard layout, screen reader, cloud provider or Wayland compositor.
"""
import argparse
from contextlib import contextmanager, nullcontext, redirect_stderr, redirect_stdout
from contextvars import ContextVar
import json
import math
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import time
# Keep sibling helpers importable outside Python's script path.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from native_test_settings import SettingsDirectory
from native_evidence import (ACTIVE, BundleError, Evidence, FORMAT_APA, collect_metadata, failure,
                             observe_native, recording, unavailable)


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
# The catalogue's copied-notice-reason, worded for Linux: written here by hand, not read from the app.
LINUX_NOTICE = "Copied. Ibis can\u2019t paste on this system yet. Press Ctrl+V to paste."
PICKER_TITLE = "Ibis"
EDITOR_TITLE = "Ibis synthetic editor"

# Execution hang guard; C# compilation happens separately before timed calls.
CONTRACT_TIMEOUT = 120
WAIT_DEADLINE = ContextVar("native_wait_deadline", default=None)


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


BIDI_ISOLATES = str.maketrans("", "", "\u2066\u2067\u2068\u2069")


def shows_count(snapshot, count):
    """Whether the footer's count, such as "3 references", reaches the accessibility tree.

    The footer splits the library name and the count into separate elements.
    WebKitGTK exposes only text in elements it keeps (the name's <bdi>, the
    count's <bdi>, the spoken detail's <p>); a plain <span> beside them loses its text.
    """
    return count in strings(snapshot)


def shows_empty_library(snapshot):
    # The status's separator and count are separate <bdi> nodes. Match the
    # complete count clause in one node, not traversal order or joined text.
    return any(re.search(r"(?:^| · )no references(?=$| · |\. checked )",
                         " ".join(str(node.get(key, "")).translate(BIDI_ISOLATES).split())) is not None
               for node in snapshot["nodes"] for key in ("name", "value"))


def retained_snapshot_fragments(snapshot, name=None):
    """Fixed predicates only, from one bounded native snapshot; never echo text."""
    # Native traversal order is not reading order (Linux walks a LIFO stack).
    # Match complete atomic wording or one composite text node, never invent a
    # phrase by joining neighboring elements from different native boundaries.
    values = [" ".join(str(node.get(key, "")).translate(BIDI_ISOLATES).split())
              for node in snapshot["nodes"] for key in ("name", "value")]
    def contains(fragment, ending=r"(?!\w)"):
        return any(re.search(r"(?:^|^· | · )" + re.escape(fragment) + ending, value) is not None
                   for value in values)
    # The count is a complete clause: either its own node/summary, or followed
    # by the spoken detail's check time or failure. A word boundary alone would
    # also accept "4 references missing; 3 references retained".
    count_ending = r"(?=$|\. (?:last read |checked |The library could not be opened: |the file could not be read$|Zotero could not be reached$))"
    return {"problem_shown": contains("not updated") or contains("moved") or
            (name is not None and contains(f"{name} moved")),
            "four_references": contains("4 references", count_ending)}


def missing_library_evidence(snapshot, name):
    text = strings(snapshot).translate(BIDI_ISOLATES)
    # Bidi isolates can expose the separator and state as separate native
    # text nodes. Their boundaries are whitespace, not missing status words.
    retained = retained_snapshot_fragments(snapshot, name)
    return {key: fragment if type(fragment) is bool else fragment in text for key, fragment in {
        "failed_source_named": f"The library could not be opened: {name}",
        "retention_explained": "the library file is unavailable; download any online-only file to this computer first; keeping the last loaded references",
        "recovery_action": "Choose it again or reconnect the drive.",
        # The footer says the library moved or was not updated; its detail, read by
        # screen readers, keeps the retained count. Names are isolated (U+2068/9).
        "retained_snapshot": all(retained.values()),
    }.items()}




# Library actions live in the footer's Library menu; application actions in More,
# whose palette lists them as options of a listbox (ui/Palette), not menu items.
MENU_ACTIONS = {"Choose Library…": "Library", "Use Zotero": "Library",
                "Reload Library": "Library", "Quit Ibis": "More"}
# ATK_ROLE_PUSH_BUTTON_MENU is how AT-SPI exposes a button with aria-haspopup.
# Library, More and the format pop-up use the same native HTML button contract.
MENU_BUTTON_ROLES = {"button", "pushbutton", "pushbuttonmenu", "combobox"}
MENU_ITEM_ROLES = {"menuitem", "checkmenuitem", "radiomenuitem", "menuitemcheckbox", "menuitemradio"}
# ATK exposes a listbox option as a list item; newer AT-SPI names the role "option".
PALETTE_ITEM_ROLES = {"listitem", "option"}


class NativeControlUnavailable(AssertionError):
    def __init__(self, name, *, named, action, returned_false, reason):
        super().__init__(f"No invokable accessibility control {name!r}")
        self.facts = {"named": bool(named), "action": bool(action), "returned_false": bool(returned_false), "reason": reason}


def menu_failure_facts(native, trigger, item, stage, last_error):
    """Fresh, fixed-field facts only: no tree, names, paths or exception text."""
    facts = {"stage": stage, "snapshot_available": False,
             "invocation": last_error.facts if isinstance(last_error, NativeControlUnavailable) else None}
    if isinstance(native, Linux):
        # Independent of window identity: a chooser or an observation refusal
        # must not hide which exact Library objects the provider exposed.
        try:
            facts["library_controls"] = native.named_control_facts("Library")
        except Exception:
            facts["library_controls"] = {"available": False, "complete": False, "matches": []}
    try:
        current = native.snapshot()
        if type(current) is not dict or type(current.get("nodes")) is not list:
            return facts
        nodes = current["nodes"]
        role = lambda node: str(node.get("role", "")).lower().replace(" ", "")
        facts.update(snapshot_available=True, visible=bool(current.get("visible")), foreground=bool(current.get("foreground")),
                     trigger_button=any(node.get("name") == trigger and role(node) in MENU_BUTTON_ROLES for node in nodes),
                     menu=any(node.get("name") == trigger and role(node) == "menu" for node in nodes),
                     item_named=any(node.get("name") == item for node in nodes),
                     chooser_or_dialog=any(role(node) in ("filechooser", "dialog") for node in nodes),
                     empty_library=any("no references" in str(node.get(key, "")) for node in nodes for key in ("name", "value")),
                     four_references=shows_count(current, "4 references"))
    except Exception:
        # Diagnostics must not replace the original failed check or print the
        # native exception, which may contain arbitrary desktop text.
        pass
    return facts


def menu_action(native, name):
    """Open the owning menu, then press the named item, as a user would."""
    if name not in MENU_ACTIONS:
        raise ValueError(f"{name!r} is not a picker menu action")
    trigger = MENU_ACTIONS[name]
    last_error, stage = None, "trigger"
    # The accessibility tree can lag the menu opening; wait for the item.
    def pressed():
        nonlocal last_error
        try:
            native.click(name)
            return True
        except NativeControlUnavailable as error:
            if error.facts["returned_false"]:
                raise
            last_error = error
            return False
    try:
        native.click(trigger)
        stage = "item"
        wait_for(pressed, f"{name} appears in the {trigger} menu")
    except (AssertionError, RuntimeError, subprocess.CalledProcessError) as error:
        print("MENU ACTION FAILURE", json.dumps(menu_failure_facts(native, trigger, name, stage, last_error or error)),
              file=sys.stderr, flush=True)
        raise


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
    wait_for(lambda: bool(named(snapshot(), "Keyboard Shortcuts")), "shortcut opens More", timeout=5)
    facts["shortcut_opened"] = True
    native.key("Escape")
    wait_for(lambda: not named(snapshot(), "Keyboard Shortcuts"), "Escape closes More", timeout=5)
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
        facts["space_reopened"] = bool(named(current, "Keyboard Shortcuts"))
        # A successful modal open hides Search from the accessibility tree.
        if not facts["space_reopened"]:
            facts["search_unchanged"] = facts["search_unchanged"] and values(current) == original
        return facts["space_reopened"]
    wait_for(reopened, "Space reopens More after Escape", timeout=5)
    native.key("Tab")
    wait_for(lambda: not named(snapshot(), "Keyboard Shortcuts"), "Tab closes More", timeout=5)

    def picker():
        current = snapshot()  # The caller also verifies the owned app is alive.
        assert current["visible"] and current["foreground"] and not named(current, "Keyboard Shortcuts"), "Native picker unavailable after Tab"
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


def escape_to_hide(native, snapshot, description):
    """Wait for each Escape's state change before sending the next key."""
    last = {}
    if has_search_text(snapshot()):
        if not snapshot()["visible"]:
            return
        native.key("Escape")
        def cleared():
            last.update(snapshot())
            return not last["visible"] or not has_search_text(last)
        wait_for(cleared, "Escape clears Search or hides picker")
        if not last["visible"]:
            return
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


def type_query(native, snapshot, text, select_all):
    """Inject once; a delayed readback must never cause another input action."""
    native.key(select_all)
    native.text(text)
    wait_for(lambda: query_failure_facts(snapshot(), text, "")["exact_search_value"], "Search holds the typed query")


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

    wait_for(lambda: values() and native.picker_has_input_focus(),
             "reopened picker visible with keyboard ownership and editable search")
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
    try:
        return SettingsDirectory(candidate)
    except FileExistsError:
        raise Unavailable("Use a disposable desktop account without existing Ibis settings", "disposable-settings") from None


CLIPBOARD_SENTINEL = "Ibis native smoke clipboard sentinel"
CLIPBOARD_READ_TIMEOUT = 3


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


def copy_with_diagnostics(native, report, key_window=None, *, expected="[@smoke000001]"):
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
    clipboard = {"polls": 0, "stalled_reads": 0, "expected": False, "sentinel": False, "empty": False}

    def copied():
        try:
            value = native.clipboard(timeout=CLIPBOARD_READ_TIMEOUT)
        except subprocess.TimeoutExpired:
            clipboard["stalled_reads"] += 1
            raise
        clipboard.update(polls=clipboard["polls"] + 1, expected=value == expected,
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
        # Only Linux offers these X11 probes. Look on the type so an optional
        # method on a plain Mock cannot manufacture non-JSON failure facts.
        probe = getattr(type(native), "clipboard_facts", None)
        if probe is not None:
            try:
                with open(os.devnull, "w") as quiet, redirect_stdout(quiet), redirect_stderr(quiet):
                    clipboard["formats"] = probe(native)
            except Exception:
                clipboard["formats"] = {"available": False}
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

    On X11, wait for mapping and native keyboard ownership before typing.
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
            wait_for(lambda: native.picker_mapped() and native.picker_has_input_focus(),
                     "picker mapped with keyboard ownership before opening typing", interval=.005)
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
        # Shift+Insert pastes the clipboard, as a person pastes a copied citation.
        self.editor = subprocess.Popen(["xterm", "-title", EDITOR_TITLE, "-xrm", "XTerm*selectToClipboard: true", "-e",
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
        deadline = min(time.monotonic() + 20, WAIT_DEADLINE.get() or math.inf)
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
        try:
            # None is a completed observation of absence, not a pending poll.
            return wait_for(lambda: (observe(),), "window identity", deadline=deadline)[0]
        except AssertionError as timeout:
            error = timeout.__cause__
            if not isinstance(error, WindowObservationFailure):
                error = WindowObservationFailure("query-deadline")
            print("LINUX WINDOW OBSERVATION FAILURE " + error.branch, file=sys.stderr, flush=True)
            raise error from None

    def named_control_facts(self, name):
        """Read roles and Action metadata only; never activate or echo native text.

        Role names come from the installed AT-SPI ROLE_* enum constants, not
        an open-ended provider string or a guessed allowlist. Unknown numeric
        roles remain identifiable by their ID. This does not change eligibility
        for click(), which still requires a real button/menu item and Action.
        """
        facts = {"available": False, "complete": False, "matches": []}
        role_names = {}
        for key in dir(self.atspi):
            if re.fullmatch(r"ROLE_[A-Z][A-Z0-9_]*", key, re.ASCII):
                try:
                    role_names[int(getattr(self.atspi, key))] = key[5:].lower().replace("_", " ")
                except (TypeError, ValueError):
                    pass
        try:
            desktop = self.atspi.Registry.getDesktop(0)
        except Exception:
            return facts
        facts.update(available=True, complete=True)

        def children(parent, owned=False):
            try:
                count = parent.childCount
            except Exception:
                facts["complete"] = False
                return
            if count > 4000:
                facts["complete"] = False
            for index in range(min(count, 4000)):
                try:
                    child = parent[index]
                    if child and (not owned or child.get_process_id() == self.app.pid):
                        yield child
                except IndexError:
                    facts["complete"] = False
                    break
                except Exception:
                    facts["complete"] = False

        stack = list(children(desktop, owned=True))
        visited = 0
        while stack:
            if visited >= 4000:
                facts["complete"] = False
                break
            visited += 1
            item = stack.pop()
            try:
                matches = item.name == name
            except Exception:
                facts["complete"] = False
                matches = False
            if matches:
                entry = {"role": "unavailable", "role_id": None, "action": "unavailable", "invokable": "unavailable"}
                try:
                    role_id = int(item.getRole())
                    if 0 <= role_id <= 65535:
                        entry.update(role=role_names.get(role_id, "unknown"), role_id=role_id)
                except Exception:
                    pass
                try:
                    action = item.queryAction()
                    if action is None:
                        entry.update(action="absent", invokable="no")
                    else:
                        entry["action"] = "available"
                        entry["invokable"] = "yes" if action.nActions > 0 else "no"
                except NotImplementedError:
                    entry.update(action="absent", invokable="no")
                except Exception:
                    pass
                facts["matches"].append(entry)
            stack.extend(children(item))
        return facts

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
        named_target, matched_role, supported_action, returned_false = False, False, False, False
        # WebKitGTK can expose a click action on an enclosing named region.
        # Bind menu actions to their widget role, avoiding enclosing regions
        # and same-named menus left in the accessibility tree.
        roles = (MENU_BUTTON_ROLES if name == FORMAT_APA or name in MENU_ACTIONS.values()
                 else PALETTE_ITEM_ROLES if MENU_ACTIONS.get(name) == "More"
                 else MENU_ITEM_ROLES if name in MENU_ACTIONS else None)
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
                    named_target = True
                    if roles is None or item.getRoleName().lower().replace(" ", "") in roles:
                        matched_role = True
                        action = item.queryAction()
                        if not action.nActions:
                            action = None
                        else:
                            supported_action = True
            except self.accessibility_error:
                continue
            except NotImplementedError:
                action = None
            # An action error can mean it already ran. Never rediscover/retry it.
            if action is not None:
                if action.doAction(0):
                    return
                returned_false = True
                break
            stack.extend(children(item))
        # A rejected role never queried Action. Do not report that as a button
        # with no action again: retain the fixed reason without native text.
        reason = "not-found" if not named_target else "role-mismatch" if not matched_role else "action-unavailable" if not supported_action else "activation-refused"
        raise NativeControlUnavailable(name, named=named_target, action=supported_action, returned_false=returned_false, reason=reason)

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

    def clipboard_facts(self):
        """Independent bounded X11 probes; retain target names, never content/errors."""
        facts = {"offered_targets": []}
        for target in ("TARGETS", "text/html"):
            command = ["xclip", "-selection", "clipboard", "-out", "-target", target]
            try:
                if target == "TARGETS":
                    names = subprocess.check_output(command, text=True, stderr=subprocess.PIPE,
                                                    timeout=CLIPBOARD_READ_TIMEOUT)
                    if type(names) is not str:
                        raise ValueError("Invalid target names")
                    facts["offered_targets"] = names.splitlines()
                else:
                    # HTML is not needed even transiently in Python or a diagnostic exception.
                    subprocess.run(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                   check=True, timeout=CLIPBOARD_READ_TIMEOUT)
                facts[target] = "succeeded"
            except subprocess.TimeoutExpired:
                facts[target] = "stalled"
            except (OSError, subprocess.SubprocessError, ValueError):
                facts[target] = "errored"
        return facts

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


def wait_for(predicate, description, timeout=20, *, interval=.1, deadline=None):
    # Source locations are stable IDs within the recorded harness hash. Never
    # serialize descriptions: callers may interpolate desktop observations.
    parent = WAIT_DEADLINE.get()
    if deadline is None:
        deadline = time.monotonic() + timeout
    deadline = min(deadline, parent if parent is not None else math.inf)
    token = WAIT_DEADLINE.set(deadline)
    # Nested observations share the guard and never record polling frequency.
    trace = nullcontext() if parent is not None else recording("request", f"wait-{sys._getframe(1).f_lineno}")
    try:
        with trace:
            unknown = None
            while time.monotonic() < deadline:
                try:
                    result = predicate()
                    if result:
                        return result
                    unknown = None
                except WindowObservationFailure as error:
                    # Unknown cannot prove mapping or hiding.
                    unknown = error
                time.sleep(min(interval, max(0, deadline - time.monotonic())))
            raise AssertionError(f"Timed out: {description}") from unknown
    finally:
        WAIT_DEADLINE.reset(token)


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
    return value.get("key_output") == "typst"


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
            return current["visible"] and text in strings(current).translate(BIDI_ISOLATES)

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
        type_query(native, snapshot, "smoke000000", "ctrl+a")
        wait_for_query(snapshot, "smoke000000", "Ibis smoke reference 000000", report, settled=True)

        def distinct_format_and_copy():
            current = snapshot()
            controls = [node.get("name") for node in current["nodes"]
                        if node.get("role", "").lower().replace(" ", "") in MENU_BUTTON_ROLES]
            # The pop-up names the format; the bar's action names what Enter does, never the format.
            return current["visible"] and current["foreground"] and controls.count(FORMAT_APA) == controls.count("Copy") == 1

        wait_for(distinct_format_and_copy, "citation format and copy have distinct accessible names")
        passed("citation format and copy have distinct accessible names")
        native.click(FORMAT_APA)
        wait_for(lambda: bool(named(snapshot(), "Typst")), "citation format lists Typst")
        # Pick with the filter: accessibility exposes the menu's radio items
        # without a pattern the exact-name click helper accepts.
        # Not every platform reports focus on the filter; the filtered list proves where typing went.
        native.text("Typst")
        # "LaTeX \cite" names only that menu item; "APA" is also the header's visible text.
        wait_for(lambda: not named(snapshot(), "LaTeX \\cite"), "citation format filter leaves only Typst")
        native.key("Down")
        native.key("Return")
        wait_for(lambda: saved_typst(settings), "Typst preferences saved before copying")
        passed("Typst choice is saved before any copy")
        type_query(native, snapshot, "smoke000001", "ctrl+a")
        wait_for_query(snapshot, "smoke000001", "Ibis smoke reference 000001", report, settled=True)
        citation = "@smoke000001"
        wait_for(lambda: visible_text("Try a paste here first: press Enter."), "first-run practice offer")
        prime_clipboard(native)
        native.key("Return")
        practice = f"{citation} pasted here. Next, Enter pastes in your app."
        wait_for(lambda: visible_text(practice), "Typst citation in the practice line")
        assert native.clipboard(timeout=CLIPBOARD_READ_TIMEOUT) == CLIPBOARD_SENTINEL, "Practice paste changed the OS clipboard"
        passed("first Enter pastes the Typst citation inside Ibis")
        native.key("Return")
        copy_deadline = time.monotonic() + 20
        def copied():
            remaining = copy_deadline - time.monotonic()
            assert remaining > 0, "Native copy exceeded its clipboard-read budget"
            value = native.clipboard(timeout=remaining)
            assert time.monotonic() < copy_deadline, "Native copy exceeded its clipboard-read budget"
            return value == citation
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
        report["reason"] = str(error) if isinstance(error, (AssertionError, BundleError)) else type(error).__name__
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
                        owned_settings.close if owned_settings else None,
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


# The native journey: the real app,
# invented references, a real text app, and the document read back. macOS
# and Windows; Linux runs the same journey inside smoke_main, where Ibis
# copies and the person pastes.
JOURNEY_LIBRARY = """@article{journey2026, author = {Example, Ada}, title = {An invented journey through citation},
  journal = {Journal of Tests}, year = {2026}}
@article{decoy2025, author = {Sample, Ben}, title = {A decoy about sleep}, year = {2025}}
@article{other2024, author = {Other, Cy}, title = {Another reference on memory}, year = {2024}}
"""
JOURNEY_QUERY = "invented journey"
JOURNEY_TITLE = "An invented journey through citation"
# The selected words become the search; the citation replaces exactly them.
JOURNEY_DOCUMENT = "As shown in invented journey, sleep matters.\n"
# Written by hand from the invented entry, never computed by Ibis's code.
# With nothing chosen, TextEdit, Notepad and a terminal all get the document
# suggestion, APA: Ibis does not know the kind of the last two (decision 0042).
JOURNEY_CITATION = "(Example, 2026)"  # The plain text of the formatted copy.
JOURNEY_PASTED = "As shown in (Example, 2026), sleep matters.\n"
# The safe copy when the last check before pasting cannot finish in time.
COPIED_HINT = "Copied. Ibis couldn\u2019t confirm where the text would go. Press ⌘V to paste."
JOURNEY_SETTINGS = b'{"schema_version": 2, "update_checks": false}'
# The journey starts with no settings, so its paste is the first with a
# suggested format and says so instead of naming the app (decision 0042).
PASTED_NOTICE = "Pasted in APA"
TERMINAL_NOTICE = "Copied. Not pasted into a terminal. Press ⌘V to paste."
COLLECTOR_TITLE = "ibis-journey-collector"
# Every Ibis answers the same shortcut, and the first one's picker takes the keys.
ANOTHER_IBIS = "Quit every Ibis, the installed one included, before the journey; it answers the same shortcut"
# The Windows collector: a console program that records every key it gets
# and stops cleanly when the stop file appears, so its window closes.
WINDOWS_COLLECTOR_SCRIPT = f"""import ctypes, msvcrt, os, sys, time
ctypes.windll.kernel32.SetConsoleTitleW({COLLECTOR_TITLE!r})
with open(sys.argv[1], "ab", buffering=0) as received:
    while not os.path.exists(sys.argv[1] + ".stop"):
        if msvcrt.kbhit():
            received.write(msvcrt.getwch().encode("utf-8"))
        else:
            time.sleep(0.02)
"""


def shows(strings, text):
    """Whether text is shown, also when highlighting splits it into pieces."""
    return text in strings or text in "".join(strings) or text in " ".join(strings)


class MacOS:
    """TextEdit, Terminal and Ibis through Accessibility and posted keys.

    Needs this terminal to have Accessibility access. Reads only TextEdit's
    one text area and Ibis's own windows, and types only into Ibis."""

    name, terminal = "TextEdit", "Terminal"
    reads_selection = True  # Ibis opens with the selected words as the search.
    select_all = "super+a"
    pasted, pasted_citation = JOURNEY_PASTED, JOURNEY_CITATION
    pasted_notice, terminal_notice, copied_hint = PASTED_NOTICE, TERMINAL_NOTICE, COPIED_HINT

    @staticmethod
    def preflight():
        if running("TextEdit"):
            raise Unavailable("Quit TextEdit before the journey; it opens its own invented document", "native-session")
        if running("ibis-desktop"):
            raise Unavailable(ANOTHER_IBIS, "disposable-settings")

    @staticmethod
    def settings(identifier):
        return Path.home() / "Library/Application Support" / identifier / "settings.json"

    @staticmethod
    def data_folders(identifier):
        return ()

    def __init__(self, folder):
        if not shutil.which("swiftc"):
            raise Unavailable("The macOS journey needs swiftc (Xcode Command Line Tools)", "native-driver")
        self.helper_path = folder / "journey-helper"
        source = Path(__file__).resolve().parent / "macos-journey.swift"
        try:
            subprocess.run(["swiftc", "-O", str(source), "-o", str(self.helper_path)], check=True,
                           capture_output=True, timeout=300)
        except (subprocess.SubprocessError, OSError):
            raise Unavailable("The macOS journey helper could not be built", "native-driver") from None
        if not self.helper("trusted")["accessibility"]:
            raise Unavailable("Give this terminal Accessibility access in System Settings > Privacy & Security", "accessibility-driver")
        self.app = None  # The Ibis process, set by the journey.
        self.last_picker = self.last_shown = None
        self.started_terminal = not running("Terminal")
        self.started_textedit = False
        self.collector = self.editor = None
        self.facts = {}

    def helper(self, *arguments):
        result = subprocess.run([str(self.helper_path), *map(str, arguments)], capture_output=True, text=True, timeout=20)
        try:
            answer = json.loads(result.stdout)
        except ValueError:
            raise RuntimeError("macOS journey helper gave no answer") from None
        if result.returncode or "error" in answer:
            raise RuntimeError(f"macOS journey helper refused: {answer.get('error', 'unknown')}")
        return answer

    def key(self, chord):
        *modifiers, name = chord.split("+")
        names = {"super": "cmd", "ctrl": "ctrl", "shift": "shift", "alt": "alt"}
        self.helper("key", name.lower(), *(names[modifier] for modifier in modifiers))

    def text(self, text):
        self.helper("type", text)

    def ibis_focused(self, value=None):
        """The picker is on screen and the system's focus is Ibis's search field.

        Keys are posted only after this, so they cannot reach another app."""
        picker = self.helper("picker", self.app.pid)
        self.last_picker = {"onscreen": "Ibis" in picker["onscreen"], "role": picker["role"],
                            "search": picker["label"] == "Search references",
                            "system_focus_is_ibis": picker["system_pid"] == self.app.pid,
                            "value_matches": value is None or picker["value"] == value}
        return all(self.last_picker[key] for key in ("onscreen", "search", "system_focus_is_ibis", "value_matches"))

    def ibis_strings(self):
        return self.helper("strings", self.app.pid)["strings"]

    def picker_onscreen(self):
        return "Ibis" in self.helper("picker", self.app.pid)["onscreen"]

    def notice_onscreen(self):
        return "Ibis notice" in self.helper("picker", self.app.pid)["onscreen"]

    def observe_notice(self, seen):
        """Record that the notice is on screen and, when readable, its words.

        While Ibis is inactive macOS often leaves the notice out of Ibis's
        AXWindows, and it ignores the mouse, so hit-testing reaches the app
        under it. Its words are best effort here; Linux checks them exactly."""
        seen["shown"] |= self.notice_onscreen()
        seen["words"].update(text for text in self.ibis_strings() if text.startswith(("Copied", "Pasted")))

    def ibis_shows(self, text):
        strings = self.ibis_strings()
        self.last_shown = {"strings": len(strings), "whole": text in strings,
                           "pieces": [word in " ".join(strings) for word in text.split()]}
        return shows(strings, text)

    def clipboard(self, *, timeout=20):
        return subprocess.run(["pbpaste"], capture_output=True, text=True, check=True, timeout=timeout).stdout

    def set_clipboard(self, text):
        subprocess.run(["pbcopy"], input=text, text=True, check=True, timeout=20)

    def front(self):
        return self.helper("front")

    def open_document(self, document):
        subprocess.run(["open", "-a", "TextEdit", str(document)], check=True, timeout=30)
        self.started_textedit = True
        def textedit():
            front = self.front()
            return front if front["bundle"] == "com.apple.TextEdit" else None
        wait_for(textedit, "TextEdit in front with the invented document")
        self.editor = textedit()["pid"]
        wait_for(lambda: self.document().get("text") == JOURNEY_DOCUMENT, "TextEdit shows the invented document")

    def document(self):
        return self.helper("document", self.editor)

    def select(self, start, length):
        self.helper("select", self.editor, start, length)

    def document_in_front(self):
        return self.front()["bundle"] == "com.apple.TextEdit"

    def open_collector(self, folder, received):
        script = folder / f"{COLLECTOR_TITLE}.command"
        script.write_text(f"#!/bin/sh\nexec {shlex.quote(sys.executable)} -c {shlex.quote(SYNTHETIC_EDITOR_SCRIPT)} "
                          f"{shlex.quote(str(received))}\n", encoding="utf-8")
        script.chmod(0o700)
        subprocess.run(["open", "-a", "Terminal", str(script)], check=True, timeout=30)
        self.collector = script
        wait_for(lambda: self.front()["bundle"] == "com.apple.Terminal", "Terminal in front with the collector")

    def close(self):
        if self.collector:
            subprocess.run(["pkill", "-f", COLLECTOR_TITLE], capture_output=True, timeout=10)
            terminal = subprocess.run(["pgrep", "-x", "Terminal"], capture_output=True, text=True, timeout=10).stdout.split()
            if self.started_terminal:
                subprocess.run(["pkill", "-x", "Terminal"], capture_output=True, timeout=10)
            elif terminal:
                self.helper("close", terminal[0], COLLECTOR_TITLE)
        if self.started_textedit:
            subprocess.run(["pkill", "-x", "TextEdit"], capture_output=True, timeout=10)


class Windows:
    """Notepad, a console and Ibis through Win32 calls and UI Automation.

    Reads only the journey's Notepad editor and Ibis's own windows. Keys go
    to Ibis only while its search field has the keyboard; the shortcut goes
    to the app in front, as a person's would, and an Alt tap may help
    Windows bring the journey's own window forward, as notepad_test.rs does."""

    name, terminal = "Notepad", "the console"
    reads_selection = False  # Ibis reads the selection as the search only on macOS.
    select_all = "ctrl+a"
    pasted, pasted_citation, pasted_notice = JOURNEY_PASTED, JOURNEY_CITATION, PASTED_NOTICE
    terminal_notice = TERMINAL_NOTICE.replace("⌘V", "Ctrl+V")
    copied_hint = COPIED_HINT.replace("⌘V", "Ctrl+V")
    VIRTUAL_KEYS = {"ctrl": 0x11, "shift": 0x10, "alt": 0x12, "super": 0x5B, "return": 0x0D, "escape": 0x1B}

    @staticmethod
    def preflight():
        tasks = subprocess.run(["tasklist", "/FI", "IMAGENAME eq notepad.exe", "/NH"], capture_output=True,
                               text=True, timeout=30).stdout
        if "notepad.exe" in tasks.lower():
            raise Unavailable("Close Notepad before the journey; it opens its own invented document", "native-session")
        tasks = subprocess.run(["tasklist", "/FI", "IMAGENAME eq ibis-desktop.exe", "/NH"], capture_output=True,
                               text=True, timeout=30).stdout
        if "ibis-desktop.exe" in tasks.lower():
            raise Unavailable(ANOTHER_IBIS, "disposable-settings")

    @staticmethod
    def settings(identifier):
        return Path(os.environ["APPDATA"]) / identifier / "settings.json"

    @staticmethod
    def data_folders(identifier):
        return (Path(os.environ["LOCALAPPDATA"]) / identifier,)  # WebView2's data.

    def __init__(self, folder):
        import ctypes
        from ctypes import wintypes
        # PowerShell 7 when installed, else the Windows PowerShell every Windows has.
        self.powershell = shutil.which("pwsh") or str(Path(os.environ.get("SystemRoot", r"C:\Windows"))
                                                       / "System32/WindowsPowerShell/v1.0/powershell.exe")
        if not Path(self.powershell).is_file():
            raise Unavailable("The Windows journey needs PowerShell", "native-driver")
        self.helper_path = Path(__file__).resolve().parent / "windows-journey.ps1"
        self.ctypes, self.wintypes = ctypes, wintypes
        user32 = self.user32 = ctypes.WinDLL("user32", use_last_error=True)
        kernel32 = self.kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        HWND, UINT = wintypes.HWND, wintypes.UINT
        self.Visitor = ctypes.WINFUNCTYPE(wintypes.BOOL, HWND, wintypes.LPARAM)
        for function, arguments, result in (
                (user32.EnumWindows, [self.Visitor, wintypes.LPARAM], wintypes.BOOL),
                (user32.EnumChildWindows, [HWND, self.Visitor, wintypes.LPARAM], wintypes.BOOL),
                (user32.GetWindowThreadProcessId, [HWND, ctypes.POINTER(wintypes.DWORD)], wintypes.DWORD),
                (user32.GetWindowTextW, [HWND, wintypes.LPWSTR, ctypes.c_int], ctypes.c_int),
                (user32.GetClassNameW, [HWND, wintypes.LPWSTR, ctypes.c_int], ctypes.c_int),
                (user32.IsWindowVisible, [HWND], wintypes.BOOL),
                (user32.GetForegroundWindow, [], HWND),
                (user32.SetForegroundWindow, [HWND], wintypes.BOOL),
                (user32.SendMessageTimeoutW, [HWND, UINT, wintypes.WPARAM, wintypes.LPARAM, UINT, UINT,
                                              ctypes.POINTER(ctypes.c_size_t)], ctypes.c_size_t),
                (user32.SendInput, [UINT, ctypes.c_void_p, ctypes.c_int], UINT),
                (user32.OpenClipboard, [HWND], wintypes.BOOL),
                (user32.CloseClipboard, [], wintypes.BOOL),
                (user32.EmptyClipboard, [], wintypes.BOOL),
                (user32.GetClipboardData, [UINT], wintypes.HANDLE),
                (user32.SetClipboardData, [UINT, wintypes.HANDLE], wintypes.HANDLE),
                (kernel32.GlobalAlloc, [UINT, ctypes.c_size_t], wintypes.HGLOBAL),
                (kernel32.GlobalLock, [wintypes.HGLOBAL], ctypes.c_void_p),
                (kernel32.GlobalUnlock, [wintypes.HGLOBAL], wintypes.BOOL),
                (kernel32.GlobalFree, [wintypes.HGLOBAL], wintypes.HGLOBAL),
                (user32.OpenInputDesktop, [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD], wintypes.HANDLE),
                (user32.CloseDesktop, [wintypes.HANDLE], wintypes.BOOL)):
            function.argtypes, function.restype = arguments, result
        # A locked, signed-out or disconnected session has no desktop to send
        # keys to or bring windows forward on (as over ssh without /IT).
        desktop = user32.OpenInputDesktop(0, False, 0x0100)  # DESKTOP_SWITCHDESKTOP
        if not desktop:
            raise Unavailable("Run the journey in a signed-in, unlocked Windows session", "native-session")
        user32.CloseDesktop(desktop)

        class KEYBDINPUT(ctypes.Structure):
            _fields_ = [("wVk", wintypes.WORD), ("wScan", wintypes.WORD), ("dwFlags", wintypes.DWORD),
                        ("time", wintypes.DWORD), ("dwExtraInfo", ctypes.c_size_t)]

        class MOUSEINPUT(ctypes.Structure):  # Only so the union has SendInput's size.
            _fields_ = [("dx", wintypes.LONG), ("dy", wintypes.LONG), ("mouseData", wintypes.DWORD),
                        ("dwFlags", wintypes.DWORD), ("time", wintypes.DWORD), ("dwExtraInfo", ctypes.c_size_t)]

        class INPUT(ctypes.Structure):
            class _Input(ctypes.Union):
                _fields_ = [("ki", KEYBDINPUT), ("mi", MOUSEINPUT)]
            _anonymous_ = ("input",)
            _fields_ = [("type", wintypes.DWORD), ("input", _Input)]

        self.INPUT, self.KEYBDINPUT = INPUT, KEYBDINPUT
        self.app = None  # The Ibis process, set by the journey.
        self.last_picker = self.last_shown = None
        self.notepad = self.window = self.editor = self.collector = self.collector_stop = None
        self.facts = {}  # Which Notepad and console host ran, for the report.

    # Windows, read through Win32 only.
    def windows(self, parent=None):
        found = []
        visit = self.Visitor(lambda window, _data: found.append(window) or True)
        if parent:
            self.user32.EnumChildWindows(parent, visit, 0)
        else:
            self.user32.EnumWindows(visit, 0)
        return [window for window in found if window and self.user32.IsWindowVisible(window)]

    def owner(self, window):
        pid = self.wintypes.DWORD()
        self.user32.GetWindowThreadProcessId(window, self.ctypes.byref(pid))
        return pid.value

    def title(self, window):
        text = self.ctypes.create_unicode_buffer(256)
        self.user32.GetWindowTextW(window, text, 256)
        return text.value

    def kind(self, window):
        text = self.ctypes.create_unicode_buffer(64)
        self.user32.GetClassNameW(window, text, 64)
        return text.value

    def message(self, window, message, wparam=0, lparam=0):
        result = self.ctypes.c_size_t()
        # SMTO_ABORTIFHUNG: a hung Notepad fails the read instead of the run.
        if not self.user32.SendMessageTimeoutW(window, message, wparam, lparam, 0x0002, 5000, self.ctypes.byref(result)):
            raise RuntimeError("A window did not answer")
        return result.value

    def owned_titles(self):
        return [self.title(window) for window in self.windows() if self.owner(window) == self.app.pid]

    def foreground(self):
        return self.user32.GetForegroundWindow()

    def activate(self, window):
        """Bring the journey's own window forward. SetForegroundWindow is
        refused to a process without recent input; an Alt tap gives it that."""
        for attempt in range(2):
            self.user32.SetForegroundWindow(window)
            try:
                wait_for(lambda: self.foreground() == window, "the journey's window comes to the front", timeout=5)
                return
            except AssertionError:
                if attempt:
                    raise
                self.send([(self.VIRTUAL_KEYS["alt"], False), (self.VIRTUAL_KEYS["alt"], True)])

    # Input, through SendInput.
    def send(self, keys, unicode=False):
        inputs = (self.INPUT * len(keys))()
        for item, (code, up) in zip(inputs, keys):
            item.type = 1  # INPUT_KEYBOARD
            flags = (0x0002 if up else 0) | (0x0004 if unicode else 0)  # KEYUP, UNICODE
            item.ki = self.KEYBDINPUT(0 if unicode else code, code if unicode else 0, flags, 0, 0)
        if self.user32.SendInput(len(keys), inputs, self.ctypes.sizeof(self.INPUT)) != len(keys):
            raise RuntimeError("Windows refused the journey's keys")

    def key(self, chord):
        *modifiers, name = chord.lower().split("+")
        codes = [self.VIRTUAL_KEYS[modifier] for modifier in modifiers]
        codes.append(self.VIRTUAL_KEYS.get(name) or ord(name.upper()))
        self.send([(code, False) for code in codes] + [(code, True) for code in reversed(codes)])

    def text(self, text):
        self.send([(ord(character), up) for character in text for up in (False, True)], unicode=True)

    # Ibis, through UI Automation.
    def snapshot(self):
        try:
            result = subprocess.run([self.powershell, "-NoProfile", "-NonInteractive", "-File", str(self.helper_path),
                                     "snapshot", str(self.app.pid)], capture_output=True, timeout=30,
                                    creationflags=subprocess.CREATE_NO_WINDOW)
            answer = json.loads(result.stdout.decode("utf-8"))
        except ValueError:
            raise RuntimeError("Windows journey helper gave no answer") from None
        if result.returncode or "error" in answer:
            raise RuntimeError(f"Windows journey helper refused: {answer.get('error', 'unknown')}")
        return answer

    def ibis_focused(self, value=None):
        """Ibis's picker is the foreground window and has the keyboard.

        Keys are sent only after this, so they cannot reach another app."""
        front = self.foreground()
        shot = self.snapshot()
        self.last_picker = {"onscreen": "Ibis" in shot["windows"], "search": shot["searches"] >= 1,
                            "focused": shot["focused"],
                            "system_focus_is_ibis": bool(front) and self.owner(front) == self.app.pid
                                                    and self.title(front) == "Ibis",
                            "value_matches": value is None or value in shot["values"]}
        ready = all(self.last_picker.values()) and self.foreground() == front
        self.last_picker["foreground"] = self.describe(front)  # A fixed kind, never a title.
        return ready

    def describe(self, window):
        if not window:
            return "none"
        if self.owner(window) == self.app.pid:
            return {"Ibis": "ibis-picker", NOTICE_TITLE: "ibis-notice"}.get(self.title(window), "ibis-other")
        if window == self.window:
            return "notepad"
        return {"ConsoleWindowClass": "console", "CASCADIA_HOSTING_WINDOW_CLASS": "windows-terminal",
                "Shell_TrayWnd": "taskbar"}.get(self.kind(window), "other")

    def picker_onscreen(self):
        return "Ibis" in self.owned_titles()

    def notice_onscreen(self):
        return NOTICE_TITLE in self.owned_titles()

    def observe_notice(self, seen):
        """The notice window by its title; its words when UI Automation shows them."""
        if self.notice_onscreen():
            seen["shown"] = True
            seen["words"].update(text for text in self.snapshot()["strings"] if text.startswith(("Copied", "Pasted")))

    def ibis_shows(self, text):
        strings = self.snapshot()["strings"]
        self.last_shown = {"strings": len(strings), "whole": text in strings,
                           "pieces": [word in " ".join(strings) for word in text.split()]}
        return shows(strings, text)

    # The clipboard, as Unicode text.
    @contextmanager
    def opened_clipboard(self, timeout):
        wait_for(lambda: self.user32.OpenClipboard(None), "the clipboard opens", timeout=timeout)
        try:
            yield
        finally:
            self.user32.CloseClipboard()

    def clipboard(self, *, timeout=20):
        with self.opened_clipboard(timeout):
            handle = self.user32.GetClipboardData(13)  # CF_UNICODETEXT
            if not handle:
                return ""
            pointer = self.kernel32.GlobalLock(handle)
            try:
                return self.ctypes.wstring_at(pointer) if pointer else ""
            finally:
                self.kernel32.GlobalUnlock(handle)

    def set_clipboard(self, text):
        data = text.encode("utf-16-le") + b"\0\0"
        with self.opened_clipboard(20):
            self.user32.EmptyClipboard()
            handle = self.kernel32.GlobalAlloc(0x0002, len(data))  # GMEM_MOVEABLE
            pointer = self.kernel32.GlobalLock(handle)
            self.ctypes.memmove(pointer, data, len(data))
            self.kernel32.GlobalUnlock(handle)
            if not self.user32.SetClipboardData(13, handle):
                self.kernel32.GlobalFree(handle)
                raise RuntimeError("The clipboard could not be written")

    # Notepad: the window titled with the invented document, and its one editor.
    def open_document(self, document):
        self.notepad = subprocess.Popen(["notepad.exe", str(document)])
        def window():
            # A packaged Notepad hands off to another process: match the
            # class and the document, never the launcher's process.
            found = [window for window in self.windows()
                     if self.kind(window) == "Notepad" and document.stem in self.title(window)]
            return found[0] if len(found) == 1 else None
        wait_for(window, "Notepad shows a window for the invented document", timeout=30)
        self.window = window()
        def editor():
            # Classic Notepad has one Edit; the modern one a RichEditD2DPT per
            # tab, so take the one showing the invented document.
            found = [child for child in self.windows(self.window) if self.kind(child) in ("Edit", "RichEditD2DPT")
                     and self.read(child) == JOURNEY_DOCUMENT]
            return found[0] if len(found) == 1 else None
        wait_for(editor, "Notepad shows the invented document")
        self.editor = editor()
        self.facts["notepad"] = {"Edit": "classic", "RichEditD2DPT": "modern"}[self.kind(self.editor)]
        self.activate(self.window)

    def read(self, editor):
        length = self.message(editor, 0x000E)  # WM_GETTEXTLENGTH
        text = self.ctypes.create_unicode_buffer(length + 1)
        self.message(editor, 0x000D, length + 1, self.ctypes.addressof(text))  # WM_GETTEXT
        # Notepad ends lines with CRLF (classic) or CR (modern); compare as LF.
        return text.value.replace("\r\n", "\n").replace("\r", "\n")

    def document(self):
        selection = self.message(self.editor, 0x00B0)  # EM_GETSEL: start, end
        start, end = selection & 0xFFFF, (selection >> 16) & 0xFFFF
        return {"text": self.read(self.editor), "selection": [start, end - start]}

    def select(self, start, length):
        self.message(self.editor, 0x00B1, start, start + length)  # EM_SETSEL

    def document_in_front(self):
        return self.foreground() == self.window

    def open_collector(self, folder, received):
        """cmd.exe owns the console, as when a person opens Command Prompt;
        a default terminal such as Windows Terminal may host it instead."""
        script = folder / f"{COLLECTOR_TITLE}.py"
        script.write_text(WINDOWS_COLLECTOR_SCRIPT, encoding="utf-8")
        self.collector_stop = Path(f"{received}.stop")
        # /s keeps the quoted command whole, spaces in paths included.
        self.collector = subprocess.Popen(f'cmd.exe /d /s /c ""{sys.executable}" "{script}" "{received}""',
                                          creationflags=subprocess.CREATE_NEW_CONSOLE)
        def console():
            found = [window for window in self.windows() if self.title(window).endswith(COLLECTOR_TITLE)]
            return found[0] if len(found) == 1 else None
        wait_for(console, "a console window runs the collector", timeout=30)
        self.facts["console"] = self.executable(self.owner(console()))
        self.activate(console())

    def executable(self, pid):
        """The owner's file name when it is a known console host, else "other"."""
        tasks = subprocess.run(["tasklist", "/FI", f"PID eq {pid}", "/FO", "CSV", "/NH"], capture_output=True,
                               text=True, timeout=30).stdout.lower()
        return next((name for name in ("cmd.exe", "conhost.exe", "windowsterminal.exe") if f'"{name}"' in tasks), "other")

    def close(self):
        if self.collector:
            self.collector_stop.touch()
            try:
                self.collector.wait(timeout=10)
            except subprocess.TimeoutExpired:
                subprocess.run(["taskkill", "/T", "/F", "/PID", str(self.collector.pid)], capture_output=True, timeout=30)
        if self.notepad:
            subprocess.run(["taskkill", "/F", "/IM", "notepad.exe"], capture_output=True, timeout=30)


class MacApp:
    """An Ibis process: started directly from an executable, or on macOS
    through LaunchServices from a bundle, so a bundle is its own paste-permission owner."""

    def __init__(self, path, library, log):
        self.bundle = path if path.suffix == ".app" else None
        # Ibis's own fixed-stage paste diagnostics, kept beside the evidence.
        environment = {"IBIS_LIBRARY": str(library), "IBIS_MAC_PASTE_DIAGNOSTICS": "1"}
        if self.bundle:
            self.marker = f"{path.resolve()}/Contents/MacOS/"
            subprocess.run(["open", "-n", "--env", f"IBIS_LIBRARY={library}", str(path)], check=True, timeout=30)
            self.process = None
            wait_for(lambda: self.running(), "Ibis started through LaunchServices")
            self.pid = self.running()[0]
        else:
            self.process = subprocess.Popen([str(path.resolve())], env=dict(os.environ, **environment),
                                            stdout=log, stderr=log)
            self.pid = self.process.pid

    def running(self):
        ps = subprocess.run(["ps", "-axww", "-o", "pid=,args="], capture_output=True, text=True, check=True, timeout=10)
        return [int(line.split(None, 1)[0]) for line in ps.stdout.splitlines()
                if len(line.split(None, 1)) == 2 and line.split(None, 1)[1].startswith(self.marker)]

    def poll(self):
        if self.process:
            return self.process.poll()
        return None if self.pid in self.running() else 0

    def stop(self):
        if self.poll() is not None:
            return
        if self.process:
            self.process.terminate()
        else:
            os.kill(self.pid, signal.SIGTERM)
        try:
            wait_for(lambda: self.poll() is not None, "Ibis stopped", timeout=10)
        except AssertionError:
            if self.process:
                self.process.kill()
            else:
                os.kill(self.pid, signal.SIGKILL)


def app_executable(path):
    """The file whose hash the evidence records: the bundle's executable."""
    if path.suffix != ".app":
        return path
    import plistlib
    with (path / "Contents/Info.plist").open("rb") as info:
        return path / "Contents/MacOS" / plistlib.load(info)["CFBundleExecutable"]


def running(name):
    return subprocess.run(["pgrep", "-x", name], capture_output=True, timeout=10).returncode == 0


def journey_main(args):
    return evidence_main(args, _journey_main, "native-journey")


def _journey_main(args):
    args.output.mkdir(parents=True, exist_ok=True)
    system = platform.system()
    report = {"platform": system, "scenario": "native-journey", "status": "failed", "checks": []}
    native = app = owned_settings = temporary = log = None
    created = []
    exit_code = 1

    def passed(name):
        report["checks"].append({"name": name, "status": "passed"})
        print(f"PASS {name}", flush=True)

    def remove_created():
        for path in created:
            # WebView2's helpers can hold its files for a moment after Ibis exits.
            wait_for(lambda: shutil.rmtree(path, ignore_errors=True) or not path.exists(),
                     "the journey's own Ibis folders are removed", timeout=15, interval=.5)

    try:
        driver = {"Darwin": MacOS, "Windows": Windows}.get(system)
        if not driver:
            raise Unavailable("The journey runs on macOS and Windows; Linux runs it inside the native smoke", "linux-x11")
        if not app_executable(args.app).is_file():
            raise Unavailable("Build or install the app before the journey", "app-build")
        driver.preflight()
        temporary = tempfile.TemporaryDirectory(prefix="ibis-journey-")
        folder = Path(temporary.name).resolve()
        library = folder / "journey.bib"
        library.write_text(JOURNEY_LIBRARY, encoding="utf-8")
        document = folder / "journey.txt"
        document.write_bytes(JOURNEY_DOCUMENT.encode())  # LF on every system
        settings = driver.settings(args.identifier)
        owned_settings = claim_settings(settings)
        # Folders this run creates are removed with all Ibis wrote in them,
        # so the next run on the same account starts clean too.
        created = [path for path in driver.data_folders(args.identifier) if not path.exists()]
        settings.write_bytes(JOURNEY_SETTINGS)
        native = observe_native(driver(folder))
        editor, terminal = native.name, native.terminal

        log = (args.output / "startup-journey.log").open("w", encoding="utf-8")
        with recording("action", "launch-journey"):
            app = MacApp(args.app, library, log)
        native.app = app
        # Ibis shows its picker once it starts; wait for the library, then hide it.
        wait_for(native.ibis_focused, "the first picker has the keyboard", timeout=60)
        native.key("Escape")
        wait_for(lambda: not native.picker_onscreen(), "Escape hides the first picker")
        passed("the app starts and Escape hides its first picker")

        with recording("action", "open-document"):
            native.open_document(document)
        start = JOURNEY_DOCUMENT.index(JOURNEY_QUERY)
        report["paste_attempts"] = 0
        while True:
            # Ibis copies instead when its last check before pasting cannot
            # finish in time, as on a busy runner. That safe copy, with the
            # document untouched and exactly the citation copied, earns one retry.
            report["paste_attempts"] += 1
            with recording("action", "select-placeholder"):
                native.select(start, len(JOURNEY_QUERY))
            assert native.document().get("selection") == [start, len(JOURNEY_QUERY)], f"{editor} did not select the placeholder"
            native.key(args.shortcut)
            if native.reads_selection:
                wait_for(lambda: native.ibis_focused(JOURNEY_QUERY), "the shortcut opens Ibis with the selection as its search")
            else:
                wait_for(native.ibis_focused, f"the shortcut opens Ibis from {editor}")
                native.key(native.select_all)
                native.text(JOURNEY_QUERY)
                wait_for(lambda: native.ibis_focused(JOURNEY_QUERY), "the search holds the typed query")
            wait_for(lambda: native.ibis_shows(JOURNEY_TITLE), "the invented reference is found")
            if report["paste_attempts"] == 1:
                passed(f"the shortcut opens the picker from {editor} and searches the selection" if native.reads_selection
                       else f"the shortcut opens the picker from {editor} and finds the typed query")
            prime_clipboard(native)
            native.key("Return")
            seen = {"shown": False, "words": set()}
            def pasted():
                if app.poll() is None:
                    native.observe_notice(seen)
                return native.document().get("text") == native.pasted and seen["shown"]
            try:
                wait_for(pasted, f"the citation replaces the selection in {editor}", timeout=15)
                break
            except AssertionError:
                text = native.document().get("text", "")
                report["paste_failure"] = {"document_unchanged": text == JOURNEY_DOCUMENT,
                                           "citation_in_document": native.pasted_citation in text,
                                           "copied_exactly": native.clipboard() == native.pasted_citation,
                                           "notice_shown": seen["shown"], "notice_words": sorted(seen["words"])}
                safe_copy = (report["paste_failure"]["document_unchanged"] and report["paste_failure"]["copied_exactly"]
                             and seen["words"] <= {native.copied_hint})
                if not safe_copy or report["paste_attempts"] == 2:
                    raise AssertionError(f"{editor} did not end up with exactly the chosen citation; notice: "
                                         + ("; ".join(sorted(seen["words"])) or "not read")) from None
                wait_for(lambda: not native.notice_onscreen(), "the notice goes away", timeout=15)
                wait_for(native.document_in_front, f"{editor} in front again")
        wrong = seen["words"] - {native.pasted_notice}
        assert not wrong, "The notice did not say it pasted: " + "; ".join(sorted(wrong))
        report["pasted_notice_read"] = native.pasted_notice in seen["words"]
        wait_for(lambda: not native.notice_onscreen(), "the notice goes away", timeout=15)
        assert native.document().get("text") == native.pasted, f"{editor} changed again after the paste: pasted more than once"
        passed(f"Enter pastes exactly the chosen citation into {editor}, once")

        # Refusal: a terminal runs pasted text as commands, so Ibis only copies.
        received = folder / "received"
        received.touch()
        with recording("action", "open-terminal"):
            native.open_collector(folder, received)
        native.key(args.shortcut)
        wait_for(native.ibis_focused, f"the shortcut opens Ibis from {terminal}")
        native.key(native.select_all)
        native.text(JOURNEY_QUERY)
        wait_for(lambda: native.ibis_focused(JOURNEY_QUERY), "the search holds the typed query")
        wait_for(lambda: native.ibis_shows(JOURNEY_TITLE), "the invented reference is found again")
        prime_clipboard(native)
        native.key("Return")
        seen = {"shown": False, "words": set()}
        def copied():
            if app.poll() is None:
                native.observe_notice(seen)
            return native.clipboard() == JOURNEY_CITATION and seen["shown"]
        try:
            wait_for(copied, "a copy and a notice", timeout=15)
        except AssertionError:
            clipboard = native.clipboard()
            report["terminal"] = {"clipboard": "expected" if clipboard == JOURNEY_CITATION else
                                  "sentinel" if clipboard == CLIPBOARD_SENTINEL else "other",
                                  "notice_shown": seen["shown"], "notice_words": sorted(seen["words"])}
            raise AssertionError(f"{terminal} did not get exactly the citation copied and a notice") from None
        wrong = seen["words"] - {native.terminal_notice}
        assert not wrong, "The notice did not give the terminal reason: " + "; ".join(sorted(wrong))
        report["terminal_notice_read"] = native.terminal_notice in seen["words"]
        wait_for(lambda: not native.notice_onscreen(), "the notice goes away", timeout=15)
        assert received.read_bytes() == b"", "Ibis typed or pasted into the terminal"
        passed("a terminal gets exactly the citation copied, a notice and no keystrokes")
        report["status"], exit_code = "passed", 0
    except Unavailable as error:
        unavailable(error.code)
        report.update(status="unavailable", reason=str(error))
        exit_code = 2
    except (AssertionError, RuntimeError, ValueError, KeyError, TypeError, subprocess.SubprocessError, OSError) as error:
        failure(error)
        report["reason"] = str(error) if isinstance(error, AssertionError) else type(error).__name__
        if native:
            report["last_picker"] = native.last_picker  # Booleans and a role name only.
            report["last_shown"] = native.last_shown
            report["app_running"] = bool(app) and app.poll() is None
    finally:
        for cleanup in (app.stop if app else None, log.close if log else None, native.close if native else None,
                        owned_settings.close if owned_settings else None,
                        remove_created, temporary.cleanup if temporary else None):
            try:
                if cleanup:
                    cleanup()
            except (OSError, subprocess.SubprocessError, RuntimeError, AssertionError) as error:
                failure(error, "cleanup")
                report.update(status="failed", reason="Journey cleanup failed")
                exit_code = 1
        if native:
            report.update(native.facts)
        print("NATIVE JOURNEY " + json.dumps(report), flush=True)
        (args.output / "journey.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return exit_code


def evidence_main(args, scenario, scenario_id):
    """Always persist the evidence manifest, including failures."""
    try:
        options = {"identifier": args.identifier, "shortcut": args.shortcut or "ctrl+shift+y",
                   "skip_performance": bool(getattr(args, "skip_performance", False))}
        fixtures = {"small": fixture().encode(), "initial-document": b"",
                    "initial-settings": b'{"schema_version": 2, "update_checks": false}'}
        if scenario_id == "native-journey":
            fixtures = {"journey": JOURNEY_LIBRARY.encode(), "initial-document": JOURNEY_DOCUMENT.encode(),
                        "initial-settings": JOURNEY_SETTINGS}
        if scenario_id == "native-smoke":
            fixtures.update(four=fixture(4).encode(), empty=b"",
                            damaged=(Path(__file__).resolve().parent.parent / "fixtures/bibtex/damaged.bib").read_bytes())
            if not options["skip_performance"]:
                fixtures["large"] = fixture(100000).encode()
        recorder = Evidence(scenario_id, options, collect_metadata(app_executable(args.app), fixtures))
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
        # Includes failures during setup and cleanup.
        recorder.failure(error)
        if isinstance(error, BundleError):
            # BundleError messages are fixed evidence refusals, safe to show.
            print(f"FAIL: {error}", file=sys.stderr)
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
    parser.add_argument("--journey", action="store_true", default=None, help="macOS and Windows: summon, search, paste into TextEdit or Notepad and read it back; then a terminal gets a copy")
    args = parser.parse_args()
    for key, value in {"identifier": "com.ibiscite.ibis", "shortcut": "ctrl+shift+y", "skip_performance": False}.items():
        if getattr(args, key) is None:
            setattr(args, key, value)
    if args.journey and args.shortcut == "ctrl+shift+y" and platform.system() == "Darwin":
        args.shortcut = "super+shift+y"  # The macOS default.
    if args.first_run_preferences:
        return first_run_preferences_main(args)
    if args.journey:
        return journey_main(args)
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
                code = f"check-{sys._getframe(1).f_lineno}"
                try:
                    with recording("request", code):
                        action()
                except Exception:
                    # Names are fixed scenario labels; no desktop text is printed.
                    print(f"FAILED CHECK {code}: {name}", file=sys.stderr)
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
                # Linux captures no destination app, so it uses the document suggestion.
                citation = "(Example, 2026)"
                # Installed release builds never report the non-activating panel as the
                # key window although Return still reaches it (#454), so the
                # key-window wait stays out of the native copy check.
                copy_with_diagnostics(native, report, expected=citation)
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
                    # The journey: Ibis says why it only copied, and the person's
                    # paste puts exactly the chosen citation into the editor.
                    wait_for(lambda: LINUX_NOTICE in strings(snapshot()), "the notice says why Ibis copied", timeout=5)
                    before = native.editor_received()
                    assert before["available"], "Synthetic editor exited before the paste"
                    native.key("shift+Insert")
                    wait_for(lambda: native.editor_received().get("text") == before["text"] + citation,
                             "the person's paste puts exactly the citation into the editor", timeout=5)

            def live_settings_copy():
                paths = [settings.with_suffix(suffix) for suffix in (".json", ".json.bak", ".json.tmp")]
                prior = {path: path.read_bytes() if path.exists() else None for path in paths}
                external = json.loads(prior[settings]) if prior[settings] is not None else {"schema_version": 1}
                external["larger_text"] = True
                external.pop("output", None)  # Copying must not recreate a removed preference.
                settings.parent.mkdir(parents=True, exist_ok=True)
                with recording("action", "settings-live-edit"):
                    settings.write_text(json.dumps(external), encoding="utf-8")
                external_bytes = settings.read_bytes()
                try:
                    copy()
                    saved = json.loads(settings.read_text(encoding="utf-8"))
                    assert saved.get("larger_text") is True, "Native copy lost a live external settings edit"
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
                    menu_action(native, "Reload Library")
                finally:
                    pass
                retained_fragments = {"problem_shown": False, "four_references": False}
                def explained():
                    nonlocal retained_fragments
                    current = snapshot()
                    report["missing_library_evidence"] = missing_library_evidence(current, library.name)
                    retained_fragments = retained_snapshot_fragments(current, library.name)
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
                menu_action(native, "Reload Library")
                wait_for(lambda: shows_empty_library(snapshot()), "empty library count")
                with recording("action", "fixture-small"):
                    library.write_text(fixture(), encoding="utf-8")
                menu_action(native, "Reload Library")
                wait_for(lambda: shows_count(snapshot(), "3 references"), "reload recovers after error")
            check("empty library and recovery after error", recover)
            stop()

            def damaged():
                with recording("action", "fixture-damaged"):
                    library.write_text((Path(__file__).resolve().parent.parent / "fixtures/bibtex/damaged.bib").read_text(encoding="utf-8"), encoding="utf-8")
                launch("damaged")
                wait_for(lambda: library.name in strings(snapshot()) and "4 entries skipped" in strings(snapshot()),
                         "damaged library starts by saying it skipped 4 entries")
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
        report.update(reason=str(error) if isinstance(error, (AssertionError, BundleError, Unavailable)) else type(error).__name__)
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
                owned_settings.close()
            except OSError as error:
                failure(error, "cleanup")
                report.update(status="failed", reason="Disposable settings cleanup failed")
                exit_code = 1
        report["elapsed_seconds"] = round(time.monotonic() - started, 2)
        (args.output / "smoke.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return exit_code


if __name__ == "__main__":
    if sys.argv[1:2] == ["--linux-failure-census"]:
        try:
            if platform.system() != "Linux" or len(sys.argv) != 4:
                raise ValueError("Private census mode unavailable")
            linux_census_worker(int(sys.argv[2]), int(sys.argv[3]))
        except Exception:
            sys.exit(1)
    else:
        sys.exit(main())
