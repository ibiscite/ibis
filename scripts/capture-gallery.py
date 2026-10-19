#!/usr/bin/env python3
"""Capture synthetic picker states in a debug evidence build, one app at a time."""
import argparse
import json
import math
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import tempfile
import time


STATES = (
    "first-run", "loading", "empty", "no-results", "many-results", "long-titles",
    "rtl", "missing-file", "invalid-settings", "damaged-library",
    "copy-confirmation", "large-text", "dark", "high-contrast",
)
DIAGNOSTIC_STAGES = (
    "installed", "install-failed", "navigated", "navigation-pending", "navigation-unavailable",
    "frontend-mounted", "effects-settled", "rendered", "event-accepted", "write-failed",
)
PNG_HEADER = b"\x89PNG\r\n\x1a\n"
WINDOWS_CAPTURE = r"""
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
$bitmap = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
try {
  $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
  $bitmap.Save($env:IBIS_GALLERY_SCREENSHOT, [System.Drawing.Imaging.ImageFormat]::Png)
} finally { $graphics.Dispose(); $bitmap.Dispose() }
"""


class CaptureError(Exception):
    """A capture cannot be used as evidence."""


def capture_screenshot(destination, platform_name):
    """Paths are separate arguments or environment data, never shell text."""
    env = os.environ.copy()
    if platform_name == "Linux":
        command = ["import", "-window", "root", str(destination)]
    elif platform_name == "Darwin":
        command = ["screencapture", "-x", "-m", "-t", "png", str(destination)]
    elif platform_name == "Windows":
        env["IBIS_GALLERY_SCREENSHOT"] = str(destination)
        command = ["pwsh", "-NoProfile", "-NonInteractive", "-Command", WINDOWS_CAPTURE]
    else:
        raise CaptureError("unsupported screenshot platform")
    try:
        subprocess.run(command, env=env, check=True, timeout=30, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except FileNotFoundError as error:
        raise CaptureError("native screenshot command is unavailable") from error
    except subprocess.TimeoutExpired as error:
        raise CaptureError("native screenshot capture timed out") from error
    except (OSError, subprocess.SubprocessError) as error:
        raise CaptureError("native screenshot capture failed") from error


def stop_app(process, platform_name):
    """Reap the app and stop its process group/tree, including on failed startup."""
    if platform_name == "Windows":
        # An exited child's PID can be reused; never target that number again.
        if process.poll() is None:
            try:
                subprocess.run(
                    ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                    check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10,
                )
            except (OSError, subprocess.SubprocessError):
                pass  # The process handle below still provides a reliable final kill.
        if process.poll() is None:
            process.kill()
    else:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        if platform_name == "Windows":
            process.kill()
        else:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        process.wait(timeout=5)


def wait_until_rendered(process, ready_file, state, timeout):
    deadline = time.monotonic() + timeout
    while True:
        if process.poll() is not None:
            raise CaptureError(f"app exited before rendering {state}; see its startup log")
        try:
            with ready_file.open("rb") as acknowledgment:
                payload = acknowledgment.read(1025)
        except FileNotFoundError:
            payload = None
        if payload is not None:
            try:
                if len(payload) > 1024 or json.loads(payload) != {"state": state}:
                    raise ValueError("wrong gallery acknowledgment")
            except (ValueError, UnicodeError) as error:
                raise CaptureError(f"app did not acknowledge the selected {state} state") from error
            return
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise CaptureError(f"app did not acknowledge rendering {state}; use a debug evidence build")
        try:
            process.wait(timeout=min(0.1, remaining))
        except subprocess.TimeoutExpired:
            pass


def report_stages(log_path):
    """Print only fixed gallery diagnostics, never arbitrary native log lines."""
    try:
        with log_path.open("rb") as log:
            lines = log.read(65536).splitlines()
        allowed = {f"ibis-gallery: {stage}".encode("ascii"): stage for stage in DIAGNOSTIC_STAGES}
        seen = set()
        for line in lines:
            if line in allowed and line not in seen:
                print(f"Gallery stage: {allowed[line]}", flush=True)
                seen.add(line)
    except OSError:
        print("Gallery stages unavailable.", flush=True)


def capture_failure_screen(output, state, platform_name):
    """One bounded diagnostic attempt; it can never become a valid state image."""
    staging = output / f"failure-{state}.capture.tmp.png"
    try:
        staging.unlink(missing_ok=True)
        capture_screenshot(staging, platform_name)
        with staging.open("rb") as image:
            if image.read(len(PNG_HEADER)) != PNG_HEADER:
                raise CaptureError("diagnostic capture is not a PNG")
        staging.replace(output / f"failure-{state}.png")
        print("Gallery failure screenshot captured.", flush=True)
    except (OSError, subprocess.SubprocessError, CaptureError):
        print("Gallery failure screenshot unavailable.", flush=True)
    finally:
        try:
            staging.unlink(missing_ok=True)
        except OSError:
            pass


def capture_gallery(app, output, *, states=STATES, wait_seconds=8, ready_timeout=15, platform_name=None):
    platform_name = platform_name or platform.system()
    if platform_name not in ("Linux", "Darwin", "Windows"):
        raise CaptureError("unsupported screenshot platform")
    if any(not math.isfinite(value) or value < 0 for value in (wait_seconds, ready_timeout)) or not states or any(state not in STATES for state in states):
        raise CaptureError("invalid gallery states or startup wait")
    app, output = Path(app).resolve(), Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    # A failed rerun must not accidentally upload screenshots from an earlier run.
    for state in states:
        (output / f"{state}.png").unlink(missing_ok=True)
        (output / f"failure-{state}.png").unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix="ibis-synthetic-gallery-") as temporary:
        source = Path(temporary) / "synthetic.bib"
        source.write_text("@misc{syntheticGallery, title={Synthetic gallery source}}\n", encoding="utf8")
        env = {**os.environ, "IBIS_GALLERY": "1", "IBIS_LIBRARY": str(source)}
        for state in states:
            if platform_name == "Windows":
                # WebView2 documents this override of userDataFolder, including
                # an inherited/default profile. Never share a profile between
                # rapidly restarted states: browser files can outlive the host.
                # https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/webview2-idl#createcorewebview2environmentwithoptions
                # Keep all owned profiles until the whole capture transaction
                # finishes; cleanup failure remains fatal, without retries.
                env["WEBVIEW2_USER_DATA_FOLDER"] = tempfile.mkdtemp(prefix="webview2-", dir=temporary)
            env["IBIS_GALLERY_STATE"] = state
            ready_file = Path(temporary) / f"{state}.ready"
            ready_file.unlink(missing_ok=True)
            env["IBIS_GALLERY_READY_FILE"] = str(ready_file)
            staging = output / f"{state}.capture.tmp.png"
            staging.unlink(missing_ok=True)
            try:
                with (output / f"startup-{state}.log").open("wb") as log:
                    options = {"env": env.copy(), "stdout": log, "stderr": subprocess.STDOUT}
                    if platform_name == "Windows":
                        options["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
                    else:
                        options["start_new_session"] = True
                    process = subprocess.Popen([str(app)], **options)
                    try:
                        try:
                            process.wait(timeout=wait_seconds)
                        except subprocess.TimeoutExpired:
                            pass
                        else:
                            raise CaptureError(f"app exited during {state} startup; see its startup log")
                        try:
                            wait_until_rendered(process, ready_file, state, ready_timeout)
                        except CaptureError:
                            log.flush()
                            report_stages(output / f"startup-{state}.log")
                            if process.poll() is None:
                                capture_failure_screen(output, state, platform_name)
                            raise
                        capture_screenshot(staging, platform_name)
                        if process.poll() is not None:
                            raise CaptureError(f"app exited while capturing {state}; see its startup log")
                        try:
                            with staging.open("rb") as image:
                                if image.read(len(PNG_HEADER)) != PNG_HEADER:
                                    raise CaptureError(f"capture for {state} is not a PNG")
                        except OSError as error:
                            raise CaptureError(f"capture for {state} was not written") from error
                        staging.replace(output / f"{state}.png")
                    finally:
                        stop_app(process, platform_name)
            finally:
                staging.unlink(missing_ok=True)
            print(f"Captured {state}", flush=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", required=True, type=Path, help="debug ibis-desktop executable with evidence assets")
    parser.add_argument("--output", required=True, type=Path, help="directory for PNGs and synthetic startup logs")
    parser.add_argument("--wait", type=float, default=8, help="startup wait in seconds per state (default: 8)")
    parser.add_argument("--ready-timeout", type=float, default=15, help="maximum additional wait for rendered-state acknowledgment (default: 15)")
    parser.add_argument("--state", choices=STATES, action="append", help="capture only this state (repeatable; default: all 14)")
    args = parser.parse_args(argv)
    if not args.app.is_file() or (os.name != "nt" and not os.access(args.app, os.X_OK)):
        parser.error("--app must name an executable regular file")
    if not math.isfinite(args.wait) or args.wait < 0:
        parser.error("--wait must be a finite nonnegative number")
    if not math.isfinite(args.ready_timeout) or args.ready_timeout < 0:
        parser.error("--ready-timeout must be a finite nonnegative number")
    if args.output.exists() and not args.output.is_dir():
        parser.error("--output must name a directory")
    try:
        capture_gallery(args.app, args.output, states=tuple(args.state or STATES), wait_seconds=args.wait, ready_timeout=args.ready_timeout)
    except CaptureError as error:
        print(f"Gallery capture failed: {error}.", file=sys.stderr)
        return 1
    except (OSError, subprocess.SubprocessError):
        # Startup logs remain local artifacts; do not print their payloads here.
        print("Gallery capture failed; inspect the state startup logs and native capture errors.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
