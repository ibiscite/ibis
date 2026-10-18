"""Native capture lifecycle tests; no window, screenshot tool or Rust build required."""
import importlib.util
import contextlib
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("capture_gallery", Path(__file__).with_name("capture-gallery.py"))
gallery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gallery)


class Process:
    pid = 12345

    def __init__(self, exited=False):
        self.alive = not exited
        self.waits = []

    def poll(self):
        return None if self.alive else 17

    def wait(self, timeout=None):
        self.waits.append(timeout)
        if self.alive:
            raise subprocess.TimeoutExpired("synthetic-app", timeout)
        return 17

    def kill(self):
        self.alive = False


class CaptureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ibis-gallery-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.app = self.root / "synthetic-app"
        self.app.write_text("synthetic app placeholder", encoding="utf8")
        self.app.chmod(0o700)
        self.output = self.root / "captures"

    @staticmethod
    def acknowledge(process):
        def launch(_args, **options):
            env = options["env"]
            Path(env["IBIS_GALLERY_READY_FILE"]).write_text(json.dumps({"state": env["IBIS_GALLERY_STATE"]}), encoding="utf8")
            return process
        return launch

    def test_invalid_arguments_never_launch_an_app(self):
        for extra in (["--wait", "-1"], ["--wait", "nan"], ["--ready-timeout", "inf"], ["--state", "unknown"], ["--app", str(self.root / "missing")]):
            with self.subTest(extra=extra), patch.object(gallery.subprocess, "Popen") as launch:
                with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
                    gallery.main(["--app", str(self.app), "--output", str(self.output), *extra])
                self.assertEqual(error.exception.code, 2)
                launch.assert_not_called()

    def test_cli_defaults_to_all_states_and_eight_seconds(self):
        with patch.object(gallery, "capture_gallery") as capture:
            self.assertEqual(gallery.main(["--app", str(self.app), "--output", str(self.output)]), 0)
        self.assertEqual(capture.call_args.kwargs["states"], gallery.STATES)
        self.assertEqual(capture.call_args.kwargs["wait_seconds"], 8)

    def test_a_live_app_without_the_selected_render_acknowledgment_only_gets_a_failure_image(self):
        for acknowledgment in (None, "not JSON", '{"state":"empty"}'):
            process = Process()

            def launch(_args, **options):
                if acknowledgment is not None:
                    Path(options["env"]["IBIS_GALLERY_READY_FILE"]).write_text(acknowledgment, encoding="utf8")
                return process

            with self.subTest(acknowledgment=acknowledgment), patch.object(gallery.subprocess, "Popen", side_effect=launch), patch.object(gallery.os, "killpg", side_effect=lambda *_: process.kill(), create=True), patch.object(gallery, "capture_screenshot") as capture:
                with self.assertRaises(gallery.CaptureError):
                    gallery.capture_gallery(self.app, self.output, states=("loading",), platform_name="Linux", wait_seconds=0, ready_timeout=0)
                capture.assert_called_once()
                self.assertEqual(capture.call_args.args[0].name, "failure-loading.capture.tmp.png")
                self.assertFalse((self.output / "loading.png").exists())
                self.assertFalse(process.alive)

    def test_missing_ack_keeps_one_explicit_failure_image_and_the_original_error(self):
        for screenshot_fails in (False, True):
            process = Process()
            self.output.mkdir(exist_ok=True)
            (self.output / "failure-loading.png").write_bytes(gallery.PNG_HEADER + b"stale")

            def capture(destination, _platform):
                self.assertTrue(process.alive)
                destination.write_bytes(gallery.PNG_HEADER + b"diagnostic screen")
                if screenshot_fails:
                    raise gallery.CaptureError("secondary screenshot failure")

            with self.subTest(screenshot_fails=screenshot_fails), patch.object(gallery.subprocess, "Popen", return_value=process), patch.object(gallery.os, "killpg", side_effect=lambda *_: process.kill(), create=True), patch.object(gallery, "capture_screenshot", side_effect=capture) as screenshot:
                with self.assertRaisesRegex(gallery.CaptureError, "did not acknowledge rendering loading"):
                    gallery.capture_gallery(self.app, self.output, states=("loading",), platform_name="Linux", wait_seconds=0, ready_timeout=0)
                self.assertEqual(screenshot.call_count, 1)
                self.assertFalse(process.alive)
                self.assertFalse((self.output / "loading.png").exists())
                self.assertFalse((self.output / "failure-loading.capture.tmp.png").exists())
                failure = self.output / "failure-loading.png"
                self.assertEqual(failure.exists(), not screenshot_fails)
                if failure.exists():
                    self.assertEqual(failure.read_bytes(), gallery.PNG_HEADER + b"diagnostic screen")

    def test_failure_diagnostics_print_only_allowlisted_fixed_lines(self):
        log = self.root / "startup.log"
        log.write_bytes(b"private path and payload\nibis-gallery: installed\nibis-gallery: installed\nibis-gallery: unknown\nibis-gallery: rendered /private/path\nibis-gallery: navigated\n")
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            gallery.report_stages(log)
        self.assertEqual(output.getvalue(), "Gallery stage: installed\nGallery stage: navigated\n")

    def test_each_state_has_its_own_process_synthetic_source_and_png(self):
        processes = []
        environments = []

        def launch(_args, **options):
            self.assertTrue(all(not process.alive for process in processes))
            process = Process()
            processes.append(process)
            environments.append(options["env"])
            self.acknowledge(process)(_args, **options)
            self.assertIn("synthetic", Path(options["env"]["IBIS_LIBRARY"]).read_text(encoding="utf8"))
            return process

        def stop(pid, _signal):
            self.assertEqual(pid, Process.pid)
            processes[-1].alive = False

        def capture(destination, platform):
            self.assertEqual(platform, "Linux")
            self.assertTrue(processes[-1].alive)
            destination.write_bytes(gallery.PNG_HEADER + b"synthetic pixels")

        with patch.object(gallery.subprocess, "Popen", side_effect=launch), patch.object(gallery.os, "killpg", side_effect=stop, create=True), patch.object(gallery, "capture_screenshot", side_effect=capture):
            gallery.capture_gallery(self.app, self.output, states=("first-run", "rtl"), platform_name="Linux", wait_seconds=0)
        self.assertEqual([env["IBIS_GALLERY_STATE"] for env in environments], ["first-run", "rtl"])
        self.assertTrue(all(env["IBIS_GALLERY"] == "1" for env in environments))
        self.assertTrue(all(not process.alive and len(process.waits) >= 2 for process in processes))
        self.assertEqual(sorted(path.name for path in self.output.glob("*.png")), ["first-run.png", "rtl.png"])

    def test_early_exit_and_capture_failure_stop_the_gallery(self):
        for early_exit in (True, False):
            process = Process(exited=early_exit)
            self.output.mkdir(exist_ok=True)
            for state in ("loading", "empty"):
                (self.output / f"{state}.png").write_bytes(gallery.PNG_HEADER + b"stale")
            with self.subTest(early_exit=early_exit), patch.object(gallery.subprocess, "Popen", side_effect=self.acknowledge(process)) as launch, patch.object(gallery.os, "killpg", side_effect=lambda *_: process.kill(), create=True), patch.object(gallery, "capture_screenshot", side_effect=gallery.CaptureError("synthetic capture failure")) as capture:
                with self.assertRaises(gallery.CaptureError):
                    gallery.capture_gallery(self.app, self.output, states=("loading", "empty"), platform_name="Linux", wait_seconds=0)
                self.assertFalse(process.alive)
                self.assertEqual(launch.call_count, 1)
                self.assertEqual(capture.call_count, 0 if early_exit else 1)
                self.assertFalse(list(self.output.glob("*.png")))

    def test_launch_failure_never_leaves_an_image(self):
        with patch.object(gallery.subprocess, "Popen", side_effect=OSError("synthetic launch failure")):
            with self.assertRaises(OSError):
                gallery.capture_gallery(self.app, self.output, states=("loading",), platform_name="Linux", wait_seconds=0)
        self.assertFalse(list(self.output.glob("*.png")))

    def test_a_missing_capture_cannot_reuse_an_old_staging_image(self):
        process = Process()
        self.output.mkdir()
        staging = self.output / "loading.capture.tmp.png"
        staging.write_bytes(gallery.PNG_HEADER + b"old screenshot")
        with patch.object(gallery.subprocess, "Popen", side_effect=self.acknowledge(process)), patch.object(gallery.os, "killpg", side_effect=lambda *_: process.kill(), create=True), patch.object(gallery, "capture_screenshot"):
            with self.assertRaisesRegex(gallery.CaptureError, "was not written"):
                gallery.capture_gallery(self.app, self.output, states=("loading",), platform_name="Linux", wait_seconds=0)
        self.assertFalse(process.alive)
        self.assertFalse(staging.exists())
        self.assertFalse((self.output / "loading.png").exists())

    def test_interrupt_cleans_up_and_invalid_capture_is_not_published(self):
        for result in (KeyboardInterrupt(), None):
            process = Process()

            def capture(destination, _platform):
                destination.write_bytes(b"not a PNG")
                if result is not None:
                    raise result

            with self.subTest(result=result), patch.object(gallery.subprocess, "Popen", side_effect=self.acknowledge(process)), patch.object(gallery.os, "killpg", side_effect=lambda *_: process.kill(), create=True), patch.object(gallery, "capture_screenshot", side_effect=capture):
                with self.assertRaises(KeyboardInterrupt if result else gallery.CaptureError):
                    gallery.capture_gallery(self.app, self.output, states=("loading",), platform_name="Linux", wait_seconds=0)
                self.assertFalse(process.alive)
                self.assertFalse(list(self.output.glob("*.png")))
                self.assertFalse(list(self.output.glob(".*.png")))

    def test_cleanup_escalates_and_windows_cleanup_reaps_the_app(self):
        process = Process()
        signals = []

        def stop(_pid, sent):
            signals.append(sent)
            if sent == 9:
                process.kill()

        with patch.object(gallery.os, "killpg", side_effect=stop, create=True), patch.object(gallery.signal, "SIGKILL", 9, create=True):
            gallery.stop_app(process, "Linux")
        self.assertEqual(signals, [signal.SIGTERM, 9])
        self.assertFalse(process.alive)
        windows = Process()
        with patch.object(gallery.subprocess, "run", return_value=subprocess.CompletedProcess([], 1)) as taskkill:
            gallery.stop_app(windows, "Windows")
        self.assertFalse(windows.alive)
        self.assertIn("/T", taskkill.call_args.args[0])

    def test_windows_capture_treats_the_destination_as_data(self):
        destination = self.output / "quote ' and $(unexpected).png"
        with patch.object(gallery.subprocess, "run") as run:
            gallery.capture_screenshot(destination, "Windows")
        arguments = run.call_args.args[0]
        self.assertNotIn(str(destination), " ".join(arguments))
        self.assertEqual(run.call_args.kwargs["env"]["IBIS_GALLERY_SCREENSHOT"], str(destination))
        self.assertTrue(run.call_args.kwargs["check"])
        self.assertEqual(len(gallery.STATES), 14)

    def test_mac_capture_uses_nonhidden_staging_and_then_publishes_the_state_name(self):
        process = Process()

        def capture(command, **options):
            self.assertEqual(command[0], "screencapture")
            self.assertTrue(options["check"])
            destination = Path(command[-1])
            self.assertFalse(destination.name.startswith("."))
            self.assertEqual(destination.name, "first-run.capture.tmp.png")
            destination.write_bytes(gallery.PNG_HEADER + b"synthetic Mac capture")
            return subprocess.CompletedProcess(command, 0)

        with patch.object(gallery.subprocess, "Popen", side_effect=self.acknowledge(process)), patch.object(gallery.subprocess, "run", side_effect=capture), patch.object(gallery.os, "killpg", side_effect=lambda *_: process.kill(), create=True):
            gallery.capture_gallery(self.app, self.output, states=("first-run",), platform_name="Darwin", wait_seconds=0)
        self.assertEqual(sorted(path.name for path in self.output.iterdir()), ["first-run.png", "startup-first-run.log"])

    def test_cleanup_reaps_a_real_synthetic_process(self):
        process = subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(60)"],
            start_new_session=os.name != "nt",
        )
        try:
            gallery.stop_app(process, gallery.platform.system())
            self.assertIsNotNone(process.poll())
        finally:
            if process.poll() is None:
                process.kill()
            process.wait(timeout=5)


if __name__ == "__main__":
    unittest.main()
