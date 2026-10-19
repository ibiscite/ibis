import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("appearance", Path(__file__).with_name("check-native-appearance-macos.py"))
appearance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(appearance)


class PreferenceTransactionTests(unittest.TestCase):
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

    def test_contrast_observation_keeps_automatic_transparency_change(self):
        actual = {"contrast": True, "transparency": True}
        self.assertEqual(appearance.checked_preferences(actual, {"contrast": True}), actual)

    def test_material_observation_requires_both_accessibility_settings_off(self):
        with self.assertRaisesRegex(AssertionError, "Native accessibility preferences changed unexpectedly"):
            appearance.checked_preferences({"contrast": False, "transparency": True},
                                           {"contrast": False, "transparency": False})


if __name__ == "__main__":
    unittest.main()
