"""Real GTK/X11 control verification, under an isolated Xvfb/dbus session."""
import argparse
import importlib
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import unittest


def fixture(layout):
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import GLib, Gtk
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
    def answer(_dialog, response):
        print("install" if response == install_response else "later", flush=True)
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
    def test_old_and_new_orders_install_once_and_missing_install_refuses(self):
        native = importlib.import_module("update-e2e")
        for layout in ("old", "new", "missing"):
            with self.subTest(layout=layout), tempfile.TemporaryDirectory(dir=Path.cwd()) as temporary:
                folder = Path(temporary)
                previous, expected, library = (folder / name for name in ("previous", "expected", "synthetic.bib"))
                previous.write_bytes(b"synthetic previous")
                expected.write_bytes(b"synthetic next")
                library.write_text("@article{synthetic, title={Synthetic}}")
                driver = native.Linux(folder, argparse.Namespace(previous=previous, expected=expected), library)
                child = None
                try:
                    child = subprocess.Popen([sys.executable, __file__, "--fixture", layout],
                                             env=native.environment(os.environ, driver.config),
                                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                    native.wait_for(lambda: driver.picker_ready({child.pid}) == (child.pid, "offer"),
                                    5, "synthetic offer did not acquire native focus", step=.05)
                    if layout == "missing":
                        with self.assertRaises(native.LinuxActivationFailure):
                            driver.look({child.pid}, native.BUTTON, timeout=3)
                        child.terminate()
                        stdout, _ = child.communicate(timeout=3)
                        self.assertEqual(stdout, b"")
                    else:
                        pressed, _, probe = driver.look({child.pid}, native.BUTTON, timeout=3)
                        self.assertTrue(pressed)
                        self.assertTrue(probe["attempted"])
                        stdout, _ = child.communicate(timeout=3)
                        self.assertEqual(stdout, b"install\n")
                        self.assertEqual(child.returncode, 0)
                finally:
                    if child is not None and child.poll() is None:
                        child.kill()
                        child.communicate(timeout=3)
                    driver.close()


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--fixture":
        fixture(sys.argv[2])
    else:
        unittest.main()
