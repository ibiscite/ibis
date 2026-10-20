#!/usr/bin/env python3
"""End-to-end update test for the Linux AppImage (#319).

Starts the previous Ibis Nightly with automatic update checks on, waits for it
to offer the newly published nightly, presses "Install and Restart" through
AT-SPI and checks that the AppImage file was replaced by the new package and
that the new version started in its place.

    xvfb-run -a dbus-run-session -- /usr/bin/python3 scripts/update-e2e.py \\
        --previous previous/Ibis-Linux.AppImage --expected dist/Ibis-Linux.AppImage --output out
    python3 scripts/update-e2e.py --self-test
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

IDENTIFIER = "io.github.ibiscite.ibis.nightly"
BUTTON = "Install and Restart"
# The app's first check comes 30 seconds after start; the download is ~100 MB.
OFFER_TIMEOUT = 180
REPLACE_TIMEOUT = 300
RESTART_TIMEOUT = 60


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def seed_settings(config):
    """Update checks already allowed, so the first-launch question never shows."""
    settings = config / IDENTIFIER / "settings.json"
    settings.parent.mkdir(parents=True, exist_ok=True)
    settings.write_text(json.dumps({"schema_version": 2, "update_checks": True}), encoding="utf-8")
    return settings


def environment(inherited, config, library):
    env = dict(inherited, XDG_CONFIG_HOME=str(config), IBIS_LIBRARY=str(library),
               NO_AT_BRIDGE="0", GTK_MODULES="gail:atk-bridge")
    for name in ("APPIMAGE", "APPDIR", "IBIS_GALLERY", "IBIS_GALLERY_STATE"):
        env.pop(name, None)
    return env


def instances(appimage, proc=Path("/proc")):
    """Processes started from this AppImage file: the AppImage runtime sets
    APPIMAGE to its path, and the updater replaces the file it names."""
    wanted = f"APPIMAGE={appimage}".encode()
    found = set()
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            if wanted in (entry / "environ").read_bytes().split(b"\0"):
                found.add(int(entry.name))
        except OSError:
            continue
    return found


def wait_for(predicate, timeout, message, step=1.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(step)
    raise AssertionError(message)


def find_button(atspi, name):
    desktop = atspi.Registry.getDesktop(0)
    stack = [app for app in desktop if app]
    seen = 0
    while stack and seen < 20000:
        item = stack.pop()
        seen += 1
        try:
            if item.getRoleName() == "push button" and item.name == name:
                return item
            stack.extend(child for child in item if child)
        except Exception:  # Accessibility objects vanish while windows change.
            continue
    return None


def run(args):
    import pyatspi

    report = {"status": "failed", "steps": []}
    started = []
    log = None
    try:
        with tempfile.TemporaryDirectory(prefix="ibis update test ") as temporary:
            folder = Path(temporary)
            appimage = folder / "Ibis Nightly.AppImage"
            shutil.copyfile(args.previous, appimage)
            appimage.chmod(0o755)
            library = folder / "synthetic.bib"
            library.write_text("@article{update1, title = {Synthetic update test}, year = {2026}}\n", encoding="utf-8")
            config = folder / "config"
            seed_settings(config)
            before, expected = digest(appimage), digest(args.expected)
            if before == expected:
                raise AssertionError("the previous and the new AppImage are identical")

            log = (args.output / "update-e2e-app.log").open("w", encoding="utf-8")
            app = subprocess.Popen([str(appimage)], env=environment(os.environ, config, library),
                                   stdout=log, stderr=log, cwd=folder)
            started.append(app)
            report["steps"].append("previous nightly started")

            button = wait_for(lambda: find_button(pyatspi, BUTTON), OFFER_TIMEOUT,
                              "the previous nightly never offered the update")
            report["steps"].append("update offered")
            old = instances(appimage)
            button.queryAction().doAction(0)
            report["steps"].append("Install and Restart pressed")

            wait_for(lambda: appimage.exists() and digest(appimage) == expected, REPLACE_TIMEOUT,
                     "the AppImage was not replaced by the new package", step=2)
            report["steps"].append("AppImage replaced by the new package")

            def restarted():
                fresh = instances(appimage) - old
                return fresh if fresh else None
            fresh = wait_for(restarted, RESTART_TIMEOUT, "the new version did not start after the update")
            time.sleep(5)
            if not instances(appimage) & fresh:
                raise AssertionError("the new version exited right after the update")
            report["steps"].append("new version started and stayed up")
            report["status"] = "passed"
            for pid in instances(appimage):
                try:
                    os.kill(pid, 15)
                except OSError:
                    pass
    except AssertionError as error:
        report["error"] = str(error)
    finally:
        for app in started:
            if app.poll() is None:
                app.kill()
        if log:
            log.close()
        (args.output / "update-e2e.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "passed" else 1


class Tests(unittest.TestCase):
    def test_settings_allow_update_checks_in_the_nightly_folder(self):
        with tempfile.TemporaryDirectory() as temporary:
            settings = seed_settings(Path(temporary))
            self.assertEqual(settings, Path(temporary) / "io.github.ibiscite.ibis.nightly/settings.json")
            self.assertEqual(json.loads(settings.read_text(encoding="utf-8")), {"schema_version": 2, "update_checks": True})

    def test_environment_drops_an_outer_appimage(self):
        env = environment({"APPIMAGE": "/outer", "PATH": "/bin"}, Path("/c"), Path("/l.bib"))
        self.assertNotIn("APPIMAGE", env)
        self.assertEqual((env["XDG_CONFIG_HOME"], env["PATH"]), ("/c", "/bin"))

    def test_instances_match_the_exact_appimage_path(self):
        with tempfile.TemporaryDirectory() as temporary:
            proc = Path(temporary)
            for pid, value in (("10", "/t/Ibis Nightly.AppImage"), ("11", "/t/Ibis Nightly.AppImage.old"), ("12", None)):
                (proc / pid).mkdir()
                environ = b"PATH=/bin\0" + (f"APPIMAGE={value}".encode() if value else b"")
                (proc / pid / "environ").write_bytes(environ)
            (proc / "self").mkdir()
            self.assertEqual(instances(Path("/t/Ibis Nightly.AppImage"), proc), {10})


def main():
    if sys.argv[1:] == ["--self-test"]:
        unittest.main(argv=[sys.argv[0]])
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--previous", type=Path, required=True)
    parser.add_argument("--expected", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    sys.exit(run(args))


if __name__ == "__main__":
    main()
