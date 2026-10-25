#!/usr/bin/env bash
# Runs the real-Zotero test (crates/ibis-zotero-local/tests/real_zotero.rs)
# against an actual Zotero: downloads the current Linux release, starts it
# with a fresh, throwaway profile and data directory and the local API
# turned on, then runs the test. Linux only; needs xvfb-run and network
# access to zotero.org. Zotero listens on a port of its own (not 23119), so a
# Zotero you have open is never touched; nothing outside a temporary
# directory is written.
#
#   scripts/real-zotero.sh
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
# Capture is deliberately opt-in; the ordinary live-API check below is unchanged.
#   scripts/real-zotero.sh --capture 10.0.6 /fresh/private/output
if [ "$#" -ne 0 ]; then
  if [ "$#" -ne 3 ] || [ "$1" != --capture ]; then
    echo "usage: real-zotero.sh [--capture EXACT_VERSION FRESH_OUTPUT]" >&2; exit 1
  fi
  xvfb-run -a python3 - "$root" "$2" "$3" <<'PY_CAPTURE'
import configparser
import ctypes as C
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import sqlite3
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.request
from urllib.parse import quote, urlparse

MAX_PAGE_BYTES = 1024 * 1024
MAX_DATABASE_BYTES = 128 * 1024 * 1024
MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
API_PATH = "users/0/items/top?format=json&include=data,csljson&sort=dateAdded&direction=asc&limit=2&start="


def prepare_output(version, output):
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:\.[0-9]+)?", version):
        raise RuntimeError("capture requires an exact release version")
    try:
        output.mkdir(mode=0o700)
    except OSError:
        raise RuntimeError("capture output must be a fresh private directory") from None


def application_info(directory, version):
    config = configparser.ConfigParser(interpolation=None)
    paths = [directory / "app/application.ini", directory / "application.ini"]
    try:
        path = next(path for path in paths if path.is_file())
        config.read(path)
        actual, build = config["App"]["Version"], config["App"]["BuildID"]
        if actual != version or not re.fullmatch(r"[0-9]{8,20}", build):
            raise ValueError()
        return {"version": actual, "build_id": build}
    except (StopIteration, OSError, KeyError, ValueError, configparser.Error):
        raise RuntimeError("downloaded application build does not match the requested release") from None


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *_args, **_kwargs):
        raise RuntimeError("capture API redirects are forbidden")


def capture_api(port, output, opener=None):
    opener = opener or urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    keys, pages, version = set(), [], None
    for start in (0, 2):
        path = API_PATH + str(start)
        request = urllib.request.Request("http://127.0.0.1:" + str(port) + "/api/" + path,
                                         headers={"User-Agent": "Ibis-Fixture-Capture", "Accept-Encoding": "identity"})
        try:
            with opener.open(request, timeout=5) as response:
                body = response.read(MAX_PAGE_BYTES + 1)
                headers = {name: response.headers.get(name, "") for name in
                           ("Zotero-API-Version", "Total-Results", "Last-Modified-Version")}
                current = headers["Last-Modified-Version"]
                if response.status != 200 or len(body) > MAX_PAGE_BYTES or headers["Zotero-API-Version"] != "3" or headers["Total-Results"] != "4" or not current.isascii() or not current.isdigit():
                    raise ValueError()
                if version is not None and current != version:
                    raise ValueError()
                version = current
                items = json.loads(body)
                if not isinstance(items, list) or len(items) != 2:
                    raise ValueError()
                for item in items:
                    key = item.get("key") if isinstance(item, dict) else None
                    if not isinstance(key, str) or not re.fullmatch(r"[A-Z0-9]{8}", key) or key in keys:
                        raise ValueError()
                    keys.add(key)
        except Exception:
            raise RuntimeError("capture API pages are incomplete, inconsistent or oversized") from None
        name = f"api-{len(pages):03}.json"
        (output / name).write_bytes(body)
        pages.append({"file": name, "request": path, "headers": headers})
    return keys, pages


def copy_bundle(data, output, api_keys):
    names = []
    for name in ("zotero.sqlite", "zotero.sqlite-wal", "zotero.sqlite-shm"):
        source = data / name
        if not os.path.lexists(source) and name != "zotero.sqlite":
            continue
        if source.is_symlink() or not source.is_file() or source.stat().st_size > MAX_DATABASE_BYTES:
            raise RuntimeError("capture database bundle is missing, nonregular or oversized")
        shutil.copyfile(source, output / name)
        (output / name).chmod(0o600)
        names.append(name)
    # Integrity checks run on another copy, preserving original bundle bytes.
    with tempfile.TemporaryDirectory() as temporary:
        validation = Path(temporary)
        for name in names:
            shutil.copyfile(output / name, validation / name)
        try:
            connection = sqlite3.connect("file:" + quote(str(validation / "zotero.sqlite")) + "?mode=ro", uri=True)
            try:
                deadline = time.monotonic() + 5
                connection.set_progress_handler(lambda: int(time.monotonic() > deadline), 1000)
                if connection.execute("PRAGMA quick_check(1)").fetchall() != [("ok",)]:
                    raise ValueError()
                rows = connection.execute("SELECT key FROM items").fetchall()
                if len(rows) != 4 or {row[0] for row in rows} != api_keys:
                    raise ValueError()
                deleted = connection.execute("SELECT count(*) FROM deletedItems").fetchone()[0]
                if deleted != 0:
                    raise ValueError()
                versions = dict(connection.execute("SELECT schema, version FROM version"))
            finally:
                connection.close()
        except (sqlite3.Error, ValueError):
            raise RuntimeError("copied database integrity or API parity check failed") from None
    return {"schema_versions": versions, "counts": {"items": 4, "deleted_items": deleted},
            "database_files": names, "quick_check": "ok"}


def file_info(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(65536), b""):
            digest.update(block)
    return {"sha256": digest.hexdigest(), "bytes": path.stat().st_size}


# Native ABI references: Xlib/XTEST official manuals; helper stays inside this private display.
def finish_capture(app, data, output, keys):
    graceful_quit(app)
    return copy_bundle(data, output, keys)


def run_capture(root, version, output):
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("capture requires the isolated Linux x86-64 entrypoint")
    prepare_output(version, output)
    app = None
    work_handle = None
    try:
        revision = subprocess.run(["git", "rev-parse", "HEAD"], cwd=root, check=True,
                                  capture_output=True, text=True, timeout=5).stdout.strip()
        if not re.fullmatch(r"[a-f0-9]{40}", revision):
            raise RuntimeError("capture generator revision is unavailable")
        subprocess.run(["git", "diff", "--quiet", "HEAD", "--", "scripts/real-zotero.sh"],
                       cwd=root, check=True, capture_output=True, timeout=5)
        enable_subreaper()
        port = 23129
        work_handle = tempfile.TemporaryDirectory()
        temporary = work_handle.name
        work = Path(temporary)
        archive = work / "zotero.tar"
        url = "https://www.zotero.org/download/client/dl?channel=release&platform=linux-x86_64&version=" + version
        downloaded = subprocess.run(["curl", "-fsSL", "--retry", "3", "--max-time", "180", "--max-filesize", str(MAX_ARCHIVE_BYTES), "-o", str(archive), "-w", "%{url_effective}", url],
                                    check=True, capture_output=True, text=True, timeout=200)
        resolved = downloaded.stdout
        parsed = urlparse(resolved)
        if parsed.scheme != "https" or parsed.hostname != "download.zotero.org" or not parsed.path.startswith("/client/release/" + version + "/"):
            raise RuntimeError("download did not resolve to the pinned official release")
        if archive.stat().st_size > MAX_ARCHIVE_BYTES:
            raise RuntimeError("application archive exceeds the capture limit")
        archive_info = file_info(archive)
        with tarfile.open(archive) as bundle:
            members, expanded = [], 0
            for member in bundle:
                members.append(member)
                expanded += max(0, member.size)
                if len(members) > 20000 or expanded > 2 * 1024 * 1024 * 1024:
                    raise RuntimeError("application archive exceeds extraction limits")
            bundle.extractall(work, members=members, filter="data")
        programs = [path for path in work.glob("*/zotero") if path.is_file() and os.access(path, os.X_OK)]
        if len(programs) != 1:
            raise RuntimeError("download does not contain one Zotero launcher")
        program = programs[0]
        provenance = application_info(program.parent, version)
        profile, data = work / "profile", work / "data"
        profile.mkdir(mode=0o700)
        data.mkdir(mode=0o700)
        prefs = {"dataDir": str(data), "useDataDir": True, "httpServer.localAPI.enabled": True,
                 "httpServer.port": port, "lastViewedFolder": "L1", "firstRun2": False,
                 "firstRunGuidance": False, "sync.autoSync": False, "automaticScraperUpdates": False,
                 "reportTranslationFailure": False, "downloadAssociatedFiles": False, "automaticSnapshots": False}
        (profile / "user.js").write_text("".join("user_pref(" + json.dumps("extensions.zotero." + key) + ", " + json.dumps(value) + ");\n" for key, value in prefs.items()) +
                                       'user_pref("app.update.enabled", false);\nuser_pref("app.update.auto", false);\n')
        with (work / "zotero.log").open("wb") as log:
            # Refuse any preexisting listener before launching a writable seed.
            import socket
            with socket.socket() as probe:
                probe.settimeout(1)
                if probe.connect_ex(("127.0.0.1", port)) == 0:
                    raise RuntimeError("capture port is already occupied")
            app = subprocess.Popen([str(program), "-no-remote", "-profile", str(profile)],
                                   stdout=log, stderr=log, start_new_session=True)
            deadline = time.monotonic() + 120
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
            while True:
                if app.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("owned Zotero did not become ready")
                try:
                    with opener.open(f"http://127.0.0.1:{port}/connector/ping", timeout=2) as response:
                        if response.status == 200:
                            break
                except Exception:
                    time.sleep(0.25)
            time.sleep(10)
            environment = dict(os.environ, IBIS_ZOTERO_PORT=str(port), IBIS_THROWAWAY_ZOTERO="1")
            with (work / "seed.log").open("wb") as seed_log:
                subprocess.run(["cargo", "test", "-p", "ibis-zotero-local", "--locked", "--test", "real_zotero", "--", "--ignored"],
                               cwd=root, env=environment, stdout=seed_log, stderr=seed_log, check=True, timeout=600)
            keys, pages = capture_api(port, output, opener)
            metadata = finish_capture(app, data, output, keys)
            app = None
        manifest = {"coverage": "partial: personal library, two articles, one book and one report; not a supported-version declaration",
                    "generator_commit": revision, "application": provenance,
                    "platform": "linux-x86_64", "archive": dict(archive_info, url=resolved),
                    "shutdown": "owned Ctrl+Q, normal exit and helpers reaped", "api_pages": pages,
                    **metadata}
        manifest["files"] = {path.name: file_info(path) for path in output.iterdir()}
        (output / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    except BaseException:
        if app is not None:
            try:
                if app.poll() is None:
                    os.killpg(app.pid, signal.SIGKILL)  # Failure cleanup never yields a manifest.
                    app.wait(timeout=5)
            except (OSError, subprocess.TimeoutExpired):
                pass
        shutil.rmtree(output)
        raise RuntimeError("synthetic capture failed; no bundle was published") from None
    finally:
        if work_handle is not None:
            work_handle.cleanup()


if __name__ == "__main__":
    try:
        run_capture(Path(sys.argv[1]), sys.argv[2], Path(sys.argv[3]))
    except Exception:
        print("synthetic capture failed; no bundle was published", file=sys.stderr)
        sys.exit(1)
PY_CAPTURE
  exit
fi
# A port of its own, so a Zotero the developer has open is never written to.
port=23129
if curl -s -m 2 -o /dev/null "http://127.0.0.1:$port/"; then
  echo "something already listens on port $port; stop it first" >&2; exit 1
fi
work=$(mktemp -d)
zotero_pid=
cleanup() {
  # Zotero runs in its own process group, so this also stops its helpers.
  [ -n "$zotero_pid" ] && kill -- "-$zotero_pid" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

echo "Downloading Zotero for Linux"
curl -fsSL --retry 3 -o "$work/zotero.tar" \
  "https://www.zotero.org/download/client/dl?channel=release&platform=linux-x86_64"
tar -xf "$work/zotero.tar" -C "$work"
app=$(find "$work" -maxdepth 2 -type f -name zotero -perm -u+x | head -n 1)
[ -n "$app" ] || { echo "the Zotero download has no zotero program" >&2; exit 1; }
grep -m1 '^Version=' "$(dirname "$app")/app/application.ini" 2>/dev/null \
  || grep -m1 '^Version=' "$(dirname "$app")/application.ini" 2>/dev/null || true

mkdir -p "$work/profile" "$work/data"
cat > "$work/profile/user.js" <<EOF
user_pref("extensions.zotero.dataDir", "$work/data");
user_pref("extensions.zotero.useDataDir", true);
user_pref("extensions.zotero.httpServer.localAPI.enabled", true);
user_pref("extensions.zotero.httpServer.port", $port);
user_pref("extensions.zotero.lastViewedFolder", "L1");
user_pref("extensions.zotero.firstRun2", false);
user_pref("extensions.zotero.firstRunGuidance", false);
user_pref("extensions.zotero.sync.autoSync", false);
user_pref("extensions.zotero.automaticScraperUpdates", false);
user_pref("extensions.zotero.reportTranslationFailure", false);
user_pref("app.update.enabled", false);
user_pref("app.update.auto", false);
EOF

echo "Starting Zotero"
setsid xvfb-run -a "$app" -profile "$work/profile" > "$work/zotero.log" 2>&1 &
zotero_pid=$!

for _ in $(seq 120); do
  if curl -fsS -m 5 -o /dev/null "http://127.0.0.1:$port/connector/ping" 2>/dev/null; then break; fi
  if ! kill -0 "$zotero_pid" 2>/dev/null; then
    echo "Zotero exited during startup:" >&2; tail -n 50 "$work/zotero.log" >&2; exit 1
  fi
  sleep 1
done
curl -fsS -m 5 -o /dev/null "http://127.0.0.1:$port/connector/ping" \
  || { echo "Zotero did not start within two minutes:" >&2; tail -n 50 "$work/zotero.log" >&2; exit 1; }
# The connector answers before the main window has opened; give the
# library view a moment so new items land in My Library.
sleep 10

cd "$root"
if ! IBIS_ZOTERO_PORT=$port IBIS_THROWAWAY_ZOTERO=1 cargo test -p ibis-zotero-local --locked --test real_zotero -- --ignored --nocapture; then
  echo "Zotero log:" >&2; tail -n 80 "$work/zotero.log" >&2; exit 1
fi
