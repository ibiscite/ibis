#!/usr/bin/env bash
# Runs the real-Zotero test (crates/ibis-zotero-local/tests/real_zotero.rs)
# against an actual Zotero: downloads the current Linux release, starts it
# with a fresh, throwaway profile and data directory and the local API
# turned on, then runs the test. Linux only; needs xvfb-run and network
# access to zotero.org. Zotero listens on a port of its own (not 23119), so a
# Zotero you have open is never touched. The ordinary run writes only to a
# temporary directory; --capture retains a synthetic bundle in fresh output.
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


# Linux subreaper plus bounded xdotool on the private display; no custom Xlib ABI.
_SUBREAPER_ENABLED = False
_FAILURE = "Zotero did not complete an owned clean shutdown."


def enable_subreaper() -> None:
    global _SUBREAPER_ENABLED
    try:
        libc = C.CDLL(None, use_errno=True)
        libc.prctl.argtypes = [C.c_int] + [C.c_ulong] * 4
        libc.prctl.restype = C.c_int
        if libc.prctl(36, 1, 0, 0, 0) != 0:  # PR_SET_CHILD_SUBREAPER
            raise RuntimeError()
        _SUBREAPER_ENABLED = True
    except Exception:
        raise RuntimeError(_FAILURE) from None


class _Attributes(C.Structure):
    # XWindowAttributes, including native-long and pointer alignment on LP64.
    _fields_ = [(name, kind) for names, kind in [
        ("x y width height border_width depth", C.c_int), ("visual", C.c_void_p),
        ("root", C.c_ulong), ("window_class bit_gravity win_gravity backing_store", C.c_int),
        ("backing_planes backing_pixel", C.c_ulong), ("save_under", C.c_int),
        ("colormap", C.c_ulong), ("map_installed map_state", C.c_int),
        ("all_event_masks your_event_mask do_not_propagate_mask", C.c_long),
        ("override_redirect", C.c_int), ("screen", C.c_void_p),
    ] for name in names.split()]


def _process(pid):
    # Linux stat fields 4/22; rsplit tolerates spaces and ')' in comm.
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return int(fields[1]), int(fields[19])


def _owned(pid, root):
    for _ in range(64):
        if pid == root:
            return True
        if pid <= 1:
            return False
        pid = _process(pid)[0]
    return False


def _request_quit(pid, started):
    if _process(pid)[1] != started:
        raise RuntimeError()
    x, xt = C.CDLL("libX11.so.6"), C.CDLL("libXtst.so.6")
    D, W, I, U, P = C.c_void_p, C.c_ulong, C.c_int, C.c_uint, C.POINTER
    for library, name, result, args in [
        (x, "XOpenDisplay", D, [C.c_char_p]), (x, "XDefaultRootWindow", W, [D]),
        (x, "XQueryTree", I, [D, W, P(W), P(W), P(P(W)), P(U)]),
        (x, "XGetWindowAttributes", I, [D, W, P(_Attributes)]),
        (x, "XInternAtom", W, [D, C.c_char_p, I]),
        (x, "XGetWindowProperty", I, [D, W, W, C.c_long, C.c_long, I, W,
                                      P(W), P(I), P(W), P(W), P(D)]),
        (x, "XFree", I, [D]), (x, "XSetInputFocus", I, [D, W, I, W]),
        (x, "XGetInputFocus", I, [D, P(W), P(I)]), (x, "XSync", I, [D, I]),
        (x, "XStringToKeysym", W, [C.c_char_p]),
        (x, "XKeysymToKeycode", C.c_ubyte, [D, W]),
        (xt, "XTestQueryExtension", I, [D, P(I), P(I), P(I), P(I)]),
        (xt, "XTestFakeKeyEvent", I, [D, U, I, W]),
        (x, "XCloseDisplay", I, [D]),
    ]:
        function = getattr(library, name)
        function.restype, function.argtypes = result, args
    display = x.XOpenDisplay(None)
    if not display:
        raise RuntimeError()

    def property_value(window, name, expected_type, expected_format):
        actual, count, remaining, form, value = W(), W(), W(), I(), D()
        atom = x.XInternAtom(display, name, 1)
        if not atom:
            return None
        status = x.XGetWindowProperty(display, window, atom, 0, 64, 0, expected_type,
                                      C.byref(actual), C.byref(form), C.byref(count),
                                      C.byref(remaining), C.byref(value))
        try:
            if status or not value or actual.value != expected_type or form.value != expected_format or remaining.value:
                return None
            if expected_format == 32:
                return C.cast(value, P(W))[0] if count.value == 1 else None
            return C.string_at(value, count.value)
        finally:
            if value:
                x.XFree(value)

    try:
        values = [I() for _ in range(4)]
        if not xt.XTestQueryExtension(display, *(C.byref(value) for value in values)):
            raise RuntimeError()
        pending, seen, candidates = [x.XDefaultRootWindow(display)], set(), []
        while pending:
            window = pending.pop()
            if window in seen:
                continue
            seen.add(window)
            if len(seen) > 512:
                raise RuntimeError()
            owner = property_value(window, b"_NET_WM_PID", 6, 32)  # XA_CARDINAL
            wm_class = property_value(window, b"WM_CLASS", 31, 8)  # XA_STRING
            attributes = _Attributes()
            if owner and wm_class and b"zotero" in wm_class.lower().split(b"\0") and _owned(owner, pid):
                if x.XGetWindowAttributes(display, window, C.byref(attributes)) and attributes.map_state == 2 and not attributes.override_redirect:
                    candidates.append((window, owner))
            root, parent, children, count = W(), W(), P(W)(), U()
            if not x.XQueryTree(display, window, C.byref(root), C.byref(parent), C.byref(children), C.byref(count)):
                raise RuntimeError()
            try:
                if count.value > 512 or len(pending) + count.value > 512:
                    raise RuntimeError()
                pending.extend(children[index] for index in range(count.value))
            finally:
                if children:
                    x.XFree(children)
        if len(candidates) != 1:
            raise RuntimeError()
        window, owner = candidates[0]
        x.XSetInputFocus(display, window, 2, 0)  # RevertToParent, CurrentTime
        x.XSync(display, 0)
        focus, revert = W(), I()
        x.XGetInputFocus(display, C.byref(focus), C.byref(revert))
        if focus.value != window or not _owned(owner, pid) or _process(pid)[1] != started:
            raise RuntimeError()
        keys = [x.XKeysymToKeycode(display, x.XStringToKeysym(key)) for key in (b"Control_L", b"q")]
        if not all(keys):
            raise RuntimeError()
        for key, pressed in [(keys[0], 1), (keys[1], 1), (keys[1], 0), (keys[0], 0)]:
            if not xt.XTestFakeKeyEvent(display, key, pressed, 0):
                raise RuntimeError()
        x.XSync(display, 0)
    finally:
        x.XCloseDisplay(display)


def graceful_quit(app: subprocess.Popen, timeout=30) -> None:
    try:
        if not _SUBREAPER_ENABLED or timeout <= 0 or app.poll() is not None:
            raise RuntimeError()
        deadline, started = time.monotonic() + timeout, _process(app.pid)[1]
        worker = os.fork()
        if worker == 0:
            # Native Xlib errors/blocked server reads stay inside this bounded child.
            try:
                with open(os.devnull, "wb") as sink:
                    os.dup2(sink.fileno(), 2)
                signal.signal(signal.SIGALRM, signal.SIG_DFL)
                signal.setitimer(signal.ITIMER_REAL, timeout)
                _request_quit(app.pid, started)
                os._exit(0)
            except BaseException:
                os._exit(1)
        while True:
            child, status = os.waitpid(worker, os.WNOHANG)
            if child:
                if not os.WIFEXITED(status) or os.WEXITSTATUS(status):
                    raise RuntimeError()
                break
            if time.monotonic() >= deadline:
                raise RuntimeError()
            time.sleep(0.01)
        remaining = deadline - time.monotonic()
        if remaining <= 0 or app.wait(timeout=remaining) != 0:
            raise RuntimeError()
        while True:
            try:
                child, _ = os.waitpid(-1, os.WNOHANG)
            except ChildProcessError:
                return
            if time.monotonic() >= deadline:
                raise RuntimeError()
            if not child:
                time.sleep(0.01)
    except Exception:
        raise RuntimeError(_FAILURE) from None


def finish_capture(app, data, output, keys):
    graceful_quit(app)
    return copy_bundle(data, output, keys)


def generator_revision(root):
    try:
        revision = subprocess.run(["git", "-c", "core.fsmonitor=false", "rev-parse", "HEAD"], cwd=root,
                                  check=True, capture_output=True, text=True, timeout=5).stdout.strip()
        if not re.fullmatch(r"[a-f0-9]{40}", revision):
            raise ValueError()
        # Seed tests and compiled crates are generator inputs too. Untracked
        # private notes/build outputs are neither read nor included.
        subprocess.run(["git", "-c", "core.fsmonitor=false", "diff", "--quiet", "HEAD"],
                       cwd=root, check=True, capture_output=True, timeout=5)
        return revision
    except Exception:
        raise RuntimeError("capture requires a clean committed generator tree") from None


def abort_owned(app, timeout=5):
    """Failure cleanup only; direct children stay PID-pinned until we reap them."""
    deadline = time.monotonic() + timeout
    try:
        if app is not None and app.poll() is None:
            os.killpg(app.pid, signal.SIGKILL)
            app.wait(timeout=max(0.01, deadline - time.monotonic()))
        while time.monotonic() < deadline:
            children = Path(f"/proc/self/task/{os.getpid()}/children").read_text().split()
            if len(children) > 256:
                return False
            for child in children:
                pid = int(child)
                try:
                    if _process(pid)[0] == os.getpid():
                        os.kill(pid, signal.SIGKILL)
                except (ProcessLookupError, FileNotFoundError):
                    pass
            for _ in range(256):
                try:
                    pid, _ = os.waitpid(-1, os.WNOHANG)
                except ChildProcessError:
                    return True
                if not pid:
                    break
            time.sleep(0.01)
    except Exception:
        return False
    return False


def run_capture(root, version, output):
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("capture requires the isolated Linux x86-64 entrypoint")
    prepare_output(version, output)
    app = None
    work, quiet, phase = None, True, "generator provenance"
    try:
        revision = generator_revision(root)
        enable_subreaper()
        port = 23129
        work = Path(tempfile.mkdtemp())
        phase = "pinned archive"
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
        phase = "synthetic profile"
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
            phase = "owned startup"
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
            phase = "owned synthetic seeding"
            environment = dict(os.environ, IBIS_ZOTERO_PORT=str(port), IBIS_THROWAWAY_ZOTERO="1",
                               IBIS_ZOTERO_CAPTURE_PID=str(app.pid), IBIS_ZOTERO_CAPTURE_START=str(_process(app.pid)[1]))
            with (work / "seed.log").open("wb") as seed_log:
                subprocess.run(["cargo", "test", "-p", "ibis-zotero-local", "--locked", "--test", "real_zotero", "--", "--ignored"],
                               cwd=root, env=environment, stdout=seed_log, stderr=seed_log, check=True, timeout=600)
            phase = "paired API pages"
            keys, pages = capture_api(port, output, opener)
            phase = "owned Quit and copied-bundle validation"
            metadata = finish_capture(app, data, output, keys)
            app = None
        phase = "capture manifest"
        manifest = {"coverage": "partial: personal library, two articles, one book and one report; not a supported-version declaration",
                    "generator_commit": revision, "application": provenance,
                    "platform": "linux-x86_64", "archive": dict(archive_info, url=resolved),
                    "shutdown": "owned Ctrl+Q, normal exit and helpers reaped", "api_pages": pages,
                    **metadata}
        manifest["files"] = {path.name: file_info(path) for path in output.iterdir()}
        (output / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    except BaseException:
        quiet = abort_owned(app)
        shutil.rmtree(output)
        cleanup = "" if quiet else "; owned cleanup incomplete, temporary profile quarantined"
        raise RuntimeError("synthetic capture failed during " + phase + cleanup + "; no bundle was published") from None
    finally:
        if work is not None and quiet:
            shutil.rmtree(work)


if __name__ == "__main__":
    try:
        run_capture(Path(sys.argv[1]), sys.argv[2], Path(sys.argv[3]))
    except RuntimeError as error:
        print(str(error), file=sys.stderr)  # Only fixed/allowlisted capture phases reach here.
        sys.exit(1)
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
