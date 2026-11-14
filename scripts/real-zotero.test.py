"""Focused checks for the opt-in synthetic capture block in the existing harness."""
import hashlib
import io
import json
import os
from contextlib import ExitStack, closing, contextmanager
from pathlib import Path
import socket
import sqlite3
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import Mock, patch


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

    def installed_launcher(self, version="7.0", build="20261007000000"):
        directory = self.root / "private-installation"
        directory.mkdir(exist_ok=True)
        launcher = directory / "zotero"
        launcher.write_text("#!/bin/sh\nexit 99\n")  # Never executed by these mocked captures.
        launcher.chmod(0o700)
        (directory / "application.ini").write_text(f"[App]\nName=Zotero\nVersion={version}\nBuildID={build}\n")
        return launcher

    @contextmanager
    def capture_session(self, occupied=False, shutdown_error=None):
        """Fake only native/process boundaries; exercise profile, pages and bundle validation."""
        app = Mock(pid=4242)
        app.poll.return_value = None
        state = types.SimpleNamespace(seed=[], profiles=[])
        pages, _ = self.pages()

        def open_request(request, timeout):
            url = request if isinstance(request, str) else request.full_url
            self.assertTrue(url.startswith("http://127.0.0.1:23129/"))
            if url.endswith("/connector/ping"):
                response = io.BytesIO(b"")
                response.status = 200
                return response
            return pages.open(request, timeout)

        def launch(arguments, **kwargs):
            state.launch = arguments
            profile = Path(arguments[arguments.index("-profile") + 1])
            state.profiles.append(profile)
            prefs = {}
            for line in (profile / "user.js").read_text().splitlines():
                key, value = json.loads("[" + line[len("user_pref("):-2] + "]")
                prefs[key] = value
            state.prefs = prefs
            state.data = Path(prefs["extensions.zotero.dataDir"])
            return app

        def seed(arguments, **kwargs):
            # Any download/fallback subprocess fails, including a future non-curl downloader.
            self.assertEqual(arguments, ["cargo", "test", "-p", "ibis-zotero-local", "--locked", "--offline",
                                         "--test", "real_zotero", "--", "--ignored"])
            state.seed.append(kwargs["env"])
            with closing(sqlite3.connect(state.data / "zotero.sqlite")) as db:
                db.executescript("CREATE TABLE version(schema TEXT, version INT); INSERT INTO version VALUES('userdata',123); CREATE TABLE items(key TEXT); CREATE TABLE deletedItems(itemID INT);")
                db.executemany("INSERT INTO items VALUES(?)", [(f"SYNTH00{i}",) for i in range(1, 5)])
                db.commit()

        with ExitStack() as stack:
            def replace(target, name, **kwargs):
                return stack.enter_context(patch.object(target, name, **kwargs))
            replace(self.capture.platform, "system", return_value="Linux")
            replace(self.capture.platform, "machine", return_value="x86_64")
            replace(self.capture, "generator_revision", return_value="a" * 40)
            replace(self.capture, "enable_subreaper")
            replace(self.capture, "_process", return_value=(1, 789))
            replace(self.capture.time, "sleep")
            replace(self.capture.subprocess, "Popen", side_effect=launch)
            commands = replace(self.capture.subprocess, "run", side_effect=seed)
            archive = replace(self.capture.tarfile, "open", side_effect=AssertionError("offline capture extracted an archive"))
            replace(self.capture.urllib.request, "build_opener", return_value=types.SimpleNamespace(open=open_request))
            probe = replace(socket, "socket").return_value.__enter__.return_value
            probe.connect_ex.return_value = 0 if occupied else 111
            state.quit = replace(self.capture, "graceful_quit", side_effect=shutdown_error)
            state.abort = replace(self.capture, "abort_owned", return_value=True)
            yield state
            # These assertions also catch a fallback swallowed by run_capture's error cleanup.
            self.assertEqual([call.args[0][0] for call in commands.call_args_list], ["cargo"] * len(state.seed))
            archive.assert_not_called()

    # real-zotero.sh is a bash tool for macOS and Linux; Windows paths do not reach it intact.
    @unittest.skipIf(sys.platform == "win32", "real-zotero.sh runs on macOS and Linux")
    def test_capture_cli_forwards_existing_and_local_selection_invocations(self):
        tools = self.root / "tools"
        tools.mkdir()
        stub = "#!" + sys.executable + "\n" + (
            "import json, sys\n"
            "from pathlib import Path\n"
            "sys.stdin.read()\n"
            "print(json.dumps({'tool': Path(sys.argv[0]).name, 'args': sys.argv[1:]}))\n"
        )
        for tool in ("xvfb-run", "curl", "tar"):
            path = tools / tool
            path.write_text(stub)
            path.chmod(0o700)
        environment = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ["PATH"])
        script = Path(__file__).with_name("real-zotero.sh").resolve()
        launcher = self.installed_launcher()
        for selection in ([], ["--executable", str(launcher)]):
            result = subprocess.run(["bash", str(script), "--capture", "7.0", str(self.root / "fresh"), *selection],
                                    env=environment, capture_output=True, text=True, check=True)
            invoked = json.loads(result.stdout)
            self.assertEqual(invoked["tool"], "xvfb-run")
            self.assertEqual(invoked["args"], ["-a", "python3", "-", str(script.parent.parent),
                                                "7.0", str(self.root / "fresh"), *selection])
        for arguments in (["--executable", str(launcher)], ["--capture", "7.0"],
                          ["--capture", "7.0", "fresh", "--executable"],
                          ["--capture", "7.0", "fresh", "--unknown", str(launcher)]):
            result = subprocess.run(["bash", str(script), *arguments], env=environment,
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("usage:", result.stderr)
            self.assertEqual(result.stdout, "")  # Neither Xvfb nor the download tools ran.

    def test_local_launcher_runs_isolated_capture_without_download_or_extraction(self):
        for version in ("7.0", "10.0.6"):
            with self.subTest(version=version):
                launcher = self.installed_launcher(version)
                output = self.root / ("capture-" + version)
                with self.capture_session() as state:
                    self.capture.run_capture(self.root, version, output, launcher)
                manifest = json.loads((output / "manifest.json").read_text())
                keys = [item["key"] for name in ("api-000.json", "api-001.json")
                        for item in json.loads((output / name).read_text())]
                self.assertEqual(keys, [f"SYNTH00{i}" for i in range(1, 5)])
                with closing(sqlite3.connect(output / "zotero.sqlite")) as db:
                    self.assertEqual(db.execute("SELECT key FROM items ORDER BY key").fetchall(),
                                     [(key,) for key in keys])
                self.assertEqual(manifest["quick_check"], "ok")
                self.assertEqual(manifest["schema_versions"], {"userdata": 123})
                self.assertEqual(manifest["coverage"], "partial: personal library, two articles, one book and one report; not a supported-version declaration")
                self.assertEqual(manifest["application"], {
                    "version": version, "build_id": "20261007000000",
                    "launcher": {"sha256": hashlib.sha256(launcher.read_bytes()).hexdigest(),
                                 "bytes": launcher.stat().st_size}})
                self.assertNotIn("archive", manifest)
                self.assertNotIn(str(self.root), json.dumps(manifest))
                self.assertEqual(manifest["counts"], {"items": 4, "deleted_items": 0})
                self.assertEqual(set(manifest["files"]), {"api-000.json", "api-001.json", "zotero.sqlite"})
                self.assertEqual(state.launch[:2], [str(launcher.resolve()), "-no-remote"])
                self.assertTrue(state.prefs["extensions.zotero.useDataDir"])
                self.assertTrue(state.prefs["extensions.zotero.httpServer.localAPI.enabled"])
                self.assertEqual(state.prefs["extensions.zotero.httpServer.port"], 23129)
                self.assertFalse(state.prefs["app.update.enabled"])
                self.assertNotEqual(state.data.parent, launcher.parent)
                self.assertFalse(state.data.exists())
                self.assertTrue(all(not profile.exists() for profile in state.profiles))
                self.assertEqual(len(state.seed), 1)
                self.assertEqual(state.seed[0]["IBIS_ZOTERO_CAPTURE_PID"], "4242")
                self.assertEqual(state.seed[0]["IBIS_ZOTERO_CAPTURE_START"], "789")
                state.quit.assert_called_once()
                state.abort.assert_not_called()

    def test_seed_disables_rustc_wrapper_and_reuses_worktree_or_supplied_target(self):
        launcher = self.installed_launcher()
        for target in (None, str(self.root / "chosen-target")):
            with self.subTest(target=target):
                with patch.dict(os.environ, {"RUSTC_WRAPPER": "blocked-wrapper"}):
                    os.environ.pop("CARGO_TARGET_DIR", None)
                    if target is not None:
                        os.environ["CARGO_TARGET_DIR"] = target
                    with self.capture_session() as state:
                        self.capture.run_capture(self.root, "7.0", self.root / ("default" if target is None else "supplied"), launcher)
                self.assertEqual(state.seed[0]["RUSTC_WRAPPER"], "")
                self.assertEqual(state.seed[0]["CARGO_TARGET_DIR"], target or str(self.root / "target"))

    def test_invalid_local_selection_refuses_before_launch_or_seed_without_fallback(self):
        launcher = self.installed_launcher()
        invalid = [self.root / "missing", launcher.parent, Path("relative/zotero"), ""]
        nonexecutable = self.root / "nonexecutable"
        nonexecutable.write_text("synthetic")
        invalid.append(nonexecutable)
        for index, selected in enumerate(invalid):
            with self.subTest(selected=selected):
                output = self.root / f"invalid-{index}"
                with self.capture_session() as state:
                    with self.assertRaises(RuntimeError):
                        self.capture.run_capture(self.root, "7.0", output, selected)
                self.assertFalse(output.exists())
                self.assertEqual(state.profiles, [])
                self.assertEqual(state.seed, [])
        for metadata in ("[App]\nVersion=7.0.1\nBuildID=20261007000000\n",
                         "[App]\nVersion=7.0\nBuildID=invalid\n", "[App]\nVersion=7.0\n", ""):
            launcher = self.installed_launcher()
            (launcher.parent / "application.ini").write_text(metadata)
            output = self.root / "bad-build"
            with self.capture_session() as state:
                with self.assertRaises(RuntimeError):
                    self.capture.run_capture(self.root, "7.0", output, launcher)
            self.assertFalse(output.exists())
            self.assertEqual(state.profiles, [])
            self.assertEqual(state.seed, [])

    def test_invalid_exact_release_refuses_before_seed(self):
        launcher = self.installed_launcher()
        for version in ("latest", "7", "7.0beta", "7.0\n", "7.0&other=value", "../7.0"):
            with self.subTest(version=version):
                output = self.root / "invalid-version"
                with self.capture_session() as state:
                    with self.assertRaisesRegex(RuntimeError, "exact release"):
                        self.capture.run_capture(self.root, version, output, launcher)
                self.assertFalse(output.exists())
                self.assertEqual(state.profiles, [])
                self.assertEqual(state.seed, [])

    def test_symlink_launcher_uses_the_selected_app_metadata_and_hash(self):
        launcher = self.installed_launcher()
        alias = self.root / "launcher-alias"
        alias.symlink_to(launcher)
        output = self.root / "symlink-capture"
        with self.capture_session() as state:
            self.capture.run_capture(self.root, "7.0", output, alias)
        self.assertEqual(state.launch[0], str(launcher.resolve()))
        manifest = json.loads((output / "manifest.json").read_text())
        self.assertEqual(manifest["application"]["launcher"], self.capture.file_info(launcher))
        self.assertNotIn("archive", manifest)
        self.assertNotIn(str(alias), json.dumps(manifest))

    def test_existing_pinned_selection_retains_archive_provenance_with_mocked_download(self):
        archive_bytes = io.BytesIO()
        with self.capture.tarfile.open(fileobj=archive_bytes, mode="w") as bundle:
            for name, body, mode in (
                ("Zotero/zotero", b"#!/bin/sh\nexit 99\n", 0o755),
                ("Zotero/app/application.ini", b"[App]\nVersion=7.0\nBuildID=20261007000000\n", 0o644),
            ):
                member = self.capture.tarfile.TarInfo(name)
                member.size, member.mode = len(body), mode
                bundle.addfile(member, io.BytesIO(body))
        resolved = "https://download.zotero.org/client/release/7.0/Zotero.tar.bz2"

        def download(arguments, **kwargs):
            self.assertEqual(arguments[0], "curl")
            self.assertEqual(arguments[-1], "https://www.zotero.org/download/client/dl?channel=release&platform=linux-x86_64&version=7.0")
            Path(arguments[arguments.index("-o") + 1]).write_bytes(archive_bytes.getvalue())
            return types.SimpleNamespace(stdout=resolved)

        work = self.root / "mocked-download"
        work.mkdir()
        with patch.object(self.capture.subprocess, "run", side_effect=download) as command:
            program, provenance = self.capture.resolve_application(work, "7.0", None)
        command.assert_called_once()
        self.assertEqual(program, work / "Zotero/zotero")
        self.assertTrue(os.access(program, os.X_OK))
        self.assertEqual(provenance["application"], {"version": "7.0", "build_id": "20261007000000"})
        self.assertEqual(provenance["archive"], dict(self.capture.file_info(work / "zotero.tar"), url=resolved))

    def test_occupied_port_or_existing_output_refuses_before_seed(self):
        launcher = self.installed_launcher()
        output = self.root / "occupied-port"
        with self.capture_session(occupied=True) as state:
            with self.assertRaises(RuntimeError):
                self.capture.run_capture(self.root, "7.0", output, launcher)
        self.assertFalse(output.exists())
        self.assertEqual(state.seed, [])
        self.assertEqual(state.profiles, [])
        output.mkdir()
        (output / "keep").write_text("invented existing output")
        with self.capture_session() as state:
            with self.assertRaises(RuntimeError):
                self.capture.run_capture(self.root, "7.0", output, launcher)
        self.assertEqual((output / "keep").read_text(), "invented existing output")
        self.assertEqual(state.seed, [])
        self.assertEqual(state.profiles, [])

    def test_failed_owned_shutdown_removes_pages_and_publishes_no_bundle(self):
        launcher = self.installed_launcher()
        output = self.root / "failed-shutdown"
        with self.capture_session(shutdown_error=RuntimeError("owned quit failed")) as state:
            with self.assertRaisesRegex(RuntimeError, "no bundle was published"):
                self.capture.run_capture(self.root, "7.0", output, launcher)
        self.assertEqual(len(state.seed), 1)
        state.quit.assert_called_once()
        state.abort.assert_called_once()
        self.assertFalse(output.exists())

    def test_capture_requires_fresh_private_output(self):
        output = self.root / "capture"
        self.capture.prepare_output("10.0.6", output)
        if os.name != "nt":
            self.assertEqual(output.stat().st_mode & 0o777, 0o700)
        with self.assertRaises(RuntimeError):
            self.capture.prepare_output("10.0.6", output)

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
