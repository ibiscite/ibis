#!/usr/bin/env python3
"""Exercise the embedded native picker; never substitute a browser or IPC mock.

Exit 1 means a failed assertion; exit 2 means the session lacks a required native
capability. Only synthetic test text is recorded, never the accessibility tree or
clipboard contents. See docs/testing.md before running on a personal desktop.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, call, patch


class Unavailable(RuntimeError):
    pass


def run(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.PIPE, timeout=20).strip()


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


def missing_library_evidence(snapshot, name):
    text = strings(snapshot)
    return {key: fragment in text for key, fragment in {
        "failed_source_named": f"The library could not be opened: {name}",
        "retention_explained": "the library file is unavailable; keeping the last loaded references",
        "recovery_action": "Choose it again or reconnect the drive.",
        "retained_snapshot": f"{name}: 4 references",
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


def wait_for_query(snapshot, query, expected, report):
    last = {"nodes": []}
    def matched():
        nonlocal last
        last = snapshot()
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
    if any(candidate.with_suffix(suffix).exists() for suffix in (".json", ".json.bak", ".json.tmp")):
        raise Unavailable("Use a disposable desktop account without existing Ibis settings")
    return candidate


def clean_settings(owned):
    for suffix in (".json", ".json.bak", ".json.tmp"):
        owned.with_suffix(suffix).unlink(missing_ok=True)


def prime_clipboard(native):
    sentinel = "Ibis native smoke clipboard sentinel"
    native.set_clipboard(sentinel)
    assert native.clipboard() == sentinel, "Native clipboard sentinel could not be established"


def summarize_performance(samples):
    return {"status": "measured-without-budget", "references": 100000,
            "sample_count": len(samples), "samples": samples,
            "summary": "per-metric maxima from fresh app processes; OS file caches persist",
            **{metric: max(sample[metric] for sample in samples)
               for metric in ("startup_ms", "first_result_ms", "memory_bytes")},
            "memory_kind": "summed current process-tree RSS/working set; shared pages may be counted more than once"}


def enforce_performance_budgets(performance, budgets):
    ceilings = {metric: budgets[metric] for metric in ("startup_ms", "first_result_ms", "memory_bytes")}
    performance.update(status="failed", ceilings=ceilings)
    for metric, ceiling in ceilings.items():
        if not 0 < performance[metric] <= ceiling:
            performance["failed_metric"] = metric
            raise AssertionError(f"100k {metric} exceeded measured baseline budget")
    performance["status"] = "passed"


def require_tree(snapshot):
    for name in ("Search references", "Search results", "Results"):
        assert named(snapshot, name), f"Accessibility tree omits named {name!r}"
    search = named(snapshot, "Search references")
    assert any(n["role"].lower().replace(" ", "") in EDITABLE_ROLES for n in search), "Named search is not an editable control"
    results = named(snapshot, "Results")
    assert any("list" in n["role"].lower() and "item" not in n["role"].lower() for n in results), "Named Results is not a list"


class Linux:
    def __init__(self):
        for program in ("xdotool", "xclip", "xterm", "openbox"):
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
        self.editor = subprocess.Popen(["xterm", "-title", "Ibis synthetic editor", "-e", "/bin/cat"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.app = None

    def window(self, pid):
        try:
            return run("xdotool", "search", "--onlyvisible", "--pid", str(pid)).splitlines()[0]
        except (subprocess.CalledProcessError, IndexError):
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

    def clipboard(self):
        return run("xclip", "-selection", "clipboard", "-out")

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
                subprocess.run(["open", "-n", "-a", "TextEdit", str(self.editor_path)], check=True,
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

    def call(self, action, text=""):
        if self.system == "Windows" and action in ("focus-editor", "editor-focused", "close-editor"):
            text = str(self.editor_started)
        if getattr(self, "last_action", None) != action:
            print(f"NATIVE ACTION {action}", flush=True)
        self.last_action = action
        try:
            result = subprocess.run(self.command + [action, str(getattr(getattr(self, "app", None), "pid", 0)), str(getattr(self, "editor_pid", 0)), text], encoding="utf-8", stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
        except subprocess.TimeoutExpired as error:
            # Never print TimeoutExpired itself: its command contains paths and
            # synthetic clipboard/query arguments. The fixed action is enough.
            for stage in helper_timeout_stages(error.stderr):
                print(stage, file=sys.stderr)
            raise AssertionError(f"Native helper timed out during {action} after 30s") from None
        if result.returncode:
            # Helpers emit fixed diagnostics only, never tree or clipboard text.
            print(result.stderr, file=sys.stderr)
            raise subprocess.CalledProcessError(result.returncode, self.command, stderr=result.stderr)
        return json.loads(result.stdout)

    def snapshot(self):
        if self.system != "Windows":
            return self.call("snapshot")
        if getattr(self, "picker_app_pid", None) != self.app.pid:
            self.picker_handle = 0
            self.picker_app_pid = self.app.pid
        current = self.call("snapshot", str(self.picker_handle))
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

    def click(self, name):
        if self.system != "Darwin":
            self.call("click", name)
            return
        state = {}
        def pressed():
            nonlocal state
            state = self.call("click", name)
            if not isinstance(state, dict) or type(state.get("pressed")) is not bool:
                raise RuntimeError("Native button action metadata unavailable")
            return state["pressed"]
        try:
            wait_for(pressed, "exact named native button becomes available")
        except AssertionError:
            print("NATIVE CONTROL STATE " + json.dumps(state), flush=True)
            raise

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

    def clipboard(self):
        return self.call("clipboard")

    def set_clipboard(self, text):
        self.call("write-clipboard", text)

    def memory(self):
        return self.call("memory")

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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--identifier", default="io.github.ibiscite.ibis", help="Bundle identifier, which names the settings folder")
    parser.add_argument("--shortcut", help="xdotool-style chord; defaults to Ctrl+Shift+Y or Control+Command+R on macOS")
    parser.add_argument("--budgets", type=Path, help="Measured per-platform baseline JSON, with startup_ms, first_result_ms, memory_bytes ceilings")
    args = parser.parse_args()
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
            # Copying saves the format even with IBIS_LIBRARY, so clean only
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
                action()
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
                native.key("ctrl+a" if system != "Darwin" else "super+a")
                native.text(text)
                wait_for_query(snapshot, text, expected, report)

            shortcut = args.shortcut or ("ctrl+super+r" if system == "Darwin" else "ctrl+shift+y")
            launch("small")
            check("startup", lambda: wait_for(lambda: "3 references" in strings(snapshot()), "synthetic library startup"))
            check("accessibility named search/count/list", lambda: require_tree(snapshot()))

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

            def copy():
                prime_clipboard(native)
                native.key("Return")
                wait_for(lambda: native.clipboard() == "[@smoke000001]", "expected citation on OS clipboard")
                wait_for(lambda: not snapshot()["visible"], "copy hides picker")
                capture_editor_focus("after-copy-hides-picker")
                try:
                    wait_for(native.editor_focused, "copy restores focus to previous editor")
                except AssertionError:
                    capture_editor_focus("copy-restoration-timeout")
                    raise

            def live_settings_copy():
                paths = [settings.with_suffix(suffix) for suffix in (".json", ".json.bak", ".json.tmp")]
                prior = {path: path.read_bytes() if path.exists() else None for path in paths}
                external = json.loads(prior[settings]) if prior[settings] is not None else {"schema_version": 1}
                external["shortcut"] = "Control+Alt+F12"
                external.pop("copy_format", None)  # Ensure the native copy must save its format.
                settings.parent.mkdir(parents=True, exist_ok=True)
                settings.write_text(json.dumps(external), encoding="utf-8")
                try:
                    copy()
                    saved = json.loads(settings.read_text(encoding="utf-8"))
                    assert saved.get("shortcut") == external["shortcut"], "Native copy lost a live external settings edit"
                    assert saved.get("copy_format") == "pandoc", "Native copy did not save its format"
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
                wait_for(lambda: "4 references" in strings(snapshot()), "file watcher updates reference count")
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
                wait_for(lambda: "0 references" in strings(snapshot()), "empty library count")
                library.write_text(fixture(), encoding="utf-8")
                menu_action(native, "Reload")
                wait_for(lambda: "3 references" in strings(snapshot()), "reload recovers after error")
            check("empty library and recovery after error", recover)
            stop()

            def settings_error(label, content, problem):
                settings.parent.mkdir(parents=True, exist_ok=True)
                settings.write_text(content, encoding="utf-8")
                backup = settings.with_suffix(".json.bak")
                if backup.exists():
                    backup.unlink()
                launch(label)
                wait_for(lambda: "3 references" in strings(snapshot()), f"{label} retains library access")
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
                wait_for(lambda: any(f"{library.name}: 4 references, 5 {suffix}" in strings(snapshot())
                                    for suffix in ("could not be read fully", "reading notices")),
                         "damaged library shows exact retained-reference and reading-notice counts")
                query("good2", "Loads after the damage")
                stop()
            check("damaged startup library warns and keeps valid records searchable", damaged)

            library.write_text(fixture(100000), encoding="utf-8")
            samples = []
            for sample in range(3):
                native.focus_editor()
                before = time.monotonic()
                launch(f"100000-{sample + 1}")
                wait_for(lambda: "100000 references" in strings(snapshot()), "100,000-reference library load", timeout=120)
                startup = (time.monotonic() - before) * 1000
                before = time.monotonic()
                query("smoke099999", "Ibis smoke reference 099999")
                first = (time.monotonic() - before) * 1000
                memory = native.memory()
                assert memory > 0, "Native process-tree memory measurement unavailable"
                samples.append({"startup_ms": round(startup, 2), "first_result_ms": round(first, 2), "memory_bytes": memory})
                stop()
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

    def test_global_macos_hid_event_contracts_without_native_execution(self):
        subprocess.run(["node", str(Path(__file__).with_name("smoke-macos.test.cjs"))], check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=20)

    def test_macos_click_reacquires_a_missing_control_and_stops_after_one_press(self):
        native = Scripted.__new__(Scripted)
        native.system = "Darwin"
        with patch.object(native, "call", side_effect=[{"pressed": False}, {"pressed": True}]) as helper, patch("time.sleep"):
            native.click("Reload")
        self.assertEqual(helper.call_args_list, [(('click', 'Reload'),), (('click', 'Reload'),)])

    def test_scripted_native_json_preserves_unicode_under_a_legacy_windows_locale(self):
        native = Scripted.__new__(Scripted)
        native.system = "Windows"
        expected = "synthetic 引用.bib"
        payload = json.dumps(expected, ensure_ascii=False).encode("utf-8").hex()
        native.command = [sys.executable, "-c", f"import sys; sys.stdout.buffer.write(bytes.fromhex('{payload}'))"]
        with patch("locale.getencoding", return_value="cp1252"), patch("builtins.print"):
            self.assertEqual(native.call("snapshot"), expected)

    def test_missing_library_requires_both_recovery_message_and_retained_snapshot(self):
        name = "synthetic 引用.bib"
        message = (f"The library could not be opened: {name}. "
                   "the library file is unavailable; keeping the last loaded references. "
                   "Choose it again or reconnect the drive.")
        current = {"nodes": [{"name": message}, {"value": f"{name}: 4 references"}]}
        self.assertTrue(all(missing_library_evidence(current, name).values()))
        current["nodes"].pop()
        evidence = missing_library_evidence(current, name)
        self.assertFalse(evidence["retained_snapshot"])
        self.assertTrue(evidence["recovery_action"])
        self.assertTrue(all(type(value) is bool for value in evidence.values()))
        current["nodes"][0]["name"] = message[:100]
        self.assertFalse(missing_library_evidence(current, name)["recovery_action"])

    def test_performance_budget_rejects_a_single_slow_sample_even_when_the_mean_is_below_budget(self):
        performance = summarize_performance([
            {"startup_ms": 100, "first_result_ms": 10, "memory_bytes": 1000},
            {"startup_ms": 200, "first_result_ms": 9, "memory_bytes": 900},
            {"startup_ms": 50, "first_result_ms": 11, "memory_bytes": 999},
        ])
        with self.assertRaisesRegex(AssertionError, "startup_ms"):
            enforce_performance_budgets(performance, {"startup_ms": 150, "first_result_ms": 15, "memory_bytes": 2000})
        self.assertEqual(performance["status"], "failed")
        self.assertEqual(performance["failed_metric"], "startup_ms")
        self.assertEqual(performance["ceilings"], {"startup_ms": 150, "first_result_ms": 15, "memory_bytes": 2000})
        self.assertEqual([sample["startup_ms"] for sample in performance["samples"]], [100, 200, 50])

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
    else:
        sys.exit(main())
