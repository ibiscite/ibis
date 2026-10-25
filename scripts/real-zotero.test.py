"""Focused checks for the opt-in synthetic capture block in the existing harness."""
import io
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import types
import unittest
from unittest.mock import patch


SOURCE = Path(__file__).with_name("real-zotero.sh").read_text()


class CaptureTests(unittest.TestCase):
    def setUp(self):
        self.assertTrue("<<'PY_CAPTURE'\n" in SOURCE, "the existing harness has no capture mode")
        source = SOURCE.split("<<'PY_CAPTURE'\n", 1)[1].split("\nPY_CAPTURE\n", 1)[0]
        self.capture = types.ModuleType("capture_test")
        exec(compile(source, "real-zotero capture", "exec"), self.capture.__dict__)
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def test_capture_requires_exact_version_and_fresh_private_output(self):
        for version in ("latest", "10", "10.0.6&other=value", "../10.0.6"):
            with self.assertRaises(RuntimeError):
                self.capture.prepare_output(version, self.root / "invalid")
        output = self.root / "capture"
        self.capture.prepare_output("10.0.6", output)
        if os.name != "nt":
            self.assertEqual(output.stat().st_mode & 0o777, 0o700)
        with self.assertRaises(RuntimeError):
            self.capture.prepare_output("10.0.6", output)
        self.assertFalse((self.root / "invalid").exists())

    def test_build_provenance_must_match_requested_version(self):
        app = self.root / "Zotero"
        app.mkdir()
        (app / "application.ini").write_text("[App]\nVersion=10.0.6\nBuildID=20261007000000\n")
        self.assertEqual(self.capture.application_info(app, "10.0.6"), {"version": "10.0.6", "build_id": "20261007000000"})
        with self.assertRaises(RuntimeError):
            self.capture.application_info(app, "9.0.6")

    def pages(self, versions=("1", "1"), duplicate=False, oversized=False):
        calls = []
        class Opener:
            def open(inner, request, timeout):
                calls.append(request.full_url)
                index = len(calls) - 1
                keys = ["SYNTH001", "SYNTH002"] if index == 0 or duplicate else ["SYNTH003", "SYNTH004"]
                body = json.dumps([{"key": key, "data": {}, "csljson": {}} for key in keys]).encode()
                response = io.BytesIO(body if not oversized else b"x" * (self.capture.MAX_PAGE_BYTES + 1))
                response.status = 200
                response.headers = {"Zotero-API-Version": "3", "Total-Results": "4", "Last-Modified-Version": versions[index]}
                return response
        return Opener(), calls

    def test_bounded_api_pages_preserve_raw_bytes_and_require_one_complete_version(self):
        output = self.root / "api"
        output.mkdir()
        opener, calls = self.pages()
        keys, pages = self.capture.capture_api(23129, output, opener)
        self.assertEqual(keys, {"SYNTH001", "SYNTH002", "SYNTH003", "SYNTH004"})
        self.assertEqual(len(pages), 2)
        self.assertTrue(all(url.startswith("http://127.0.0.1:23129/api/") for url in calls))
        self.assertIn("start=2", calls[1])
        self.assertEqual(json.loads((output / "api-000.json").read_text())[0]["key"], "SYNTH001")
        for options in ({"versions": ("1", "2")}, {"duplicate": True}, {"oversized": True}):
            opener, _ = self.pages(**options)
            with self.assertRaises(RuntimeError):
                self.capture.capture_api(23129, output, opener)

    def test_copied_bundle_is_integrity_checked_and_matches_captured_keys(self):
        data = self.root / "data"
        data.mkdir()
        db = sqlite3.connect(data / "zotero.sqlite")
        try:
            db.executescript("CREATE TABLE version(schema TEXT, version INT); INSERT INTO version VALUES('userdata',123); CREATE TABLE items(key TEXT); CREATE TABLE deletedItems(itemID INT);")
            db.executemany("INSERT INTO items VALUES(?)", [(f"SYNTH00{i}",) for i in range(1, 5)])
            db.commit()
        finally:
            db.close()
        output = self.root / "bundle"
        output.mkdir()
        metadata = self.capture.copy_bundle(data, output, {f"SYNTH00{i}" for i in range(1, 5)})
        self.assertEqual(metadata["schema_versions"], {"userdata": 123})
        self.assertEqual(metadata["counts"], {"items": 4, "deleted_items": 0})
        self.assertTrue((output / "zotero.sqlite").is_file())
        other = self.root / "mismatch"
        other.mkdir()
        with self.assertRaises(RuntimeError):
            self.capture.copy_bundle(data, other, {"WRONG001"})

    def test_nonregular_bundle_cannot_be_published_as_a_database(self):
        data = self.root / "not-a-db"
        data.mkdir()
        (data / "zotero.sqlite").mkdir()
        output = self.root / "output"
        output.mkdir()
        with self.assertRaises(RuntimeError):
            self.capture.copy_bundle(data, output, set())

    def test_committed_wal_rows_survive_without_mutating_the_original_bundle(self):
        data = self.root / "wal"
        data.mkdir()
        db = sqlite3.connect(data / "zotero.sqlite")
        self.addCleanup(db.close)
        db.execute("PRAGMA journal_mode=WAL")
        db.execute("PRAGMA wal_autocheckpoint=0")
        db.executescript("CREATE TABLE version(schema TEXT, version INT); INSERT INTO version VALUES('userdata',123); CREATE TABLE items(key TEXT); CREATE TABLE deletedItems(itemID INT);")
        db.executemany("INSERT INTO items VALUES(?)", [(f"SYNTH00{i}",) for i in range(1, 5)])
        db.commit()
        self.assertGreater((data / "zotero.sqlite-wal").stat().st_size, 0)
        before = {path.name: path.read_bytes() for path in data.iterdir()}
        output = self.root / "retained"
        output.mkdir()
        result = self.capture.copy_bundle(data, output, {f"SYNTH00{i}" for i in range(1, 5)})
        self.assertIn("zotero.sqlite-wal", result["database_files"])
        self.assertEqual(result["counts"]["items"], 4)
        self.assertEqual(before, {path.name: path.read_bytes() for path in data.iterdir()})
        self.assertEqual(before, {path.name: path.read_bytes() for path in output.iterdir()})

    def test_shutdown_failure_prevents_database_copy(self):
        with patch.object(self.capture, "graceful_quit", side_effect=RuntimeError("owned quit failed")):
            with patch.object(self.capture, "copy_bundle") as copied:
                with self.assertRaises(RuntimeError):
                    self.capture.finish_capture(object(), self.root, self.root, set())
                copied.assert_not_called()

    def test_quit_requires_subreaper_and_bounded_owned_process_ancestry(self):
        with patch.object(self.capture, "_SUBREAPER_ENABLED", False):
            with self.assertRaisesRegex(RuntimeError, "owned clean shutdown"):
                self.capture.graceful_quit(object())
        with patch.object(self.capture, "_process", return_value=(123, 1)) as process:
            self.assertFalse(self.capture._owned(123, 456))
            self.assertEqual(process.call_count, 64)

    def test_corrupt_database_and_excess_file_size_are_rejected(self):
        data = self.root / "corrupt"
        data.mkdir()
        (data / "zotero.sqlite").write_bytes(b"not a sqlite database")
        output = self.root / "rejected"
        output.mkdir()
        with self.assertRaises(RuntimeError):
            self.capture.copy_bundle(data, output, set())
        with patch.object(self.capture, "MAX_DATABASE_BYTES", 1):
            with self.assertRaisesRegex(RuntimeError, "oversized"):
                self.capture.copy_bundle(data, output, set())

    @unittest.skipUnless(hasattr(os, "killpg"), "POSIX process-group APIs unavailable")
    def test_adopted_helper_is_stopped_even_after_root_application_exit(self):
        app = unittest.mock.Mock()
        app.poll.return_value = 0
        parent = os.getpid()
        with patch.object(self.capture.Path, "read_text", return_value="123"):
            with patch.object(self.capture, "_process", return_value=(parent, 1)):
                with patch.object(self.capture.os, "kill") as killed:
                    with patch.object(self.capture.os, "killpg") as group:
                        with patch.object(self.capture.os, "waitpid", side_effect=[(123, 9), ChildProcessError]):
                            self.assertTrue(self.capture.abort_owned(app))
        killed.assert_called_once_with(123, self.capture.signal.SIGKILL)
        group.assert_not_called()  # A reaped root PID/group could have been reused.

    def test_uncommitted_seed_change_refuses_generator_provenance(self):
        generator = self.root / "generator"
        generator.mkdir()
        def git(*args):
            return subprocess.run(["git", "-c", "user.name=Synthetic", "-c", "user.email=synthetic@example.invalid",
                                   "-c", "commit.gpgsign=false", "-c", "core.fsmonitor=false", *args],
                                  cwd=generator, check=True, capture_output=True)
        git("init", "-q")
        (generator / "seed.rs").write_text("synthetic seed\n")
        git("add", "seed.rs")
        git("commit", "-qm", "synthetic generator")
        self.assertEqual(len(self.capture.generator_revision(generator)), 40)
        (generator / "seed.rs").write_text("changed seed\n")
        with self.assertRaisesRegex(RuntimeError, "clean committed generator"):
            self.capture.generator_revision(generator)

    def test_quit_uses_verified_owned_focus_and_untargeted_xtest_keys(self):
        commands = []
        def xdo(_deadline, *args):
            commands.append(args)
            if args[0] == "search":
                return "123"
            if args[0] == "getwindowpid":
                return "456"
            if args[0] == "getwindowfocus":
                return "123"
            return ""
        with patch.object(self.capture, "_xdo", side_effect=xdo):
            with patch.object(self.capture, "_owned", return_value=True):
                with patch.object(self.capture, "_process", return_value=(1, 789)):
                    self.capture._request_quit(456, 789, 100)
        self.assertEqual(commands[-1], ("key", "--clearmodifiers", "ctrl+q"))
        self.assertIn(("windowfocus", "--sync", "123"), commands)

    def test_wrong_owner_or_focus_never_receives_quit_keys(self):
        for owned, focus in ((False, "123"), (True, "999")):
            commands = []
            def xdo(_deadline, *args):
                commands.append(args)
                return "456" if args[0] == "getwindowpid" else focus if args[0] == "getwindowfocus" else "123"
            with patch.object(self.capture, "_xdo", side_effect=xdo):
                with patch.object(self.capture, "_owned", return_value=owned):
                    with self.assertRaises(RuntimeError):
                        self.capture._request_quit(456, 789, 100)
            self.assertFalse(any(args[0] == "key" for args in commands))


if __name__ == "__main__":
    unittest.main()
