"""Real GTK/X11 control verification, under an isolated Xvfb/dbus session."""
import argparse
import importlib
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


NATIVE_TOOLS = ("Xvfb", "xvfb-run", "xauth", "dbus-run-session", "cc", "xdotool", "xprop", "openbox", "xdpyinfo")
GTK_PREFLIGHT = """
import gi
gi.require_version('Gtk', '3.0')
gi.require_version('GdkX11', '3.0')
from gi.repository import Gdk, GdkX11, Gtk
result = Gtk.init_check()
if not (result[0] if isinstance(result, tuple) else result):
    raise SystemExit(2)
if not isinstance(Gdk.Display.get_default(), GdkX11.X11Display):
    raise SystemExit(3)
print('gtk-ready')
"""


def native_preflight():
    display = os.environ.get("DISPLAY", "")
    if not re.fullmatch(r":[0-9]+(?:\.[0-9]+)?", display):
        raise AssertionError("GTK regression requires a local DISPLAY supplied by xvfb-run")
    for tool in NATIVE_TOOLS:
        if shutil.which(tool) is None:
            raise AssertionError(f"GTK regression requires {tool}")
    try:
        # Use the same interpreter and inherited display as the actual fixture;
        # CI invokes /usr/bin/python3 so distro GI/GTK typelibs are available.
        result = subprocess.run([sys.executable, "-c", GTK_PREFLIGHT], capture_output=True,
                                timeout=5, env=dict(os.environ))
        if result.returncode or result.stdout.strip() != b"gtk-ready":
            raise AssertionError("GTK regression Python GI/GTK3 initialization failed on DISPLAY")
        extensions = subprocess.run(["xdpyinfo", "-display", display, "-queryExtensions"], capture_output=True,
                                    timeout=5, env=dict(os.environ))
        if extensions.returncode or not re.search(rb"\bXTEST\b", extensions.stdout):
            raise AssertionError("GTK regression DISPLAY requires the XTEST keyboard extension")
    except (OSError, subprocess.SubprocessError):
        raise AssertionError("GTK regression setup probe failed") from None


class SetupTests(unittest.TestCase):
    def test_ci_interpreter_and_display_reach_gtk_and_xtest_probes(self):
        results = [subprocess.CompletedProcess([], 0, b"gtk-ready\n", b""),
                   subprocess.CompletedProcess([], 0, b"    XTEST (opcode: 132)\n", b"")]
        with patch.dict(os.environ, {"DISPLAY": ":77.0"}), patch("shutil.which", return_value="/synthetic/tool"), \
                patch("subprocess.run", side_effect=results) as run:
            native_preflight()
            self.assertEqual(run.call_args_list[0].args[0][:2], [sys.executable, "-c"])
            self.assertEqual(run.call_args_list[1].args[0], ["xdpyinfo", "-display", ":77.0", "-queryExtensions"])
            for call in run.call_args_list:
                self.assertEqual(call.kwargs["env"]["DISPLAY"], ":77.0")
                self.assertEqual(call.kwargs["timeout"], 5)

    def test_missing_runner_tools_fail_before_launch(self):
        for missing in NATIVE_TOOLS:
            with self.subTest(tool=missing), patch.dict(os.environ, {"DISPLAY": ":77"}), \
                    patch("shutil.which", side_effect=lambda tool: None if tool == missing else "/synthetic/tool"), \
                    patch("subprocess.run") as run:
                with self.assertRaisesRegex(AssertionError, "requires " + re.escape(missing)):
                    native_preflight()
                run.assert_not_called()

    def test_missing_or_nonlocal_display_fails_before_launch(self):
        for display in ("", "remote:0"):
            with self.subTest(display=display), patch.dict(os.environ, {"DISPLAY": display}), patch("subprocess.run") as run:
                with self.assertRaisesRegex(AssertionError, "local DISPLAY"):
                    native_preflight()
                run.assert_not_called()

    def test_unavailable_gi_gtk_or_xtest_and_probe_timeouts_fail(self):
        ready = subprocess.CompletedProcess([], 0, b"gtk-ready\n", b"")
        bad = subprocess.CompletedProcess([], 1, b"", b"synthetic-private-sentinel")
        for results, label in (([bad], "GI/GTK3"), ([ready, bad], "XTEST"),
                               ([ready, subprocess.CompletedProcess([], 0, b"no extensions", b"")], "XTEST"),
                               ([subprocess.TimeoutExpired("probe", 5)], "setup probe failed")):
            with self.subTest(label=label), patch.dict(os.environ, {"DISPLAY": ":77"}), \
                    patch("shutil.which", return_value="/synthetic/tool"), patch("subprocess.run", side_effect=results):
                with self.assertRaisesRegex(AssertionError, label) as failure:
                    native_preflight()
                self.assertNotIn("synthetic-private-sentinel", str(failure.exception))


def fixture_output(child, output, timeout=3):
    child.wait(timeout=timeout)
    with output.open("rb") as source:
        return source.read(64)


@unittest.skipUnless(os.name == "posix", "requires an owned POSIX process group")
class FixtureCompletionTests(unittest.TestCase):
    def test_finished_fixture_does_not_wait_for_inherited_stdio(self):
        with tempfile.TemporaryDirectory(dir=Path.cwd()) as temporary:
            output = Path(temporary) / "fixture.stdout"
            # A service retains the pipes after the actual fixture has exited.
            code = ("import subprocess, sys; from pathlib import Path; "
                    "subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)']); "
                    "Path(sys.argv[1]).write_bytes(b'install\\n')")
            child = subprocess.Popen([sys.executable, "-c", code, str(output)], start_new_session=True,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                self.assertEqual(fixture_output(child, output, timeout=1), b"install\n")
                self.assertEqual(child.returncode, 0)
            finally:
                try:
                    os.killpg(child.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                child.wait(timeout=3)
                child.stdout.close()
                child.stderr.close()


def fixture(layout):
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gdk, GLib, Gtk
    picker = Gtk.Window(title="Ibis")
    picker.show_all()
    offer = Gtk.MessageDialog(title="An update is available", text="An update is available",
                              transient_for=picker, modal=True, use_header_bar=0)
    install_response = Gtk.ResponseType.OK if layout == "old" else Gtk.ResponseType.ACCEPT
    order = [("Install and Restart", install_response), ("Later", Gtk.ResponseType.CANCEL)]
    if layout == "new":
        order.reverse()
    if layout == "missing":
        order = [("Later", Gtk.ResponseType.CANCEL)]
    buttons = [offer.add_button(label, response) for label, response in order]
    keyboard = {"press": 0, "release": 0, "sent": 0}
    def key_event(_dialog, event):
        if event.keyval == Gdk.KEY_space:
            keyboard["press" if event.type == Gdk.EventType.KEY_PRESS else "release"] += 1
            keyboard["sent"] += int(bool(event.send_event))
        return False
    offer.connect("key-press-event", key_event)
    offer.connect("key-release-event", key_event)
    def answer(_dialog, response):
        deliberate = keyboard == {"press": 1, "release": 1, "sent": 0}
        print("install" if response == install_response and deliberate else "wrong-response-or-keyboard-path", flush=True)
        Gtk.main_quit()
    offer.connect("response", answer)
    offer.show_all()
    buttons[0].grab_focus()
    offer.present()
    GLib.timeout_add_seconds(10, lambda: (Gtk.main_quit(), False)[1])
    Gtk.main()


@unittest.skipUnless(platform.system() == "Linux" and os.environ.get("IBIS_GTK_CONTROL_TEST") == "1",
                     "requires IBIS_GTK_CONTROL_TEST=1 and an isolated GTK/X11 display")
class NativeControlTests(unittest.TestCase):
    layouts = ()

    def test_old_and_new_orders_install_once_and_missing_install_refuses(self):
        self.assertTrue(self.layouts, "run each layout on a fresh X display with --layout old|new|missing")
        native_preflight()
        native = importlib.import_module("update-e2e")
        for layout in self.layouts:
            with self.subTest(layout=layout), tempfile.TemporaryDirectory(dir=Path.cwd()) as temporary:
                folder = Path(temporary)
                previous, expected, library = (folder / name for name in ("previous", "expected", "synthetic.bib"))
                previous.write_bytes(b"synthetic previous")
                expected.write_bytes(b"synthetic next")
                library.write_text("@article{synthetic, title={Synthetic}}")
                driver = native.Linux(folder, argparse.Namespace(previous=previous, expected=expected), library)
                child = None
                output = folder / "fixture.stdout"
                try:
                    # Activated accessibility services can inherit fixture FDs.
                    # Wait for this PID, not EOF from those descendants' pipes.
                    with output.open("wb") as stdout, (folder / "fixture.stderr").open("wb") as stderr:
                        child = subprocess.Popen([sys.executable, __file__, "--fixture", layout],
                                                 env=native.environment(os.environ, driver.config),
                                                 stdout=stdout, stderr=stderr)
                    native.wait_for(lambda: driver.picker_ready({child.pid}) == (child.pid, "offer"),
                                    5, "synthetic offer did not acquire native focus", step=.05)
                    if layout == "missing":
                        with self.assertRaises(native.LinuxActivationFailure):
                            driver.look({child.pid}, native.BUTTON, timeout=3)
                        child.terminate()
                        self.assertEqual(fixture_output(child, output), b"")
                    else:
                        pressed, _, probe = driver.look({child.pid}, native.BUTTON, timeout=3)
                        self.assertTrue(pressed)
                        self.assertTrue(probe["attempted"])
                        self.assertEqual(fixture_output(child, output), b"install\n")
                        self.assertEqual(child.returncode, 0)
                finally:
                    if child is not None and child.poll() is None:
                        child.kill()
                        child.wait(timeout=3)
                    driver.close()


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--fixture":
        fixture(sys.argv[2])
    else:
        if len(sys.argv) == 3 and sys.argv[1] == "--layout":
            if sys.argv[2] not in ("old", "new", "missing"):
                raise SystemExit("unknown GTK control layout")
            NativeControlTests.layouts = (sys.argv[2],)
            sys.argv = sys.argv[:1]
        unittest.main()
