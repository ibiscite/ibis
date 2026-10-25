"""Focused checks for the opt-in synthetic capture block in the existing harness."""
import io
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import types
import unittest


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
        with sqlite3.connect(data / "zotero.sqlite") as db:
            db.executescript("CREATE TABLE version(schema TEXT, version INT); INSERT INTO version VALUES('userdata',123); CREATE TABLE items(key TEXT); CREATE TABLE deletedItems(itemID INT);")
            db.executemany("INSERT INTO items VALUES(?)", [(f"SYNTH00{i}",) for i in range(1, 5)])
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


if __name__ == "__main__":
    unittest.main()
