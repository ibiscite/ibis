#!/usr/bin/env python3
"""Verify the macOS picker over a full-screen synthetic editor and its chooser.

Requires the native adapters from #217 and a disposable interactive macOS
session. Contract self-tests do not establish native acceptance.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import unittest


class Unavailable(RuntimeError):
    pass


def flags(state, names):
    if not isinstance(state, dict) or any(type(state.get(name)) is not bool for name in names):
        raise RuntimeError("Native window state is incomplete")
    return state


def fullscreen_ready(state, expected=True):
    state = flags(state, ("available", "visible"))
    if not state["available"]:
        if state.get("fullscreen", False) is not None:
            raise RuntimeError("Unavailable fullscreen state must be unknown")
        return False
    state = flags(state, ("fullscreen",))
    return state["fullscreen"] is expected and state["visible"]


def chooser_ready(state):
    state = flags(state, ("visible", "focused", "picker_visible"))
    return state["visible"] and state["focused"] and state["picker_visible"]


def load_native():
    path = Path(__file__).with_name("smoke-native.py")
    if not path.is_file():
        raise Unavailable("Integrate the native smoke adapters before running the panel scenario")
    spec = importlib.util.spec_from_file_location("ibis_native_smoke", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    report = {"platform": platform.system(), "status": "failed", "checks": []}
    native = smoke = app = log = owned = None
    exit_code = 1
    try:
        if platform.system() != "Darwin":
            raise Unavailable("The full-screen panel scenario requires an interactive macOS session")
        if not args.app.is_file():
            raise Unavailable("Build the embedded native app before running the panel scenario")
        smoke = load_native()
        with tempfile.TemporaryDirectory(prefix="ibis panel smoke ") as temporary:
            folder = Path(temporary)
            owned = smoke.claim_settings(smoke.settings_path("Darwin", folder))
            native = smoke.Scripted("Darwin")
            library = folder / "synthetic 引用.bib"
            library.write_text(smoke.fixture(), encoding="utf-8")
            log = (args.output / "startup-panel.log").open("w", encoding="utf-8")
            env = dict(os.environ, IBIS_LIBRARY=str(library))
            env.pop("IBIS_GALLERY", None)
            env.pop("IBIS_GALLERY_STATE", None)
            app = subprocess.Popen([str(args.app.resolve())], env=env, stdout=log, stderr=log)
            native.app = app

            def snapshot():
                assert app.poll() is None, "Native app exited during panel checks"
                return native.snapshot()

            def picker_ready():
                current = snapshot()
                return current["visible"] and current["foreground"] and smoke.search_focused(current)

            def editor_state():
                return native.call("editor-window-state")

            def chooser_state():
                return flags(native.call("chooser-state"), ("visible", "focused", "picker_visible"))

            def passed(name):
                report["checks"].append({"name": name, "status": "passed"})

            smoke.wait_for(lambda: "3 references" in smoke.strings(snapshot()), "synthetic panel library startup")
            smoke.prepare_initial_picker_show(native, snapshot)
            native.focus_editor()
            smoke.wait_for(native.editor_focused, "synthetic editor focused")
            native.call("set-editor-fullscreen", "true")
            smoke.wait_for(lambda: fullscreen_ready(editor_state()), "editor visibly entered its full-screen Space")
            native.key("ctrl+super+r")
            smoke.wait_for(picker_ready, "picker visible with focused search over full-screen editor")
            assert fullscreen_ready(editor_state()), "Summoning the picker switched away from the full-screen editor"
            passed("picker appears over the editor in its full-screen Space")

            smoke.prime_clipboard(native)
            native.key("super+a")
            native.text("smoke000001")
            smoke.wait_for(lambda: "Ibis smoke reference 000001" in smoke.strings(snapshot()), "native search over full-screen editor")
            native.key("Return")
            smoke.wait_for(lambda: native.clipboard() == "[@smoke000001]", "citation copied from full-screen picker")
            smoke.wait_for(lambda: not snapshot()["visible"], "copy hides the panel")
            smoke.wait_for(lambda: native.editor_focused() and fullscreen_ready(editor_state()), "copy returns to the visible full-screen editor")
            passed("Enter copies, hides, and returns to the full-screen editor")

            native.key("ctrl+super+r")
            smoke.wait_for(picker_ready, "panel reopens over the full-screen editor")
            assert fullscreen_ready(editor_state()), "Reopening the picker switched away from the full-screen editor"
            smoke.escape_to_hide(native, snapshot, "Escape hides the panel")
            smoke.wait_for(lambda: native.editor_focused() and fullscreen_ready(editor_state()), "Escape returns to the visible full-screen editor")
            passed("Escape returns to the full-screen editor")

            native.call("set-editor-fullscreen", "false")
            smoke.wait_for(lambda: fullscreen_ready(editor_state(), False), "editor leaves full screen")
            native.key("ctrl+super+r")
            smoke.wait_for(picker_ready, "panel search focused before opening its chooser")
            smoke.menu_action(native, "Choose library…")
            smoke.wait_for(lambda: chooser_ready(chooser_state()), "native chooser receives focus while the picker stays visible")
            passed("library chooser is visible and focused without hiding the panel")
            native.call("cancel-chooser")
            smoke.wait_for(lambda: not chooser_state()["visible"], "native chooser closes after Cancel")
            def picker_has_focus():
                current = snapshot()
                return current["visible"] and any(node.get("focused") is True for node in current["nodes"])
            # Native cancellation may return to the chooser trigger rather
            # than the search box; both keep keyboard access to the picker.
            smoke.wait_for(picker_has_focus, "Cancel returns keyboard focus to the picker")
            smoke.escape_to_hide(native, snapshot, "dismissal after chooser hides the panel")
            smoke.wait_for(native.editor_focused, "dismissal after chooser returns to editor")
            passed("Cancel and dismissal restore picker and previous editor focus")
        report["status"] = "passed"
        exit_code = 0
    except Exception as error:
        unavailable = isinstance(error, Unavailable) or (smoke is not None and isinstance(error, smoke.Unavailable))
        report["status"] = "unavailable" if unavailable else "failed"
        report["reason"] = str(error) if isinstance(error, (AssertionError, RuntimeError)) else type(error).__name__
        exit_code = 2 if unavailable else 1
    finally:
        if app and app.poll() is None:
            app.terminate()
            try:
                app.wait(timeout=10)
            except subprocess.TimeoutExpired:
                app.kill()
                app.wait(timeout=5)
        if log:
            log.close()
        if native:
            for cleanup in (lambda: native.call("set-editor-fullscreen", "false"), native.close):
                try:
                    cleanup()
                except (OSError, RuntimeError, subprocess.SubprocessError, ValueError):
                    # Keep the first failure's reason; cleanup often fails after it.
                    if report.get("status") != "failed":
                        report.update(status="failed", reason="Synthetic editor cleanup failed")
                    exit_code = 1
        if owned:
            try:
                smoke.clean_settings(owned)
            except OSError:
                report.update(status="failed", reason="Disposable settings cleanup failed")
                exit_code = 1
        (args.output / "panel-smoke.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report))
    return exit_code


class ContractTests(unittest.TestCase):
    def test_unavailable_fullscreen_state_never_satisfies_exit_readiness(self):
        unavailable = {"available": False, "fullscreen": None, "visible": True}
        self.assertTrue(not unavailable["fullscreen"] and unavailable["visible"],
                        "Negating an unknown fullscreen state reproduces the unsafe exit predicate")
        self.assertFalse(fullscreen_ready(unavailable, False))
        self.assertFalse(fullscreen_ready(unavailable))
        actual = {"available": True, "fullscreen": False, "visible": True}
        self.assertTrue(fullscreen_ready(actual, False))
        self.assertFalse(fullscreen_ready(actual))

    def test_transition_readiness_requires_fresh_available_boolean_and_on_screen_window(self):
        for current in (
            {"available": False, "fullscreen": None, "visible": True},
            {"available": True, "fullscreen": False, "visible": False},
            {"available": True, "fullscreen": True, "visible": True},
        ):
            self.assertFalse(fullscreen_ready(current, False))
        self.assertTrue(fullscreen_ready({"available": True, "fullscreen": False, "visible": True}, False))

    def test_malformed_available_fullscreen_state_is_an_error(self):
        for current in (
            {"available": "true", "fullscreen": False, "visible": True},
            {"available": True, "fullscreen": None, "visible": True},
            {"available": False, "fullscreen": False, "visible": True},
        ):
            with self.assertRaises(RuntimeError):
                fullscreen_ready(current, False)

    def test_fullscreen_flag_alone_cannot_hide_a_space_switch(self):
        self.assertFalse(fullscreen_ready({"available": True, "fullscreen": True, "visible": False}))
        self.assertTrue(fullscreen_ready({"available": True, "fullscreen": True, "visible": True}))

    def test_chooser_must_receive_focus_while_the_picker_remains_visible(self):
        self.assertFalse(chooser_ready({"visible": True, "focused": False, "picker_visible": True}))
        self.assertFalse(chooser_ready({"visible": True, "focused": True, "picker_visible": False}))
        self.assertTrue(chooser_ready({"visible": True, "focused": True, "picker_visible": True}))

    def test_missing_or_non_boolean_native_flags_are_not_a_hidden_window(self):
        for state in ({}, {"fullscreen": True}, {"fullscreen": True, "visible": "true"}):
            with self.assertRaises(RuntimeError):
                fullscreen_ready(state)
        with self.assertRaises(RuntimeError):
            chooser_ready({"visible": True, "focused": True})


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        unittest.main(argv=[sys.argv[0]])
    else:
        sys.exit(main())
