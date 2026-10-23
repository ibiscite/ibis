#!/usr/bin/env python3
"""End-to-end update test for Ibis Nightly (#319, #493).

Installs the previous Ibis Nightly with automatic update checks on, waits for
it to offer the newly published nightly, accepts the install/restart action through
the system's accessibility interface and checks that the installed app was
replaced by the new version and that the new version started in its place.

- Linux: the AppImage, under a virtual display, pressed through AT-SPI.
- Windows: the setup executable, pressed through UI Automation; the updater
  runs the new setup executable, which starts the app again.
- macOS: the .app from its update archive, pressed through System Events; the
  updater replaces the .app and reopens it through LaunchServices.

    xvfb-run -a dbus-run-session -- /usr/bin/python3 scripts/update-e2e.py \\
        --previous previous/Ibis-Linux.AppImage --expected dist/Ibis-Linux.AppImage --output out
    python scripts/update-e2e.py --previous previous/Ibis-Windows-setup.exe \\
        --expected dist/Ibis-Windows-setup.exe --version 0.1.1-nightly.20261019223000 --output out
    python3 scripts/update-e2e.py --previous previous/Ibis-macOS-Apple-silicon.app.tar.gz \\
        --expected dist/Ibis-macOS-Apple-silicon.app.tar.gz --output out
    python3 scripts/update-e2e.py --self-test

Windows and macOS change the signed-in account's own settings folder and
installed apps, so run it only on a disposable account such as a CI runner. It
refuses to start when Ibis Nightly settings already exist there.
"""

import argparse
import hashlib
import json
import os
import platform
import plistlib
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import MagicMock, patch
import urllib.request
from pathlib import Path

IDENTIFIER = "io.github.ibiscite.ibis.nightly"
BUTTON = "Install and Restart"
# The app's first check comes 30 seconds after start; the download is ~100 MB.
OFFER_TIMEOUT = 180
REPLACE_TIMEOUT = 300
RESTART_TIMEOUT = 60
UNINSTALL = r"Software\Microsoft\Windows\CurrentVersion\Uninstall"

# Lists the named processes' windows and named elements, or presses the named
# button. Prints {"pressed": bool, "seen": [...], "probe": {...}}, where the
# probe holds only counts, error numbers or exception type names, and how a
# press was made.
WINDOWS_LOOK = r"""
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]
$S = [System.Windows.Automation.TreeScope]
$seen = [System.Collections.Generic.List[string]]::new()
$probe = @{ pids = 0; windows = 0; elements = 0; errors = [System.Collections.Generic.List[string]]::new()
  attempted = $false; blocked = $false; offer_matches = 0; action_matches = 0
  invoke_available = $false; enabled = $false; on_screen = $false }
# Only the exception's type name is kept: its message can name paths or windows.
function Note($id, $error) { if ($probe.errors.Count -lt 50) { $probe.errors.Add("[$id] " + $error.Exception.GetType().Name) } }
function Answer($pressed, $how) {
  $probe.errors = @($probe.errors); $probe.pressed_by = $how
  @{ pressed = $pressed; seen = @($seen); probe = $probe } | ConvertTo-Json -Compress -Depth 4
}
try {
  $ids = @($env:IBIS_PIDS -split ',' | Where-Object { $_ } | Select-Object -Unique)
  if ($ids.Count -gt 20 -or ($env:IBIS_PRESS -and $env:IBIS_PRESS -cne 'Install and Restart')) { throw 'Invalid update scope' }
  $owned = @(); $offers = @()
  foreach ($id in $ids) {
    if ($id -notmatch '^[1-9][0-9]{0,9}$') { throw 'Invalid update process' }
    $probe.pids++
    $condition = [System.Windows.Automation.PropertyCondition]::new($A::ProcessIdProperty, [int]$id)
    $windows = @($A::RootElement.FindAll($S::Children, $condition))
    $probe.windows += $windows.Count
    if ($probe.windows -gt 20) { throw 'Update window bound exceeded' }
    foreach ($window in $windows) {
      if ($window.Current.ProcessId -ne [int]$id) { throw 'Update window owner changed' }
      $owned += $window
      $seen.Add("[$id] window: " + $window.Current.Name)
      if ($window.Current.Name -ceq 'An update is available') { $offers += $window }
    }
  }
  $probe.offer_matches = $offers.Count
  if ($env:IBIS_PRESS -and $offers.Count -gt 1) { throw 'Update offer is ambiguous' }
  $places = if ($env:IBIS_PRESS) { $offers } else { $owned }
  $matches = @()
  foreach ($window in $places) {
    $items = @($window.FindAll($S::Descendants, [System.Windows.Automation.Condition]::TrueCondition))
    $probe.elements += $items.Count
    if ($probe.elements -gt 4000) { throw 'Update element bound exceeded' }
    foreach ($item in $items) {
      $name = $item.Current.Name
      if ($env:IBIS_PRESS) {
        # Older nightlies use the exact custom label. New Windows offers
        # use the OS-localized Yes button: IDYES is 6, never IDNO (7).
        $custom = $name -ceq $env:IBIS_PRESS
        $standard = !$custom -and $item.Current.ControlType.ProgrammaticName -ceq 'ControlType.Button' -and $item.Current.AutomationId -ceq '6'
        if ($custom -or $standard) { $matches += $item }
      }
      if ($name -and $seen.Count -lt 200) { $seen.Add($item.Current.ControlType.ProgrammaticName + ': ' + $name) }
    }
  }
  $probe.action_matches = $matches.Count
  if ($env:IBIS_PRESS -and $offers.Count -eq 1 -and $matches.Count -gt 0) {
    if ($matches.Count -ne 1) { throw 'Update action is ambiguous' }
    $item = $matches[0]; $window = $offers[0]
    if ($item.Current.ProcessId -ne $window.Current.ProcessId) { throw 'Update action owner changed' }
    $probe.enabled = $item.Current.IsEnabled -and $window.Current.IsEnabled
    $probe.on_screen = !$item.Current.IsOffscreen -and !$window.Current.IsOffscreen
    $invoke = $null
    $probe.invoke_available = $item.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$invoke)
    if (!$probe.enabled -or !$probe.on_screen -or !$probe.invoke_available -or !$invoke) { throw 'Update action unavailable' }
    # Only absence polls. An uncertain invocation must never reach a retry,
    # and a successful invocation may immediately destroy the AX element.
    $probe.attempted = $true
    $invoke.Invoke()
    Answer $true 'invoke'
    exit 0
  }
} catch { $probe.blocked = $true; Note 0 $_ }
Answer $false $null
"""

MACOS_LOOK = r"""
ObjC.import('ApplicationServices');
function run(argv) {
  const [press, ...pids] = argv;
  const events = Application('System Events');
  const seen = [];
  // Only Apple event error numbers are kept, never their text.
  const probe = { pids: pids.length, processes: 0, windows: 0, errors: [], trusted: null, pressed_by: null };
  try { probe.trusted = Boolean($.AXIsProcessTrusted()); } catch (e) { probe.trusted = null; }
  const note = (pid, e) => { if (probe.errors.length < 50) probe.errors.push(`[${pid}] ${Number.isInteger(e && e.errorNumber) ? e.errorNumber : 'error'}`); };
  const text = (item) => { try { return String(item.name() || item.value() || ''); } catch (e) { return ''; } };
  const processes = (pid) => {
    try { return events.processes.whose({ unixId: Number(pid) })(); }
    catch (e) { note(pid, e); return []; }  // The process quit while being read, or no permission.
  };
  const answer = () => JSON.stringify({ pressed: probe.pressed_by !== null, seen: seen.slice(0, 200), probe });
  for (const pid of pids) {
    const apps = processes(pid);
    probe.processes += apps.length;
    for (const app of apps) {
      let windows = [];
      try { windows = app.windows(); } catch (e) { note(pid, e); continue; }
      probe.windows += windows.length;
      for (const window of windows) {
        seen.push(`[${pid}] window: ${text(window)}`);
        let places = [window];
        try { places = places.concat(window.sheets()); } catch (e) { note(pid, e); }
        for (const place of places) {
          let buttons = [], labels = [];
          try { buttons = place.buttons(); } catch (e) { note(pid, e); }
          try { labels = place.staticTexts(); } catch (e) { note(pid, e); }
          for (const button of buttons) {
            if (press && text(button) === press) {
              try { button.click(); probe.pressed_by = 'click'; return answer(); } catch (e) { note(pid, e); }
            }
            seen.push(`[${pid}] button: ${text(button)}`);
          }
          for (const label of labels) seen.push(`[${pid}] text: ${text(label)}`);
        }
      }
    }
  }
  return answer();
}
"""


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def seed_settings(config):
    """Update checks already allowed, so the first-launch question never shows."""
    settings = config / IDENTIFIER / "settings.json"
    settings.parent.mkdir(parents=True, exist_ok=True)
    settings.write_text(json.dumps({"schema_version": 2, "update_checks": True}), encoding="utf-8")
    return settings


def claim_settings(config):
    """Windows and macOS have no settings-folder override, so the test uses the
    account's own folder and must never overwrite someone's real settings."""
    folder = config / IDENTIFIER
    if any(os.path.lexists(folder / name) for name in ("settings.json", "settings.json.bak", "settings.json.tmp")):
        raise SystemExit(f"Ibis Nightly settings already exist in {folder}; use a disposable account")
    return seed_settings(config)


def clean_settings(settings):
    for name in ("settings.json", "settings.json.bak", "settings.json.tmp"):
        (settings.parent / name).unlink(missing_ok=True)


def environment(inherited, config, library):
    env = dict(inherited, IBIS_LIBRARY=str(library))
    if config is not None:
        env.update(XDG_CONFIG_HOME=str(config), NO_AT_BRIDGE="0", GTK_MODULES="gail:atk-bridge")
    for name in ("APPIMAGE", "APPDIR", "IBIS_GALLERY", "IBIS_GALLERY_STATE"):
        env.pop(name, None)
    return env


def look_result(stdout):
    """The helpers' answer; anything else is a helper failure, never a pass.
    The optional probe holds fixed counts, error codes and exception type
    names saying where a look stopped, so an empty answer is never blind."""
    result = json.loads(stdout)
    if not isinstance(result, dict) or type(result.get("pressed")) is not bool or not isinstance(result.get("seen"), list):
        raise RuntimeError("accessibility helper returned an unexpected answer")
    probe = result.get("probe")
    if probe is not None and not isinstance(probe, dict):
        raise RuntimeError("accessibility helper returned an unexpected probe")
    return result["pressed"], [str(item) for item in result["seen"]], probe


def instances(appimage, proc=Path("/proc")):
    """Processes started from this AppImage file: the AppImage runtime sets
    APPIMAGE to its path, and the updater replaces the file it names."""
    wanted = f"APPIMAGE={appimage}".encode()
    found = set()
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            if wanted in (entry / "environ").read_bytes().split(b"\0"):
                found.add(int(entry.name))
        except OSError:
            continue
    return found


def processes_under(ps_output, prefix):
    """macOS: processes whose command starts with an executable inside the bundle."""
    found = set()
    for line in ps_output.splitlines():
        pid, _, command = line.strip().partition(" ")
        if pid.isdigit() and command.lstrip().startswith(prefix):
            found.add(int(pid))
    return found


def bundle_executable(bundle):
    with open(bundle / "Contents/Info.plist", "rb") as file:
        info = plistlib.load(file)
    return bundle / "Contents/MacOS" / info["CFBundleExecutable"], info.get("CFBundleShortVersionString")


def only_app(folder):
    found = [entry for entry in folder.iterdir() if entry.suffix == ".app"]
    if len(found) != 1:
        raise AssertionError(f"expected one .app in the update archive, found {len(found)}")
    return found[0]


def new_entries(before, after):
    return sorted(set(after) - set(before))


def app_executable(folder):
    """Windows: the app beside the uninstaller in the install folder."""
    found = sorted(entry for entry in folder.iterdir()
                   if entry.suffix.lower() == ".exe" and not entry.name.lower().startswith("uninstall"))
    if not found:
        raise AssertionError(f"the setup executable installed no app in {folder}")
    return found[0]


def wait_for(predicate, timeout, message, step=1.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(step)
    raise AssertionError(message)


def feed_version(url="https://github.com/ibiscite/ibis/releases/download/nightly/latest.json"):
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            return json.load(response).get("version")
    except Exception as error:  # Only for the failure report.
        return f"unavailable ({type(error).__name__})"


class Linux:
    """The AppImage is copied into the test folder; the updater replaces that file."""

    def __init__(self, folder, args):
        import pyatspi
        self.atspi = pyatspi
        self.config = folder / "config"
        self.settings = seed_settings(self.config)
        self.appimage = folder / "Ibis Nightly.AppImage"
        shutil.copyfile(args.previous, self.appimage)
        self.appimage.chmod(0o755)
        self.before, self.expected = digest(self.appimage), digest(args.expected)
        if self.before == self.expected:
            raise AssertionError("the previous and the new AppImage are identical")
        self.command = [str(self.appimage)]

    def instances(self):
        return instances(self.appimage)

    def updated(self):
        return self.appimage.exists() and digest(self.appimage) == self.expected

    def facts(self):
        return {"appimage_matches_new_package": self.updated()}

    def look(self, pids, press=None):
        seen, visited = [], 0
        for app in self.atspi.Registry.getDesktop(0):
            if not app:
                continue
            stack = [app]
            while stack and visited < 20000:
                item = stack.pop()
                visited += 1
                try:
                    role = item.getRoleName()
                    if press and role == "push button" and item.name == press:
                        item.queryAction().doAction(0)
                        return True, [], None
                    if role in ("frame", "dialog", "alert", "push button", "label") and item.name and len(seen) < 200:
                        seen.append(f"{app.name}[{app.get_process_id()}] {role}: {item.name[:200]}")
                    stack.extend(child for child in item if child)
                except Exception:  # Accessibility objects vanish while windows change.
                    continue
        return False, seen, None

    def close(self):
        pass


class MacOS:
    """The .app from the update archive in a private Applications folder; the
    updater replaces the bundle there and reopens it with LaunchServices."""

    def __init__(self, folder, args):
        # Resolved, because LaunchServices reopens /private/var/..., not /var/...
        folder = folder.resolve()
        self.config = None
        self.settings = claim_settings(Path.home() / "Library/Application Support")
        applications, expected = folder / "Applications", folder / "expected"
        for target, archive in ((applications, args.previous), (expected, args.expected)):
            target.mkdir()
            subprocess.run(["tar", "-xzf", str(archive.resolve()), "-C", str(target)], check=True)
        self.bundle = only_app(applications)
        executable, self.previous_version = bundle_executable(self.bundle)
        new_executable, self.expected_version = bundle_executable(only_app(expected))
        self.expected = digest(new_executable)
        if digest(executable) == self.expected:
            raise AssertionError("the previous and the new app are identical")
        self.command = [str(executable)]

    def instances(self):
        ps = subprocess.run(["ps", "-axww", "-o", "pid=,args="], capture_output=True, text=True, check=True)
        return processes_under(ps.stdout, f"{self.bundle}/Contents/MacOS/")

    def installed(self):
        try:
            return bundle_executable(self.bundle)
        except (OSError, KeyError, plistlib.InvalidFileException):
            return None, None  # Mid-replacement.

    def updated(self):
        executable, version = self.installed()
        return (executable is not None and version == self.expected_version and executable.exists()
                and digest(executable) == self.expected)

    def facts(self):
        return {"previous_version": self.previous_version, "expected_version": self.expected_version,
                "installed_version": self.installed()[1], "executable_matches_new_package": self.updated()}

    def look(self, pids, press=None):
        result = subprocess.run(["osascript", "-l", "JavaScript", "-e", MACOS_LOOK, press or "", *map(str, pids)],
                                capture_output=True, timeout=60)
        return look_result(helper_stdout(result, "System Events"))

    def close(self):
        clean_settings(self.settings)


class WindowsActivationFailure(AssertionError):
    def __init__(self, probe):
        super().__init__("Windows update activation failed or its outcome is unknown")
        flags = {"attempted", "blocked", "invoke_available", "enabled", "on_screen"}
        counts = {"pids", "windows", "elements", "offer_matches", "action_matches"}
        self.probe = {key: value for key, value in (probe.items() if isinstance(probe, dict) else [])
                      if (key in flags and type(value) is bool)
                      or (key in counts and type(value) is int and 0 <= value <= 2147483647)}


class Windows:
    """Installed for the current user by the setup executable; the updater
    runs the new setup executable, which starts the app again."""

    def __init__(self, folder, args):
        if not args.version:
            raise SystemExit("--version is required on Windows")
        self.version = args.version
        self.config = None
        roaming = powershell("[Environment]::GetFolderPath('ApplicationData')").strip()
        self.settings = claim_settings(Path(roaming))
        if digest(args.previous) == digest(args.expected):
            raise AssertionError("the previous and the new setup executable are identical")
        before = uninstall_entries()
        subprocess.run([str(args.previous.resolve()), "/S"], check=True, timeout=300)
        added = new_entries(before, uninstall_entries())
        if len(added) != 1:
            raise AssertionError(f"the setup executable registered {len(added)} uninstall entries")
        self.key = added[0]
        uninstaller = registry_value(self.key, "UninstallString").strip('"')
        self.folder = Path(uninstaller).parent
        self.executable = app_executable(self.folder)
        self.previous_version = registry_value(self.key, "DisplayVersion")
        self.before = digest(self.executable)
        self.command = [str(self.executable)]

    def instances(self):
        out = powershell("ConvertTo-Json -Compress -InputObject @(Get-CimInstance Win32_Process | "
                         "Where-Object ExecutablePath -eq $env:IBIS_EXE | ForEach-Object { [int]$_.ProcessId })",
                         IBIS_EXE=str(self.executable))
        return set(json.loads(out or "[]"))

    def installed_version(self):
        try:
            return registry_value(self.key, "DisplayVersion")
        except OSError:
            return None

    def updated(self):
        try:
            changed = digest(self.executable) != self.before
        except OSError:
            return False  # Being replaced.
        return changed and self.installed_version() == self.version

    def facts(self):
        try:
            changed = digest(self.executable) != self.before
        except OSError:
            changed = None
        return {"previous_version": self.previous_version, "expected_version": self.version,
                "installed_version": self.installed_version(), "executable_changed": changed}

    def look(self, pids, press=None):
        probe = None
        try:
            result = look_result(powershell(WINDOWS_LOOK, IBIS_PIDS=",".join(map(str, pids)), IBIS_PRESS=press or ""))
            pressed, _, probe = result
            if press:
                flags = ("attempted", "blocked", "invoke_available", "enabled", "on_screen")
                limits = {"pids": 20, "windows": 20, "elements": 4000, "offer_matches": 1, "action_matches": 1}
                valid = (isinstance(probe, dict) and all(type(probe.get(key)) is bool for key in flags)
                         and all(type(probe.get(key)) is int and 0 <= probe[key] <= limit for key, limit in limits.items())
                         and probe.get("errors") == [] and not probe["blocked"] and probe["attempted"] == pressed)
                if not valid or (pressed and (not all(probe[key] for key in ("invoke_available", "enabled", "on_screen"))
                                              or any(probe[key] < 1 for key in limits) or probe.get("pressed_by") != "invoke")) \
                        or (not pressed and (probe["action_matches"] != 0 or probe.get("pressed_by") is not None)):
                    raise RuntimeError("Windows update action metadata is unavailable or blocked")
            return result
        except (RuntimeError, ValueError, subprocess.TimeoutExpired):
            if press:
                # offered() retries those helper errors; AssertionError instead
                # ends qualification when an activation may have happened.
                raise WindowsActivationFailure(probe) from None
            raise

    def close(self):
        uninstaller = self.folder / "uninstall.exe"
        if uninstaller.exists():
            subprocess.run([str(uninstaller), "/S"], timeout=120)
        clean_settings(self.settings)


def helper_stdout(result, name):
    """Decode in this thread: Windows text-pipe readers can lose decode errors."""
    stdout = result.stdout.decode("utf-8", errors="strict")
    stderr = result.stderr.decode("utf-8", errors="strict")
    if result.returncode:
        raise RuntimeError(f"{name} helper failed: {stderr.strip()[-500:]}")
    return stdout


def powershell(script, **env):
    """Runs a script from a file: commands read from standard input end at the
    first line break inside a block. Values arrive as environment variables."""
    with tempfile.TemporaryDirectory(prefix="ibis update helper ") as folder:
        path = Path(folder) / "helper.ps1"
        # JSON and paths must survive Windows' non-UTF-8 default code page.
        path.write_text("[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)\n" + script, encoding="utf-8")
        result = subprocess.run(["pwsh", "-NoProfile", "-NonInteractive", "-File", str(path)],
                                capture_output=True, timeout=120, env=dict(os.environ, **env))
    return helper_stdout(result, "PowerShell")


def uninstall_entries():
    import winreg
    try:
        root = winreg.OpenKey(winreg.HKEY_CURRENT_USER, UNINSTALL)
    except FileNotFoundError:
        return []  # A fresh runner account may have no per-user installations.
    with root:
        return [winreg.EnumKey(root, index) for index in range(winreg.QueryInfoKey(root)[0])]


def registry_value(key, name):
    import winreg
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, f"{UNINSTALL}\\{key}") as entry:
        return winreg.QueryValueEx(entry, name)[0]


SYSTEMS = {"Linux": Linux, "Darwin": MacOS, "Windows": Windows}


def run(args):
    report = {"status": "failed", "system": platform.system(), "steps": []}
    started = []
    log = None
    install = None
    try:
        with tempfile.TemporaryDirectory(prefix="ibis update test ", ignore_cleanup_errors=True) as temporary:
            folder = Path(temporary)
            library = folder / "synthetic.bib"
            library.write_text("@article{update1, title = {Synthetic update test}, year = {2026}}\n", encoding="utf-8")
            seen = {"helper": None, "probe": None}
            try:
                install = SYSTEMS[platform.system()](folder, args)
                report["steps"].append("previous nightly installed")

                log = (args.output / "update-e2e-app.log").open("w", encoding="utf-8")
                app = subprocess.Popen(install.command, env=environment(os.environ, install.config, library),
                                       stdout=log, stderr=log, cwd=folder)
                started.append(app)
                report["steps"].append("previous nightly started")

                # The processes running just before the press, so helper
                # processes the old version starts later never count as new.
                seen["old"] = set()
                def offered():
                    seen["old"] = install.instances()
                    try:
                        pressed, _, seen["probe"] = install.look(seen["old"], BUTTON)
                        return pressed
                    except (RuntimeError, ValueError, subprocess.TimeoutExpired) as problem:
                        seen["helper"] = str(problem)  # Windows change while being read.
                        return False
                wait_for(offered, OFFER_TIMEOUT, "the previous nightly never offered the update", step=2)
                old = seen["old"]
                report["steps"].append("update offered and Install and Restart pressed")

                wait_for(install.updated, REPLACE_TIMEOUT, "the installed app was not replaced by the new version", step=2)
                report["steps"].append("installed app replaced by the new version")

                def restarted():
                    fresh = install.instances() - old
                    return fresh if fresh else None
                fresh = wait_for(restarted, RESTART_TIMEOUT, "the new version did not start after the update")
                time.sleep(5)
                if not install.instances() & fresh:
                    raise AssertionError("the new version exited right after the update")
                report["steps"].append("new version started and stayed up")
                report["status"] = "passed"
            except Exception as error:
                report["error"] = str(error) if isinstance(error, AssertionError) else f"{type(error).__name__}: {error}"
                if isinstance(error, WindowsActivationFailure) and error.probe:
                    seen["probe"] = error.probe
                if seen["helper"]:
                    report["last_helper_problem"] = seen["helper"]
                if seen["probe"] is not None:
                    report["last_probe"] = seen["probe"]  # From the last look that answered.
                if install:
                    report.update(install.facts())
                    try:
                        pids = install.instances()
                        report["instances"] = len(pids)
                        _, report["on_screen"], report["on_screen_probe"] = install.look(pids)
                    except Exception as problem:
                        report["on_screen"] = f"unavailable ({problem})"
                report["published_feed"] = feed_version()
                report["running"] = [app.poll() is None for app in started]
            finally:
                for app in started:
                    if app.poll() is None:
                        app.kill()
                if install:
                    try:
                        for pid in install.instances():
                            try:
                                os.kill(pid, 15)
                            except OSError:
                                pass  # Already gone.
                        install.close()
                    except Exception as problem:  # Cleanup never hides the result.
                        print(f"cleanup incomplete: {type(problem).__name__}: {problem}", file=sys.stderr)
                if log:
                    log.close()
    finally:
        (args.output / "update-e2e.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    if report["status"] != "passed":
        log_path = args.output / "update-e2e-app.log"
        if log_path.exists():
            print("App output (last 60 lines):")
            print("\n".join(log_path.read_text(encoding="utf-8", errors="replace").splitlines()[-60:]))
    return 0 if report["status"] == "passed" else 1


class Tests(unittest.TestCase):
    @staticmethod
    def windows_probe(pressed=False):
        return {"attempted": pressed, "blocked": False, "pids": 1, "windows": int(pressed), "elements": int(pressed),
                "offer_matches": int(pressed), "action_matches": int(pressed), "invoke_available": pressed,
                "enabled": pressed, "on_screen": pressed, "errors": [], "pressed_by": "invoke" if pressed else None}

    def test_windows_success_requires_complete_uncoerced_qualification_facts(self):
        for field, value in (("offer_matches", True), ("action_matches", "1"), ("enabled", 1), ("on_screen", None),
                             ("invoke_available", False), ("windows", 0), ("pressed_by", None), ("errors", ["synthetic"])):
            probe = self.windows_probe(True)
            probe[field] = value
            with self.subTest(field=field), patch(__name__ + ".powershell", return_value=json.dumps({"pressed": True, "seen": [], "probe": probe})):
                with self.assertRaises(AssertionError):
                    Windows.__new__(Windows).look({42}, BUTTON)
        with patch(__name__ + ".powershell", return_value='{"pressed":true,"seen":[],"probe":{"attempted":true}}'):
            with self.assertRaises(AssertionError):
                Windows.__new__(Windows).look({42}, BUTTON)

    def test_windows_fatal_activation_keeps_only_fixed_failure_facts(self):
        probe = {"attempted": True, "blocked": True, "invoke_available": False, "action_matches": 1,
                 "private": "synthetic payload", "enabled": "synthetic payload", "elements": -1}
        with patch(__name__ + ".powershell", return_value=json.dumps({"pressed": False, "seen": [], "probe": probe})):
            with self.assertRaises(AssertionError) as failure:
                Windows.__new__(Windows).look({42}, BUTTON)
        self.assertEqual(getattr(failure.exception, "probe", None),
                         {"attempted": True, "blocked": True, "invoke_available": False, "action_matches": 1})

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell is required for the Windows update activation contract")
    def test_windows_offer_uses_exact_ownership_uniqueness_and_invocation_capability(self):
        # Execute the actual embedded helper with private provider models.
        # Never shadow System.* types: Windows may bind its real assemblies.
        model = r'''
param([string]$Scenario)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
namespace IbisUpdateContract {
 public enum TreeScope { Children, Descendants }
 public class Condition { public static object TrueCondition=new object(); }
 public class PropertyCondition { public object Value; public PropertyCondition(object key,object value) {Value=value;} }
 public class AndCondition { public AndCondition(object a,object b) {} }
 public class ControlType {
  public string ProgrammaticName; public ControlType(string name) {ProgrammaticName="ControlType."+name;}
  public static ControlType Button=new ControlType("Button"), Pane=new ControlType("Pane");
 }
 public class InvokePattern { public static object Pattern=new object(); }
 public class AutomationElement {
  public static object ProcessIdProperty=new object(), ControlTypeProperty=new object(), NameProperty=new object();
  public static Node RootElement=new Node();
 }
 public class Node {
  public string Name="", Type="Pane", AutomationId=""; public int ProcessId=42; public bool IsEnabled=true, IsOffscreen=false;
  public bool Supported=true, ThrowOnRead=false, Fail=false; public int Invocations=0;
  public Node[] Nodes=new Node[0];
  public Node Current { get {if(ThrowOnRead && Invocations>0) throw new Exception("synthetic stale element"); return this;} }
  public ControlType ControlType { get {return Type=="Button" ? IbisUpdateContract.ControlType.Button : IbisUpdateContract.ControlType.Pane;} }
  public Node[] FindAll(TreeScope scope,object condition) {return Nodes;}
  public Node FindFirst(TreeScope scope,object condition) {return Nodes.Length>0 ? Nodes[0] : null;}
  public bool TryGetCurrentPattern(object pattern,ref object value) {value=Supported && pattern==InvokePattern.Pattern ? this : null; return value!=null;}
  public object GetCurrentPattern(object pattern) {if(!Supported) throw new Exception("unsupported"); return this;}
  public void Invoke() {Invocations++; if(Fail) throw new Exception("synthetic uncertain invocation");}
 }
}
'@
$window=[IbisUpdateContract.Node]::new(); $window.Name='An update is available'
$item=[IbisUpdateContract.Node]::new(); $item.Name='Install and Restart'; $item.ThrowOnRead=$true
$window.Nodes=@($item); [IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($window)
switch($Scenario) {
 'button' {$item.Type='Button'}
 'standard' {$item.Type='Button'; $item.Name='Yes'; $item.AutomationId='6'}
 'standard-localized' {$item.Type='Button'; $item.Name='はい'; $item.AutomationId='6'}
 'standard-rtl' {$item.Type='Button'; $item.Name='نعم'; $item.AutomationId='6'}
 'standard-no' {$item.Type='Button'; $item.Name='No'; $item.AutomationId='7'}
 'standard-wrong-id' {$item.Type='Button'; $item.Name='Yes'; $item.AutomationId='06'}
 'standard-pane' {$item.Name='Yes'; $item.AutomationId='6'}
 'standard-disabled' {$item.Type='Button'; $item.Name='Yes'; $item.AutomationId='6'; $item.IsEnabled=$false}
 'standard-hidden' {$item.Type='Button'; $item.Name='Yes'; $item.AutomationId='6'; $item.IsOffscreen=$true}
 'standard-uncertain' {$item.Type='Button'; $item.Name='Yes'; $item.AutomationId='6'; $item.Fail=$true}
 'standard-unsupported' {$item.Type='Button'; $item.Name='Yes'; $item.AutomationId='6'; $item.Supported=$false}
 'standard-duplicate' {$item.Type='Button'; $item.Name='Yes'; $item.AutomationId='6'; $window.Nodes=@($item,$item)}
 'standard-wrong-owner' {$item.Type='Button'; $item.Name='Yes'; $item.AutomationId='6'; $item.ProcessId=43}
 'mixed-actions' {$standard=[IbisUpdateContract.Node]::new(); $standard.Type='Button'; $standard.Name='Yes'; $standard.AutomationId='6'; $window.Nodes=@($item,$standard)}
 'wrong-owner' {$item.ProcessId=43}
 'wrong-window' {$window.Name='Other dialog'}
 'wrong-name' {$item.Name='Install and Restart suffix'}
 'disabled' {$item.IsEnabled=$false}
 'hidden' {$item.IsOffscreen=$true}
 'unsupported' {$item.Supported=$false}
 'duplicate-action' {$window.Nodes=@($item,$item)}
 'duplicate-window' {[IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($window,$window)}
 'missing-window' {[IbisUpdateContract.AutomationElement]::RootElement.Nodes=@()}
 'wrong-window-owner' {$window.ProcessId=43}
 'window-bound' {[IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($window)*21}
 'case-name' {$item.Name='install and restart'}
 'case-window' {$window.Name='an update is available'}
 'bound' {$window.Nodes=@($item)*4001}
 'uncertain' {$item.Fail=$true}
}
$env:IBIS_PIDS='42'; $env:IBIS_PRESS='Install and Restart'
function Contract-Result($pressed,$how) {
 $expected=$Scenario -in @('pane','button','standard','standard-localized','standard-rtl')
 if($pressed -ne $expected -or $item.Invocations -ne [int]($expected -or $Scenario -in @('uncertain','standard-uncertain'))) {throw 'Unsafe or missing activation'}
 if($probe.attempted -ne ($expected -or $Scenario -in @('uncertain','standard-uncertain'))) {throw 'Incorrect attempt metadata'}
 if($probe.blocked -ne ($Scenario -notin @('pane','button','standard','standard-localized','standard-rtl','wrong-window','wrong-name','case-name','case-window','missing-window','standard-no','standard-wrong-id','standard-pane'))) {throw 'Unsafe observation can poll'}
 if($expected -and $how -ne 'invoke') {throw 'Incorrect invocation metadata'}
}
'''
        # Assert activation counters, retaining real serialization for the caller.
        helper = WINDOWS_LOOK.replace("Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes", "")
        helper = helper.replace("System.Windows.Automation.", "IbisUpdateContract.")
        helper = helper.replace("function Answer($pressed, $how) {",
                                "function Answer($pressed, $how) {\n  Contract-Result $pressed $how")
        with tempfile.TemporaryDirectory() as temporary:
            script = Path(temporary) / "update-contract.ps1"
            script.write_text(model + helper, encoding="utf-8")
            for scenario in ("pane", "button", "standard", "standard-localized", "standard-rtl", "standard-no", "standard-wrong-id", "standard-pane",
                             "standard-disabled", "standard-hidden", "standard-uncertain", "standard-unsupported", "standard-duplicate",
                             "standard-wrong-owner", "mixed-actions", "wrong-owner", "wrong-window", "wrong-name", "disabled", "hidden",
                             "unsupported", "duplicate-action", "duplicate-window", "missing-window", "wrong-window-owner",
                             "window-bound", "case-name", "case-window", "bound", "uncertain"):
                with self.subTest(scenario=scenario):
                    expected = scenario in ("pane", "button", "standard", "standard-localized", "standard-rtl")
                    result = subprocess.run([shutil.which("pwsh"), "-NoProfile", "-File", str(script), scenario],
                                            capture_output=True, timeout=30)
                    self.assertEqual(result.returncode, 0, (result.stdout + result.stderr).decode("utf-8", errors="replace"))
                    payload = result.stdout.decode("utf-8")
                    pressed, _, probe = look_result(payload)
                    with patch(__name__ + ".powershell", return_value=payload):
                        if probe["blocked"]:
                            with self.assertRaises(WindowsActivationFailure):
                                Windows.__new__(Windows).look({42}, BUTTON)
                        else:
                            self.assertEqual(Windows.__new__(Windows).look({42}, BUTTON)[0], expected)
                    self.assertEqual(pressed, expected)

    def test_windows_activation_uncertainty_is_fatal_instead_of_pollable(self):
        windows = Windows.__new__(Windows)
        for problem in (RuntimeError("synthetic helper failure"), ValueError("synthetic malformed JSON"),
                        UnicodeDecodeError("utf-8", b"\xff", 0, 1, "synthetic"),
                        subprocess.TimeoutExpired("synthetic", 120)):
            with self.subTest(problem=type(problem).__name__), patch(__name__ + ".powershell", side_effect=problem) as helper:
                with self.assertRaises(AssertionError):  # offered() does not retry AssertionError.
                    windows.look({42}, BUTTON)
                self.assertEqual(helper.call_count, 1)
        for probe in (None, {}, {"attempted": True}, {"attempted": "false"}, {"attempted": False, "blocked": True}):
            payload = json.dumps({"pressed": False, "seen": [], "probe": probe})
            with self.subTest(probe=probe), patch(__name__ + ".powershell", return_value=payload):
                with self.assertRaises(AssertionError):
                    windows.look({42}, BUTTON)

    def test_uncertain_windows_press_is_not_retried_by_the_offer_loop(self):
        windows = Windows.__new__(Windows)
        windows.command, windows.config = ["synthetic"], None
        windows.instances, windows.facts, windows.close = MagicMock(return_value={42}), MagicMock(return_value={}), MagicMock()
        def two_polls(predicate, _timeout, message, **_kwargs):
            for _ in range(2):
                if predicate():
                    return True
            raise AssertionError(message)
        for problem in (RuntimeError("synthetic"), ValueError("synthetic"), subprocess.TimeoutExpired("synthetic", 120)):
            with self.subTest(problem=type(problem).__name__), tempfile.TemporaryDirectory() as temporary, \
                    patch.dict(SYSTEMS, {"Windows": lambda *_args: windows}), patch("platform.system", return_value="Windows"), \
                    patch(__name__ + ".powershell", side_effect=problem) as helper, \
                    patch(__name__ + ".wait_for", side_effect=two_polls), patch(__name__ + ".feed_version", return_value="synthetic"), \
                    patch("subprocess.Popen"), patch("os.kill"), patch("builtins.print"):
                args = argparse.Namespace(output=Path(temporary))
                self.assertEqual(run(args), 1)
                self.assertEqual(sum(bool(call.kwargs.get("IBIS_PRESS")) for call in helper.call_args_list), 1)

    def test_windows_absence_can_poll_but_success_returns_the_single_attempt(self):
        windows = Windows.__new__(Windows)
        for pressed in (False, True):
            probe = self.windows_probe(pressed)
            payload = json.dumps({"pressed": pressed, "seen": [], "probe": probe})
            with patch(__name__ + ".powershell", return_value=payload):
                self.assertEqual(windows.look({42}, BUTTON), (pressed, [], probe))

    def test_settings_allow_update_checks_in_the_nightly_folder(self):
        with tempfile.TemporaryDirectory() as temporary:
            settings = seed_settings(Path(temporary))
            self.assertEqual(settings, Path(temporary) / "io.github.ibiscite.ibis.nightly/settings.json")
            self.assertEqual(json.loads(settings.read_text(encoding="utf-8")), {"schema_version": 2, "update_checks": True})

    def test_claim_never_overwrites_existing_settings_and_clean_removes_only_its_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            settings = claim_settings(root)
            (settings.parent / "settings.json.bak").write_text("{}", encoding="utf-8")
            (settings.parent / "other.json").write_text("{}", encoding="utf-8")
            with self.assertRaises(SystemExit):
                claim_settings(root)
            clean_settings(settings)
            self.assertEqual([entry.name for entry in settings.parent.iterdir()], ["other.json"])

    @unittest.skipIf(os.name == "nt", "creating symlinks needs extra rights on Windows")
    def test_claim_refuses_a_dangling_link_and_leaves_it_and_its_target_alone(self):
        for name in ("settings.json", "settings.json.bak", "settings.json.tmp"):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                folder = root / "config" / IDENTIFIER
                folder.mkdir(parents=True)
                target = root / "elsewhere.json"
                (folder / name).symlink_to(target)
                with self.assertRaises(SystemExit):
                    claim_settings(root / "config")
                self.assertTrue((folder / name).is_symlink())
                self.assertFalse(target.exists())

    def test_macos_update_needs_the_new_version_as_well_as_the_new_executable(self):
        with tempfile.TemporaryDirectory() as temporary:
            bundle = Path(temporary) / "Ibis Nightly.app"
            (bundle / "Contents/MacOS").mkdir(parents=True)
            (bundle / "Contents/MacOS/ibis-desktop").write_bytes(b"new")
            mac = MacOS.__new__(MacOS)
            mac.bundle, mac.expected, mac.expected_version = bundle, digest(bundle / "Contents/MacOS/ibis-desktop"), "new-version"
            for version, updated in (("old-version", False), ("new-version", True)):
                with open(bundle / "Contents/Info.plist", "wb") as file:
                    plistlib.dump({"CFBundleExecutable": "ibis-desktop", "CFBundleShortVersionString": version}, file)
                self.assertEqual(mac.updated(), updated)

    def test_environment_drops_an_outer_appimage(self):
        env = environment({"APPIMAGE": "/outer", "PATH": "/bin"}, Path("/c"), Path("/l.bib"))
        self.assertNotIn("APPIMAGE", env)
        self.assertEqual((env["XDG_CONFIG_HOME"], env["PATH"]), (str(Path("/c")), "/bin"))

    def test_environment_without_a_config_folder_sets_only_the_library(self):
        env = environment({"PATH": "/bin", "IBIS_GALLERY": "1"}, None, Path("/l.bib"))
        self.assertEqual(env, {"PATH": "/bin", "IBIS_LIBRARY": str(Path("/l.bib"))})

    @unittest.skipIf(os.name == "nt", "AppImages and /proc exist only on Linux")
    def test_instances_match_the_exact_appimage_path(self):
        with tempfile.TemporaryDirectory() as temporary:
            proc = Path(temporary)
            for pid, value in (("10", "/t/Ibis Nightly.AppImage"), ("11", "/t/Ibis Nightly.AppImage.old"), ("12", None)):
                (proc / pid).mkdir()
                environ = b"PATH=/bin\0" + (f"APPIMAGE={value}".encode() if value else b"")
                (proc / pid / "environ").write_bytes(environ)
            (proc / "self").mkdir()
            self.assertEqual(instances(Path("/t/Ibis Nightly.AppImage"), proc), {10})

    def test_macos_processes_match_only_executables_inside_the_bundle(self):
        ps = ("  101 /private/var/t/Applications/Ibis Nightly.app/Contents/MacOS/ibis-desktop -psn_0\n"
              "  102 /private/var/t/Applications/Ibis Nightly.app.bak/Contents/MacOS/ibis-desktop\n"
              "  103 /usr/bin/open -n /private/var/t/Applications/Ibis Nightly.app\n"
              "  104 /private/var/t/Applications/Ibis Nightly.app/Contents/MacOS/renamed\n")
        prefix = "/private/var/t/Applications/Ibis Nightly.app/Contents/MacOS/"
        self.assertEqual(processes_under(ps, prefix), {101, 104})

    def test_macos_bundle_executable_comes_from_info_plist(self):
        with tempfile.TemporaryDirectory() as temporary:
            bundle = Path(temporary) / "Ibis Nightly.app"
            (bundle / "Contents").mkdir(parents=True)
            with open(bundle / "Contents/Info.plist", "wb") as file:
                plistlib.dump({"CFBundleExecutable": "ibis-desktop", "CFBundleShortVersionString": "0.1.1-nightly.1"}, file)
            self.assertEqual(bundle_executable(bundle), (bundle / "Contents/MacOS/ibis-desktop", "0.1.1-nightly.1"))
            self.assertEqual(only_app(Path(temporary)), bundle)
            (Path(temporary) / "Other.app").mkdir()
            with self.assertRaises(AssertionError):
                only_app(Path(temporary))

    def test_windows_install_is_found_from_the_new_uninstall_entry(self):
        self.assertEqual(new_entries(["Ibis", "Other"], ["Other", "Ibis", "Ibis Nightly"]), ["Ibis Nightly"])
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            for name in ("uninstall.exe", "ibis-desktop.exe", "resources.pak"):
                (folder / name).write_bytes(b"")
            self.assertEqual(app_executable(folder), folder / "ibis-desktop.exe")
            (folder / "ibis-desktop.exe").unlink()
            with self.assertRaises(AssertionError):
                app_executable(folder)

    def test_windows_install_can_start_without_an_uninstall_registry_parent(self):
        registry = MagicMock()
        key = MagicMock()
        key.__enter__.return_value = key
        registry.OpenKey.side_effect = [FileNotFoundError(), key]
        registry.QueryInfoKey.return_value = (1, 0, 0)
        registry.EnumKey.return_value = "Ibis Nightly"
        with patch.dict(sys.modules, {"winreg": registry}):
            before = uninstall_entries()
            after = uninstall_entries()
        self.assertEqual(before, [])
        self.assertEqual(new_entries(before, after), ["Ibis Nightly"])
        registry.EnumKey.assert_called_once_with(key.__enter__.return_value, 0)

    def test_windows_registry_inventory_preserves_access_and_enumeration_errors(self):
        registry = MagicMock()
        with patch.dict(sys.modules, {"winreg": registry}):
            registry.OpenKey.side_effect = PermissionError("synthetic registry denial")
            with self.assertRaises(PermissionError):
                uninstall_entries()
            registry.OpenKey.side_effect = None
            registry.QueryInfoKey.return_value = (1, 0, 0)
            registry.EnumKey.side_effect = FileNotFoundError("synthetic vanished entry")
            with self.assertRaises(FileNotFoundError):
                uninstall_entries()

    def test_helper_answers_are_checked(self):
        self.assertEqual(look_result('{"pressed": false, "seen": ["[1] button: Later"]}'), (False, ["[1] button: Later"], None))
        self.assertEqual(look_result('{"pressed": true, "seen": []}'), (True, [], None))
        probe = {"pids": 1, "processes": 1, "windows": 0, "errors": ["[1] -25211"]}
        self.assertEqual(look_result(json.dumps({"pressed": False, "seen": [], "probe": probe})), (False, [], probe))
        for bad in ('{"pressed": "yes", "seen": []}', '{"seen": []}', '[]', '{"pressed": true, "seen": "x"}',
                    '{"pressed": false, "seen": [], "probe": "x"}'):
            with self.assertRaises(RuntimeError):
                look_result(bad)

    def test_native_helpers_preserve_utf8_when_the_default_encoding_is_cp1252(self):
        expected = "Synthetic 合成 한국어 \u0081"
        payload = json.dumps({"pressed": False, "seen": [expected]}, ensure_ascii=False).encode("utf-8")
        run_child = subprocess.run
        command = [sys.executable, "-c", f"import sys; sys.stdout.buffer.write(bytes.fromhex('{payload.hex()}'))"]
        mac = MacOS.__new__(MacOS)
        for helper in (lambda: look_result(powershell("Write-Output 'synthetic'")), lambda: mac.look([123])):
            with self.subTest(helper=helper), patch("locale.getencoding", return_value="cp1252"), \
                    patch("subprocess.run", side_effect=lambda _command, **kwargs: run_child(command, **kwargs)):
                self.assertEqual(helper(), (False, [expected], None))

    def test_native_helpers_decode_binary_pipes_in_the_caller(self):
        payload = json.dumps({"pressed": False, "seen": ["Synthetic 合成"]}, ensure_ascii=False).encode("utf-8")
        mac = MacOS.__new__(MacOS)
        for helper in (lambda: look_result(powershell("synthetic")), lambda: mac.look([123])):
            def binary_result(command, **kwargs):
                # Windows decodes text pipes on reader threads, where errors
                # can be lost. Only bytes may cross that thread boundary.
                self.assertTrue(kwargs["capture_output"])
                self.assertFalse(kwargs.get("text", False))
                self.assertIsNone(kwargs.get("encoding"))
                self.assertIsNone(kwargs.get("errors"))
                return subprocess.CompletedProcess(command, 0, stdout=payload, stderr=b"")
            with self.subTest(helper=helper), patch("subprocess.run", side_effect=binary_result):
                self.assertEqual(helper(), (False, ["Synthetic 合成"], None))

    def test_native_helper_failure_keeps_decoded_utf8_diagnostics(self):
        mac = MacOS.__new__(MacOS)
        for helper in (lambda: powershell("synthetic"), lambda: mac.look([123])):
            failed = subprocess.CompletedProcess([], 1, stdout=b"", stderr="Synthetic 合成".encode("utf-8"))
            with self.subTest(helper=helper), patch("subprocess.run", return_value=failed):
                with self.assertRaisesRegex(RuntimeError, "helper failed: Synthetic 合成"):
                    helper()

    def test_powershell_sets_utf8_before_running_the_script(self):
        def inspect_script(command, **_kwargs):
            script = Path(command[-1]).read_text(encoding="utf-8")
            self.assertTrue(script.startswith("[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)\n"))
            self.assertTrue(script.endswith("Write-Output '合成'"))
            return subprocess.CompletedProcess(command, 0, stdout="合成".encode("utf-8"), stderr=b"")
        with patch("subprocess.run", side_effect=inspect_script):
            self.assertEqual(powershell("Write-Output '合成'"), "合成")

    def test_malformed_native_helper_utf8_is_a_failure(self):
        run_child = subprocess.run
        command = [sys.executable, "-c", "import sys; sys.stdout.buffer.write(b'\\xff')"]
        mac = MacOS.__new__(MacOS)
        for helper in (lambda: powershell("synthetic"), lambda: mac.look([123])):
            with self.subTest(helper=helper), patch("subprocess.run", side_effect=lambda _command, **kwargs: run_child(command, **kwargs)):
                with self.assertRaises(UnicodeDecodeError):
                    helper()


def main():
    if sys.argv[1:] == ["--self-test"]:
        unittest.main(argv=[sys.argv[0]], verbosity=2)
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--previous", type=Path, required=True, help="Package of the nightly being replaced")
    parser.add_argument("--expected", type=Path, required=True, help="Package of the new nightly")
    parser.add_argument("--version", help="Version of the new nightly (required on Windows)")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    sys.exit(run(args))


if __name__ == "__main__":
    main()
