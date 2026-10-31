"""Consumer scenarios for the native evidence/replay contract (no desktop needed)."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

from native_evidence import ACTIVE, BundleError, Evidence, collect_metadata, load_bundle, observe_native, validate_bundle


def metadata():
    return {
        "build": {"revision": "a" * 40, "dirty_patch_sha256": None,
                  "artifact_sha256": "b" * 64, "app_version": "0.2.0",
                  "architecture": "x86_64", "profile": "debug"},
        "build_unavailable_reason": "not-observed",
        "harness_sha256": "c" * 64,
        "fixtures": {key: "d" * 64 for key in ("small", "initial-settings", "initial-document")},
        "environment": {"os": "linux", "os_version": "24.04", "os_build": None, "webview_version": None,
                        "locale": "en_US.UTF-8", "display_pixels": [1280, 800],
                        "display_mm": [338, 211], "display_count": 1,
                        "unavailable": ["os_build", "citation_language", "webview_version", "keyboard_layout", "input_method",
                                        "text_size", "theme", "contrast", "motion", "fonts", "scale"],
                        **{key: None for key in ("citation_language", "keyboard_layout", "input_method", "text_size",
                                                 "theme", "contrast", "motion", "fonts", "scale")}},
    }


class EvidenceScenarios(unittest.TestCase):
    def recorder(self, replay=None):
        return Evidence("native-smoke", {"identifier": "com.ibiscite.ibis", "shortcut": "ctrl+shift+y",
                                         "skip_performance": True, "seed": 0}, metadata(), replay)

    def exercise(self, recorder):
        # Completion B before A is explicit; replay must retain it. Hide/reopen
        # and synthetic typing are actions, without any raw readback in the tape.
        first = recorder.dispatch("request", "wait-1")
        second = recorder.dispatch("request", "wait-2")
        recorder.complete(second, "complete")
        recorder.complete(first, "complete")
        for code in ("key-escape", "key-shortcut", "text-query-1"):
            with recorder.step("action", code):
                pass

    def test_pass_schema_and_replay_preserve_actions_and_completion_order(self):
        original = self.recorder()
        self.exercise(original)
        bundle = original.finish(0)
        validate_bundle(bundle)
        self.assertEqual(bundle["result"], {"outcome": "pass", "failure_kind": None,
                                          "reason": None, "failed_invariant": None})
        self.assertEqual(bundle["request_completion_order"], [2, 1])
        replay = self.recorder(bundle)
        self.exercise(replay)
        replayed = replay.finish(0)
        self.assertEqual(replayed["result"], bundle["result"])
        self.assertEqual([(e["code"], e["phase"]) for e in replayed["events"]],
                         [(e["code"], e["phase"]) for e in bundle["events"]])

    def test_failure_retains_invariant_but_never_exception_text(self):
        recorder = self.recorder()
        with self.assertRaises(AssertionError):
            with recorder.step("request", "check-3"):
                raise AssertionError("PRIVATE /home/person clipboard text")
        recorder.failure(AssertionError("PRIVATE"))
        bundle = recorder.finish(1)
        self.assertEqual(bundle["result"]["failed_invariant"], "check-3")
        self.assertEqual(bundle["result"]["outcome"], "fail")
        self.assertNotIn("PRIVATE", json.dumps(bundle))
        validate_bundle(bundle)

    def test_unavailable_is_never_pass_even_without_actions(self):
        bundle = self.recorder().finish(2)
        self.assertEqual(bundle["result"]["outcome"], "unavailable")
        self.assertEqual(bundle["missing_capabilities"], ["native-session"])
        validate_bundle(bundle)

    def test_cancelled_request_is_recorded(self):
        recorder = self.recorder()
        request = recorder.dispatch("request", "wait-1")
        recorder.complete(request, "cancel")
        recorder.failure(RuntimeError("PRIVATE"))
        self.assertEqual(recorder.finish(1)["request_completion_order"], [request])

    def test_replay_refuses_completion_reordering_and_changed_build(self):
        original = self.recorder()
        self.exercise(original)
        bundle = original.finish(0)
        replay = self.recorder(bundle)
        first = replay.dispatch("request", "wait-1")
        replay.dispatch("request", "wait-2")
        with self.assertRaisesRegex(BundleError, "Replay sequence diverged"):
            replay.complete(first, "complete")
        self.assertEqual(replay.finish(1)["result"]["reason"], "replay-diverged")
        changed = copy.deepcopy(bundle)
        changed["build"]["artifact_sha256"] = "e" * 64
        with self.assertRaisesRegex(BundleError, "Replay inputs differ"):
            self.recorder(changed)

    def test_unknown_native_input_is_refused_before_driver_and_not_serialized(self):
        recorder, native = self.recorder(), Mock()
        original = native.text
        token = ACTIVE.set(recorder)
        try:
            observe_native(native)
            with self.assertRaisesRegex(BundleError, "Unrecordable native input refused"):
                native.text("PRIVATE desktop clipboard /home/person")
        finally:
            ACTIVE.reset(token)
        original.assert_not_called()
        self.assertNotIn("PRIVATE", json.dumps(recorder.finish(1)))

    def test_library_menu_reload_records_fixed_actions_and_replays_once(self):
        bundle = None
        for _ in range(2):
            recorder, native = self.recorder(bundle), Mock(spec=["click"])
            click = native.click
            token = ACTIVE.set(recorder)
            try:
                observe_native(native)
                native.click("Library")
                native.click("Reload Library")
            finally:
                ACTIVE.reset(token)
            self.assertEqual(click.call_args_list, [unittest.mock.call("Library"), unittest.mock.call("Reload Library")])
            bundle = recorder.finish(0)
            self.assertEqual(bundle["result"]["outcome"], "pass")
            self.assertEqual([(event["code"], event["phase"]) for event in bundle["events"]],
                             [("click-library", "dispatch"), ("click-library", "complete"),
                              ("click-reload", "dispatch"), ("click-reload", "complete")])
            self.assertNotIn("Reload Library", json.dumps(bundle))

    def test_obsolete_and_unknown_click_labels_are_refused_without_driver_or_desktop_text(self):
        recorder, native = self.recorder(), Mock(spec=["click"])
        click = native.click
        token = ACTIVE.set(recorder)
        try:
            observe_native(native)
            for label in ("Reload", "PRIVATE desktop clipboard /home/person"):
                with self.assertRaisesRegex(BundleError, "^Unrecordable native input refused$"):
                    native.click(label)
        finally:
            ACTIVE.reset(token)
        click.assert_not_called()
        self.assertEqual(recorder.finish(1)["events"], [])

    def test_metadata_drops_private_os_versions_probe_output_and_locale(self):
        with tempfile.TemporaryDirectory() as temporary:
            app = Path(temporary) / "missing-app"
            with patch("native_evidence._probe", return_value=b"PRIVATE /home/person"), \
                    patch("native_evidence.platform.release", return_value="PRIVATE"), \
                    patch("native_evidence.locale.setlocale", return_value="PRIVATE"), \
                    patch("native_evidence.platform.machine", return_value="PRIVATE"):
                captured = collect_metadata(app, {"small": b"synthetic"})
            self.assertNotIn("PRIVATE", json.dumps(captured))
            self.assertIsNone(captured["environment"]["locale"])
            self.assertIsNone(captured["build"]["artifact_sha256"])

    def test_replay_refuses_changed_action_before_it_can_run_and_missing_tail(self):
        recorder = self.recorder()
        self.exercise(recorder)
        bundle = recorder.finish(0)
        replay = self.recorder(bundle)
        with self.assertRaisesRegex(BundleError, "Replay sequence diverged"):
            replay.dispatch("action", "key-return")
        # A retrying caller cannot resume desktop input after a divergence.
        with self.assertRaisesRegex(BundleError, "Replay sequence diverged"):
            replay.dispatch("request", "wait-1")
        self.assertEqual(replay.finish(1)["result"]["reason"], "replay-diverged")
        self.assertEqual(self.recorder(bundle).finish(0)["result"]["reason"], "replay-diverged")

    def test_unavailable_replay_cannot_silently_change_missing_capability(self):
        bundle = self.recorder().finish(2)
        replay = self.recorder(bundle)
        replay.missing_capability = "app-build"
        self.assertEqual(replay.finish(2)["result"]["reason"], "replay-diverged")

    def test_event_bound_still_leaves_valid_failure_evidence(self):
        recorder = self.recorder()
        for _ in range(1999):
            with recorder.step("action", "key-escape"):
                pass
        with self.assertRaisesRegex(BundleError, "Evidence event limit exceeded"):
            with recorder.step("request", "check-3"):
                recorder.dispatch("action", "key-return")
        bundle = recorder.finish(1)
        self.assertEqual(len(bundle["events"]), 4000)
        self.assertEqual(bundle["result"]["failed_invariant"], "check-3")

    def test_recovered_retry_does_not_own_a_later_failure(self):
        recorder = self.recorder()
        with recorder.step("request", "check-1"):
            with self.assertRaises(AssertionError):
                with recorder.step("action", "click-reload"):
                    raise AssertionError("synthetic retry")
        with self.assertRaises(AssertionError):
            with recorder.step("request", "check-2"):
                raise AssertionError("synthetic final failure")
        self.assertEqual(recorder.finish(1)["result"]["failed_invariant"], "check-2")

    def test_malformed_older_extra_private_fields_and_wrong_completion_refused(self):
        recorder = self.recorder()
        self.exercise(recorder)
        bundle = recorder.finish(0)
        for change in (lambda b: b.update(schema_version=0),
                       lambda b: b.update(schema_version=True),
                       lambda b: b.update(private="PRIVATE"),
                       lambda b: b["build"].update(path="/home/person"),
                       lambda b: b["fixtures"].update(small="/home/person"),
                       lambda b: b.update(fixtures={}),
                       lambda b: b.update(run_id="PRIVATE" + b["run_id"]),
                       lambda b: b.update(replay_command="PRIVATE"),
                       lambda b: b["options"].update(shortcut="ctrl+PRIVATE"),
                       lambda b: b["events"][0].update(code="/private/person"),
                       lambda b: b["events"][0].update(at_ms=10 ** 1000),
                       lambda b: b.update(request_completion_order=[1, 2]),
                       lambda b: b["result"].update(outcome="unavailable"),
                       lambda b: b["environment"].update(locale="/home/person")):
            altered = copy.deepcopy(bundle)
            change(altered)
            with self.assertRaises(BundleError):
                validate_bundle(altered)

    def test_load_is_bounded_strict_json_and_messages_do_not_echo_input(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "manifest.json"
            for content in ('{"schema_version":1,"schema_version":1}', 'NaN', '{PRIVATE', ' ' * 1048577):
                path.write_text(content, encoding="utf-8")
                with self.assertRaises(BundleError) as caught:
                    load_bundle(path)
                self.assertNotIn("PRIVATE", str(caught.exception))

    def test_failed_evidence_io_is_clear_and_preserves_existing_bundle(self):
        with tempfile.TemporaryDirectory() as temporary:
            recorder = self.recorder()
            destination = recorder.write(Path(temporary), recorder.finish(2))
            self.assertEqual(load_bundle(destination)["result"]["outcome"], "unavailable")
            with self.assertRaisesRegex(BundleError, "Evidence write failed"):
                recorder.write(Path(temporary), recorder.finish(2))
            self.assertEqual(load_bundle(destination)["result"]["outcome"], "unavailable")


if __name__ == "__main__":
    unittest.main()
