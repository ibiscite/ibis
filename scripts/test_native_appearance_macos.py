import importlib.util
from pathlib import Path
from tempfile import TemporaryDirectory
import subprocess
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("appearance", Path(__file__).with_name("check-native-appearance-macos.py"))
appearance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(appearance)


def picker_geometry():
    return {"window": 12, "position": [20, 30], "size": [720, 480], "screen": [1600, 1000]}


class PreferenceTransactionTests(unittest.TestCase):
    def test_picker_child_lifetime_is_checked_before_and_after_each_helper(self):
        from types import SimpleNamespace
        from unittest.mock import Mock
        request = {"owner": {"pid": 77, "executable": "/owned/ibis"}}
        ui = SimpleNamespace(call=Mock(return_value={"window": 12}))
        dead = SimpleNamespace(pid=77, poll=Mock(return_value=0))
        with self.assertRaisesRegex(RuntimeError, "process exited"):
            appearance.owned_picker_call(dead, ui, "picker-frame", request)
        ui.call.assert_not_called()
        exiting = SimpleNamespace(pid=77, poll=Mock(side_effect=[None, 0]))
        with self.assertRaisesRegex(RuntimeError, "process exited"):
            appearance.owned_picker_call(exiting, ui, "picker-frame", request)
        self.assertEqual(ui.call.call_count, 1)
        live = SimpleNamespace(pid=77, poll=Mock(return_value=None))
        self.assertEqual(appearance.owned_picker_call(live, ui, "picker-frame", request), {"window": 12})
        self.assertEqual(live.poll.call_count, 2)

    def test_foreign_picker_pid_never_authorizes_a_helper_call(self):
        from types import SimpleNamespace
        from unittest.mock import Mock
        ui = SimpleNamespace(call=Mock())
        child = SimpleNamespace(pid=77, poll=Mock(return_value=None))
        for action, request in (("picker-owner", {"pid": 88}), ("picker-size", {"owner": {"pid": 88}})):
            with self.assertRaisesRegex(RuntimeError, "identity"):
                appearance.owned_picker_call(child, ui, action, request)
        ui.call.assert_not_called()

    def test_registration_null_is_not_failure_and_timeout_diagnostic_is_fixed(self):
        ui = appearance.NativeUI()
        with patch.object(appearance.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "null", "")):
            self.assertIsNone(ui.call("picker-owner"))
        self.assertFalse(hasattr(ui, "failure_facts"))
        with patch.object(appearance.subprocess, "run", side_effect=subprocess.TimeoutExpired("private command", 25)):
            with self.assertRaisesRegex(appearance.HelperTimeout, "picker-frame/invocation/helper-timeout"):
                ui.call("picker-frame")
        self.assertEqual(ui.failure_facts, {"action": "picker-frame", "stage": "invocation", "reason": "helper-timeout"})

    def test_helper_failure_is_fixed_and_first_failure_survives_restoration(self):
        import json
        ui = appearance.NativeUI()
        envelope = {"nativeAppearanceFailure": {"action": "picker-owner", "stage": "registration", "reason": "native-call-failed"}}
        with patch.object(appearance.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, json.dumps(envelope), "private payload")):
            with self.assertRaisesRegex(RuntimeError, "picker-owner/registration/native-call-failed"):
                ui.call("picker-owner", "private request")
        self.assertEqual(ui.failure_facts, envelope["nativeAppearanceFailure"])
        with patch.object(appearance.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, "", "private payload")):
            with self.assertRaises(RuntimeError):
                ui.call("set")
        self.assertEqual(ui.failure_facts, envelope["nativeAppearanceFailure"])

    def test_untrusted_helper_diagnostic_payload_is_never_echoed(self):
        import json
        for envelope in ({"action": "private payload", "stage": "registration", "reason": "native-call-failed"},
                         {"action": "picker-owner", "stage": "private payload", "reason": "native-call-failed"},
                         {"action": "picker-owner", "stage": "registration", "reason": "private payload"},
                         {"action": "picker-size", "stage": "registration", "reason": "native-call-failed"},
                         {"action": "picker-owner", "stage": "registration", "reason": "native-call-failed", "extra": "private payload"}):
            ui = appearance.NativeUI()
            result = subprocess.CompletedProcess([], 0, json.dumps({"nativeAppearanceFailure": envelope}), "private payload")
            with patch.object(appearance.subprocess, "run", return_value=result):
                with self.assertRaisesRegex(RuntimeError, "picker-owner/invocation/invalid-response") as error:
                    ui.call("picker-owner", "private request")
            self.assertNotIn("private payload", str(error.exception))
            self.assertEqual(ui.failure_facts, {"action": "picker-owner", "stage": "invocation", "reason": "invalid-response"})

    def test_replaced_window_between_phases_cannot_produce_more_captures(self):
        state = picker_geometry()
        captures, writes = [], []
        read = appearance.pin_picker_window(lambda: dict(state))
        def resize(window, size):
            writes.append(window)
            state["size"] = list(size)
        with TemporaryDirectory() as folder, patch.object(appearance, "wait_for", side_effect=lambda read, _: read()):
            appearance.capture_sizes("material", Path(folder), read, resize, lambda path: captures.append(path.name))
            state["window"] = 13  # Same PID, dimensions and title; different native window.
            with self.assertRaisesRegex(RuntimeError, "identity changed"):
                appearance.capture_sizes("reduce-transparency", Path(folder), read, resize, lambda path: captures.append(path.name))
        self.assertEqual(captures, ["material.png", "material-resized.png"])
        self.assertEqual(writes, [12, 12])

    def test_each_phase_captures_normal_and_verified_resized_window_then_restores(self):
        events = []
        state = picker_geometry()
        def frame(): return dict(state)
        def resize(window, size):
            self.assertEqual(window, 12)
            events.append(("resize", size))
            state["size"] = list(size)
        with TemporaryDirectory() as folder, patch.object(appearance, "wait_for", side_effect=lambda read, _: read() or self.fail("no verified resize")):
            for name in appearance.APPEARANCE_PHASES:
                result = appearance.capture_sizes(name, Path(folder), frame, resize,
                    lambda path: events.append(("capture", path.name)))
                self.assertEqual(result, {"normal": [720, 480], "resized": [840, 560], "restored": True})
        captures = [value for action, value in events if action == "capture"]
        self.assertEqual(len(captures), 10)
        self.assertEqual(state["size"], [720, 480])
        self.assertIn("increase-contrast-resized.png", captures)

    def test_noop_resize_cannot_produce_resized_capture_and_still_restores(self):
        captures, resized = [], []
        frame = picker_geometry
        def wait(read, _):
            if not read(): raise AssertionError("unverified resize")
        with TemporaryDirectory() as folder, patch.object(appearance, "wait_for", side_effect=wait):
            with self.assertRaisesRegex(AssertionError, "unverified resize"):
                appearance.capture_sizes("material", Path(folder), frame,
                    lambda window, size: resized.append(size), lambda path: captures.append(path.name))
        self.assertEqual(captures, ["material.png"])
        self.assertEqual(resized[-1], [720, 480])

    def test_failed_resized_capture_restores_geometry_and_both_preferences(self):
        state = picker_geometry()
        prefs = {"contrast": True, "transparency": True}
        def resize(window, size): state["size"] = list(size)
        def capture(path):
            if "resized" in path.name: raise RuntimeError("synthetic capture failure")
        with TemporaryDirectory() as folder, patch.object(appearance, "wait_for", side_effect=lambda read, _: read()):
            with self.assertRaisesRegex(RuntimeError, "synthetic capture failure"):
                with appearance.saved_preferences(lambda: dict(prefs), lambda key, value: prefs.update({key: value})):
                    prefs.update(contrast=False, transparency=False)
                    appearance.capture_sizes("material", Path(folder), lambda: dict(state), resize, capture)
        self.assertEqual(state["size"], [720, 480])
        self.assertEqual(prefs, {"contrast": True, "transparency": True})

    def test_invalid_or_offscreen_geometry_never_authorizes_resize(self):
        frame = picker_geometry()
        for key, value in (("window", True), ("size", [float("nan"), 480]), ("position", [-1, 30]), ("screen", [700, 500])):
            with self.assertRaises(RuntimeError):
                appearance.picker_frame({**frame, key: value})

    def test_missing_fresh_image_cannot_reuse_prior_png_and_nonpng_is_rejected(self):
        with TemporaryDirectory() as folder, patch.object(appearance.time, "sleep"):
            path = Path(folder) / "material.png"
            path.write_bytes(b"\x89PNG\r\n\x1a\nold evidence")
            with patch.object(appearance.subprocess, "run"):
                with self.assertRaisesRegex(RuntimeError, "not written"):
                    appearance.capture_screen(path)
            self.assertFalse(path.exists())
            with patch.object(appearance.subprocess, "run", side_effect=lambda *args, **kwargs: path.write_bytes(b"wrong format")):
                with self.assertRaisesRegex(RuntimeError, "not a PNG"):
                    appearance.capture_screen(path)

    def test_false_quit_request_can_still_have_verified_termination(self):
        ui = appearance.NativeUI()
        with patch.object(ui, "call", side_effect=[False, {"requested": False}, True]) as helper:
            ui.close(True)
        self.assertEqual(ui.cleanup_facts, {"requested": False, "terminated": True, "forced": False})
        self.assertFalse(any(call.args[0] == "force-close" for call in helper.call_args_list))

    def test_cleanup_never_forces_without_verified_original_preferences(self):
        ui = appearance.NativeUI()
        with patch.object(ui, "call", side_effect=[False, {"requested": False}, False]) as helper, patch.object(appearance, "wait_for", side_effect=AssertionError):
            with self.assertRaisesRegex(RuntimeError, "requires verified preference restoration"):
                ui.close(False)
        self.assertFalse(any(call.args[0] == "force-close" for call in helper.call_args_list))

    def test_exit_after_quit_deadline_needs_no_force_or_preference_override(self):
        ui = appearance.NativeUI()
        with patch.object(ui, "call", side_effect=[False, {"requested": False}, True]) as helper, patch.object(appearance, "wait_for", side_effect=AssertionError):
            ui.close(False)
        self.assertTrue(ui.cleanup_facts["terminated"])
        self.assertFalse(any(call.args[0] == "force-close" for call in helper.call_args_list))

    def test_native_controls_require_fresh_unique_ownership_before_invoking(self):
        subprocess.run(["node", str(Path(__file__).with_name("appearance-macos.test.cjs"))],
                       check=True, capture_output=True, timeout=20)

    def test_records_actual_final_state_but_still_fails_after_an_attempt_error(self):
        observations, calls = [], []
        def write(key, value):
            calls.append(key)
            if len(calls) == 1:
                raise appearance.HelperTimeout("Native appearance helper timed out")
        with self.assertRaisesRegex(RuntimeError, "Original accessibility preferences could not be restored"):
            with appearance.saved_preferences(lambda: {"contrast": False, "transparency": True}, write,
                                              observe=observations.append):
                pass
        self.assertEqual(observations[0]["final_preferences"], {"contrast": False, "transparency": True})
        self.assertTrue(observations[0]["matches_original"])
        self.assertEqual(observations[0]["attempts"][0]["stage"], "helper-timeout")
        self.assertEqual(len(observations[0]["attempts"]), 3)

    def test_restores_original_preferences_after_failed_observation(self):
        state = {"contrast": True, "transparency": True}
        def write(key, value):
            state[key] = value
        with self.assertRaisesRegex(AssertionError, "synthetic observation"):
            with appearance.saved_preferences(lambda: dict(state), write):
                write("contrast", False)
                write("transparency", False)
                raise AssertionError("synthetic observation")
        self.assertEqual(state, {"contrast": True, "transparency": True})

    def test_attempts_every_restore_after_one_failure(self):
        calls = []
        def write(key, value):
            calls.append((key, value))
            if len(calls) == 1:
                raise RuntimeError("synthetic restore failure")
        with self.assertRaisesRegex(RuntimeError, "Original accessibility preferences could not be restored"):
            with appearance.saved_preferences(lambda: {"contrast": False, "transparency": True}, write):
                pass
        self.assertEqual(calls, [("contrast", False), ("transparency", True), ("contrast", False)])

    def test_requires_boolean_state_before_any_change(self):
        calls = []
        with self.assertRaisesRegex(RuntimeError, "Native accessibility preference state unavailable"):
            with appearance.saved_preferences(lambda: {"contrast": None, "transparency": True}, lambda *args: calls.append(args)):
                self.fail("Invalid initial state entered transaction")
        self.assertEqual(calls, [])

    def test_reports_successful_restoration_even_when_observation_fails(self):
        restored = []
        with self.assertRaises(AssertionError):
            with appearance.saved_preferences(lambda: {"contrast": False, "transparency": True},
                                              lambda *args: None, lambda: restored.append(True)):
                raise AssertionError("synthetic observation")
        self.assertEqual(restored, [True])

    def test_mode_parser_accepts_only_fixed_native_facts(self):
        self.assertEqual(appearance.mode_facts("ibis: native appearance mode=vibrancy reason=applied\n"), [("vibrancy", "applied")])
        self.assertEqual(appearance.mode_facts("untrusted arbitrary error\nibis: native appearance mode=other reason=payload\n"), [])

    def test_material_selection_accepts_only_success_or_explicit_glass_fallback(self):
        for mode, reason in [("liquid-glass", "applied"), ("vibrancy", "unsupported-glass")]:
            log = f"ibis: native appearance mode={mode} reason={reason}\n"
            self.assertEqual(appearance.material_fact(log), (mode, reason))
        for mode, reason in [("vibrancy", "applied"), ("opaque", "native-failure"), ("liquid-glass", "unsupported-glass"), ("mica", "applied")]:
            self.assertIsNone(appearance.material_fact(f"ibis: native appearance mode={mode} reason={reason}\n"))
        with self.assertRaisesRegex(RuntimeError, "exceeded bound"):
            appearance.material_fact("x" * (1024 * 1024 + 1))

    def test_contrast_observation_keeps_automatic_transparency_change(self):
        actual = {"contrast": True, "transparency": True}
        self.assertEqual(appearance.checked_preferences(actual, {"contrast": True}), actual)

    def test_material_observation_requires_both_accessibility_settings_off(self):
        with self.assertRaisesRegex(AssertionError, "Native accessibility preferences changed unexpectedly"):
            appearance.checked_preferences({"contrast": False, "transparency": True},
                                           {"contrast": False, "transparency": False})


if __name__ == "__main__":
    unittest.main()
