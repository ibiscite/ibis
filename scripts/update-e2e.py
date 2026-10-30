#!/usr/bin/env python3
"""End-to-end update test for Ibis Nightly (#319, #493).

Installs the previous Ibis Nightly with automatic update checks on, waits for
it to offer the newly published nightly, accepts the install/restart action through
the system's accessibility interface and checks that the installed app was
replaced by the new version and that the new version started in its place.
The seeded synthetic library and shortcut must remain in its settings.

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
import stat
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import MagicMock, patch
import urllib.request
from pathlib import Path
# Keep sibling helpers available to direct execution and importlib/runpy callers.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from native_test_settings import clean_settings, settings_exist

IDENTIFIER = "com.ibiscite.ibis.nightly"
SHORTCUT = "Control+Alt+Y"
BUTTON = "Install and Restart"
# The app's first check comes 30 seconds after start; the download is ~100 MB.
OFFER_TIMEOUT = 180
REPLACE_TIMEOUT = 300
RESTART_TIMEOUT = 60
PICKER_TIMEOUT = 30
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
              try { button.click(); probe.pressed_by = 'click'; return answer(); }
              catch (e) { throw Error('Update activation failed or its outcome is unknown'); }
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

# Read-only native focus probes. Reopening goes through the installed app's
# single-instance Show command, never a global shortcut or a foreign window.
WINDOWS_PICKER = r"""
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class IbisUpdateFocus {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
'@
$A = [System.Windows.Automation.AutomationElement]
$S = [System.Windows.Automation.TreeScope]
$pickerWindows = @(); $count = 0
foreach ($id in @($env:IBIS_PIDS -split ',' | Where-Object { $_ } | Select-Object -Unique)) {
  if ($id -notmatch '^[1-9][0-9]{0,9}$' -or ++$count -gt 20) { throw 'Invalid picker scope' }
  $condition = [System.Windows.Automation.PropertyCondition]::new($A::ProcessIdProperty, [int]$id)
  $windows = @($A::RootElement.FindAll($S::Children, $condition))
  if ($windows.Count -gt 20) { throw 'Picker window bound exceeded' }
  foreach ($window in $windows) {
    if ($window.Current.ProcessId -ne [int]$id) { throw 'Picker owner changed' }
    if ($window.Current.Name -ceq 'Ibis' -and !$window.Current.IsOffscreen) { $pickerWindows += $window }
  }
}
if ($pickerWindows.Count -gt 1) { throw 'Picker is ambiguous' }
$owner = $null
if ($pickerWindows.Count -eq 1) {
  $window = $pickerWindows[0]
  $handle = $window.Current.NativeWindowHandle
  if ($handle -ne 0 -and [IbisUpdateFocus]::GetForegroundWindow().ToInt64() -eq [long]$handle) {
    $owner = $window.Current.ProcessId
  }
}
@{pid=$owner} | ConvertTo-Json -Compress
"""

MACOS_PICKER = r"""
ObjC.import('ApplicationServices');
function run(pids) {
  if (pids.length > 20 || pids.some(pid => !/^[1-9][0-9]{0,9}$/.test(pid))) throw Error('Invalid picker scope');
  const attr = (item, name) => {
    const result = Ref();
    if ($.AXUIElementCopyAttributeValue(item, $(name), result) !== 0) throw Error('Picker focus unavailable');
    return result[0];
  };
  const events = Application('System Events');
  const matches = [];
  for (const pid of pids) {
    for (const app of events.processes.whose({ unixId: Number(pid) })()) {
      const windows = app.windows();
      if (windows.length > 20) throw Error('Picker window bound exceeded');
      for (const window of windows) {
        if (window.name() === 'Ibis' && window.visible()) matches.push(Number(pid));
      }
    }
  }
  if (matches.length > 1) throw Error('Picker is ambiguous');
  if (matches.length === 0) return JSON.stringify({pid: null});
  const focused = attr($.AXUIElementCreateSystemWide(), 'AXFocusedUIElement');
  const owner = Ref();
  if ($.AXUIElementGetPid(focused, owner) !== 0) throw Error('Picker owner unavailable');
  if (Number(owner[0]) !== matches[0]) return JSON.stringify({pid: null});
  const window = attr($.AXUIElementCreateApplication(matches[0]), 'AXFocusedWindow');
  return JSON.stringify({pid: ObjC.unwrap(attr(window, 'AXTitle')) === 'Ibis' ? matches[0] : null});
}
"""


def picker_result(stdout, pids):
    result = json.loads(stdout)
    if not isinstance(result, dict) or set(result) != {"pid"}:
        raise AssertionError("picker helper returned an unexpected answer")
    owner = result["pid"]
    if owner is not None and (type(owner) is not int or owner not in pids):
        raise AssertionError("picker helper returned an unowned process")
    return owner


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def seed_settings(config, library):
    """Update checks already allowed, so the first-launch question never shows."""
    settings = config / IDENTIFIER / "settings.json"
    settings.parent.mkdir(parents=True, exist_ok=True)
    settings.write_text(json.dumps({"schema_version": 2, "update_checks": True,
                                    "library": {"bibtex": str(library)}, "shortcut": SHORTCUT}), encoding="utf-8")
    return settings


def claim_settings(config, library):
    """This harness uses Windows/macOS's account settings folder and must
    never overwrite someone's real settings."""
    folder = config / IDENTIFIER
    if settings_exist(folder / "settings.json"):
        raise SystemExit(f"Ibis Nightly settings already exist in {folder}; use a disposable account")
    return seed_settings(config, library)


def environment(inherited, config):
    env = dict(inherited)
    if config is not None:
        env.update(XDG_CONFIG_HOME=str(config), NO_AT_BRIDGE="0", GTK_MODULES="gail:atk-bridge")
    for name in ("APPIMAGE", "APPDIR", "IBIS_GALLERY", "IBIS_GALLERY_STATE", "IBIS_LIBRARY"):
        env.pop(name, None)
    return env


def preserved_settings(settings, library):
    """Fixed facts only: never include settings contents or paths in a failure."""
    try:
        metadata = settings.lstat()
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 65536:
            raise ValueError("invalid synthetic settings")
        with settings.open("rb") as source:
            contents = source.read(65537)
        if len(contents) > 65536:
            raise ValueError("oversized synthetic settings")
        saved = json.loads(contents)
    except (OSError, ValueError, RecursionError):
        raise AssertionError("the updated app's synthetic settings could not be read") from None
    for field, expected in (("library", {"bibtex": str(library)}), ("shortcut", SHORTCUT)):
        if not isinstance(saved, dict) or saved.get(field) != expected:
            raise AssertionError(f"the synthetic {field} choice was not preserved")
    return {"library": True, "shortcut": True}


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

    def __init__(self, folder, args, library):
        import pyatspi
        self.atspi = pyatspi
        self.config = folder / "config"
        self.settings = seed_settings(self.config, library)
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

    def picker_ready(self, pids):
        # The update lane's Xvfb has no WM. Native input focus still resolves
        # to the owning top-level via WM_CLASS; AT-SPI focus alone is stale.
        deadline = time.monotonic() + 5
        def query(*args):
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise AssertionError("picker focus observation timed out")
            result = subprocess.run(["xdotool", *args], capture_output=True, timeout=remaining)
            if result.returncode == 1 and args[0] == "search" and not result.stdout and not result.stderr:
                return ""
            return helper_stdout(result, "X11 picker").strip()
        windows, count = set(), 0
        if len(pids) > 20:
            raise AssertionError("picker process bound exceeded")
        for pid in pids:
            for window in query("search", "--onlyvisible", "--pid", str(pid)).splitlines():
                count += 1
                if not window.isdecimal() or count > 20:
                    raise AssertionError("picker window bound exceeded")
                if query("getwindowpid", window) != str(pid):
                    raise AssertionError("picker window owner changed")
                if query("getwindowname", window) == "Ibis":
                    windows.add((pid, window))
        if len(windows) > 1:
            raise AssertionError("picker is ambiguous")
        if not windows:
            return None
        pid, window = windows.pop()
        focused = query("getwindowfocus")
        if focused != window or query("getwindowpid", focused) != str(pid):
            return None
        # A closed/replaced window cannot lend its earlier focus to a retry.
        if query("getwindowname", window) != "Ibis" or query("getwindowpid", window) != str(pid):
            raise AssertionError("picker window changed during focus observation")
        return pid

    def look(self, pids, press=None):
        seen, visited, matches = [], 0, []
        for app in self.atspi.Registry.getDesktop(0):
            if not app or app.get_process_id() not in pids:
                continue
            stack = [(app, False)]
            while stack:
                item, in_offer = stack.pop()
                visited += 1
                if visited > 20000:
                    raise AssertionError("update accessibility tree bound exceeded")
                try:
                    role = item.getRoleName()
                    owner = item.get_process_id()
                    name = item.name
                    if owner not in pids:
                        raise AssertionError("update accessibility owner changed")
                    in_offer = in_offer or (role in ("dialog", "alert", "frame") and name == "An update is available")
                    if press and in_offer and role == "push button" and name == press:
                        matches.append(item)
                    if role in ("frame", "dialog", "alert", "push button", "label") and name and len(seen) < 200:
                        seen.append(f"[{owner}] {role}: {name[:200]}")
                    stack.extend((child, in_offer) for child in item if child)
                except AssertionError:
                    raise
                except Exception:
                    # A partial census cannot prove that activation is unique.
                    if press:
                        raise AssertionError("update accessibility observation unavailable") from None
        if len(matches) > 1:
            raise AssertionError("update action is ambiguous")
        if matches:
            try:
                item = matches[0]
                if item.get_process_id() not in pids or item.name != press:
                    raise AssertionError("update action owner changed")
                action = item.queryAction()
                if action.nActions != 1 or not action.doAction(0):
                    raise AssertionError("update action was refused")
            except Exception:
                raise AssertionError("Linux update activation failed or its outcome is unknown") from None
            return True, seen, None
        return False, seen, None

    def close(self):
        pass


class MacOS:
    """The .app from the update archive in a private Applications folder; the
    updater replaces the bundle there and reopens it with LaunchServices."""

    def __init__(self, folder, args, library):
        # Resolved, because LaunchServices reopens /private/var/..., not /var/...
        folder = folder.resolve()
        self.config = None
        self.settings = claim_settings(Path.home() / "Library/Application Support", library)
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

    def picker_ready(self, pids):
        result = subprocess.run(["osascript", "-l", "JavaScript", "-e", MACOS_PICKER, *map(str, pids)],
                                capture_output=True, timeout=10)
        return picker_result(helper_stdout(result, "System Events picker"), pids)

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

    def __init__(self, folder, args, library):
        if not args.version:
            raise SystemExit("--version is required on Windows")
        self.version = args.version
        self.config = None
        roaming = powershell("[Environment]::GetFolderPath('ApplicationData')").strip()
        self.settings = claim_settings(Path(roaming), library)
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

    def picker_ready(self, pids):
        return picker_result(powershell(WINDOWS_PICKER, timeout=10, IBIS_PIDS=",".join(map(str, pids))), pids)

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


def powershell(script, *, timeout=120, **env):
    """Runs a script from a file: commands read from standard input end at the
    first line break inside a block. Values arrive as environment variables."""
    with tempfile.TemporaryDirectory(prefix="ibis update helper ") as folder:
        path = Path(folder) / "helper.ps1"
        # JSON and paths must survive Windows' non-UTF-8 default code page.
        path.write_text("[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)\n" + script, encoding="utf-8")
        result = subprocess.run(["pwsh", "-NoProfile", "-NonInteractive", "-File", str(path)],
                                capture_output=True, timeout=timeout, env=dict(os.environ, **env))
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
                install = SYSTEMS[platform.system()](folder, args, library)
                report["steps"].append("previous nightly installed")

                log = (args.output / "update-e2e-app.log").open("w", encoding="utf-8")
                app = subprocess.Popen(install.command, env=environment(os.environ, install.config),
                                       stdout=log, stderr=log, cwd=folder)
                started.append(app)
                report["steps"].append("previous nightly started")

                wait_for(install.instances, PICKER_TIMEOUT, "the previous nightly did not start")
                # Let first-launch registration settle, then issue Show exactly
                # once through the same owned command. The child must exit;
                # a second persistent instance is never a successful reopen.
                time.sleep(5)
                initial = install.instances()
                if not initial:
                    raise AssertionError("the previous nightly exited before its reopen")
                reopen = subprocess.Popen(install.command, env=environment(os.environ, install.config),
                                          stdout=log, stderr=log, cwd=folder)
                started.append(reopen)
                if reopen.wait(timeout=15) != 0:
                    raise AssertionError("the previous nightly refused its single-instance reopen")
                def picker_ready():
                    pids = initial & install.instances()
                    owner = install.picker_ready(pids)
                    if owner is not None and (type(owner) is not int or owner not in pids):
                        raise AssertionError("the reopened picker has no original process owner")
                    return owner
                picker = wait_for(picker_ready, PICKER_TIMEOUT, "the previous nightly's owned picker was not visible and focused")
                report["steps"].append("previous nightly reopened once with its owned picker visible and focused")

                # The processes running just before the press, so helper
                # processes the old version starts later never count as new.
                seen["old"] = set()
                def offered():
                    seen["old"] = install.instances()
                    if picker not in seen["old"]:
                        raise AssertionError("the reopened picker exited before offering the update")
                    try:
                        pressed, _, seen["probe"] = install.look({picker}, BUTTON)
                        return pressed
                    except (RuntimeError, ValueError, subprocess.TimeoutExpired) as problem:
                        seen["helper"] = type(problem).__name__
                        raise AssertionError("update activation helper failed or its outcome is unknown") from None
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
                report["settings_preserved"] = preserved_settings(install.settings, library)
                report["steps"].append("synthetic library and shortcut choices preserved")
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
    @unittest.skipUnless(shutil.which("node"), "Node is required for the macOS picker provider contract")
    def test_macos_picker_requires_owned_unique_visible_and_native_focused_window(self):
        model = r'''
const scenario = process.argv[1];
const ObjC = {import() {}, unwrap: value => value};
const Ref = () => [];
const $ = name => name;
$.AXUIElementCreateSystemWide = () => 'system';
$.AXUIElementCreateApplication = pid => { if (pid !== 42) throw Error('unowned app'); return 'app'; };
$.AXUIElementCopyAttributeValue = (item, name, result) => {
  if (scenario === 'unknown') return -1;
  const values = {'system:AXFocusedUIElement': 'focused', 'app:AXFocusedWindow': 'window',
    'window:AXTitle': scenario === 'wrong-focus-title' ? 'Other' : scenario === 'offer-focused' ? 'An update is available' : 'Ibis'};
  if (!(item + ':' + name in values)) throw Error('unexpected attribute');
  result[0] = values[item + ':' + name]; return 0;
};
$.AXUIElementGetPid = (item, result) => { result[0] = scenario === 'foreign' ? 99 : 42; return scenario === 'owner-error' ? -1 : 0; };
const window = {name: () => scenario === 'title' ? 'Ibis suffix' : 'Ibis', visible: () => scenario !== 'hidden'};
const Application = name => {
  if (name !== 'System Events') throw Error('unexpected application');
  return {processes: {whose: ({unixId}) => {
    if (unixId !== 42) throw Error('unowned process');
    return () => [{windows: () => scenario === 'duplicate' ? [window, window] :
      scenario === 'offer-focused' ? [window, {name: () => 'An update is available', visible: () => true}] : [window]}];
  }}};
};
'''
        for scenario in ("success", "hidden", "title", "duplicate", "foreign", "unknown", "owner-error", "wrong-focus-title", "offer-focused"):
            with self.subTest(scenario=scenario):
                result = subprocess.run([shutil.which("node"), "-e", model + MACOS_PICKER + "\nprocess.stdout.write(run(['42']));", scenario],
                                        capture_output=True, timeout=5)
                if scenario in ("duplicate", "unknown", "owner-error"):
                    self.assertNotEqual(result.returncode, 0)
                else:
                    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", errors="replace"))
                    expected = (42, "offer") if scenario == "offer-focused" else 42 if scenario == "success" else None
                    self.assertEqual(picker_result(result.stdout, {42}), expected)

    def test_linux_picker_accepts_an_immediate_owned_offer_without_accepting_foreign_or_duplicate_modals(self):
        for scenario in ("offer", "foreign", "duplicate"):
            def observe(command, **_kwargs):
                args = command[1:]
                values = {("search", "--onlyvisible", "--pid", "42"): "101\n102" + ("\n103" if scenario == "duplicate" else ""),
                          ("getwindowname", "101"): "Ibis", ("getwindowname", "102"): "An update is available",
                          ("getwindowname", "103"): "An update is available", ("getwindowfocus",): "102"}
                value = values.get(tuple(args), "99" if scenario == "foreign" and args == ["getwindowpid", "102"] else "42")
                return subprocess.CompletedProcess(command, 0, stdout=value.encode(), stderr=b"")
            with self.subTest(scenario=scenario), patch("subprocess.run", side_effect=observe):
                linux = Linux.__new__(Linux)
                if scenario == "offer":
                    self.assertEqual(linux.picker_ready({42}), (42, "offer"))
                else:
                    with self.assertRaises(AssertionError):
                        linux.picker_ready({42})

    def test_picker_helper_refuses_unknown_or_unowned_focus(self):
        for payload in ('{}', '[]', '{"pid":true}', '{"pid":"42"}', '{"pid":99}', '{"pid":42,"extra":1}'):
            with self.subTest(payload=payload), self.assertRaises(AssertionError):
                picker_result(payload, {42})
        self.assertIsNone(picker_result('{"pid":null}', {42}))
        self.assertEqual(picker_result('{"pid":42}', {42}), 42)

    def test_linux_picker_requires_unique_owned_visible_main_and_native_input_focus(self):
        for scenario in ("success", "absent", "foreign", "duplicate", "wrong-title", "unfocused", "unknown", "closed", "replaced"):
            queries, names = [], 0
            def observed(command, **kwargs):
                nonlocal names
                self.assertGreater(kwargs["timeout"], 0)
                self.assertLessEqual(kwargs["timeout"], 5)
                args = command[1:]
                queries.append(args)
                if args[0] == "search":
                    self.assertEqual(args, ["search", "--onlyvisible", "--pid", "42"])
                    if scenario == "absent":
                        return subprocess.CompletedProcess(command, 1, stdout=b"", stderr=b"")
                    value = "101\n102" if scenario == "duplicate" else "101"
                elif args[0] == "getwindowpid":
                    value = "99" if scenario == "foreign" or (scenario == "replaced" and names > 1) else "42"
                elif args[0] == "getwindowname":
                    names += 1
                    value = "Other" if scenario == "wrong-title" or (scenario == "closed" and names > 1) else "Ibis"
                elif args[0] == "getwindowfocus":
                    if scenario == "unknown":
                        raise subprocess.TimeoutExpired(command, 5)
                    value = "999" if scenario == "unfocused" else "101"
                else:
                    self.fail("unexpected native operation")
                return subprocess.CompletedProcess(command, 0, stdout=value.encode(), stderr=b"")
            with self.subTest(scenario=scenario), patch("subprocess.run", side_effect=observed):
                linux = Linux.__new__(Linux)
                if scenario in ("foreign", "duplicate", "closed", "replaced", "unknown"):
                    with self.assertRaises((AssertionError, subprocess.TimeoutExpired)):
                        linux.picker_ready({42})
                else:
                    self.assertEqual(linux.picker_ready({42}), 42 if scenario == "success" else None)
            self.assertTrue(all(args[0] in ("search", "getwindowpid", "getwindowname", "getwindowfocus") for args in queries))

    def update_consumer(self, scenario):
        events = []
        state = {"reopened": False, "pressed": False}
        with tempfile.TemporaryDirectory() as temporary:
            def installed(folder, _args, library):
                install = MagicMock(command=["synthetic-owned-install"], config=None)
                install.settings = seed_settings(folder, library)
                install.instances.side_effect = lambda: {43} if state["pressed"] or (scenario == "replacement" and state["reopened"]) else {42}
                def picker(pids):
                    events.append(("picker", set(pids)))
                    if state["reopened"] and scenario == "immediate-offer":
                        return 42, "offer"
                    return 42 if state["reopened"] and scenario != "unfocused" else None
                install.picker_ready.side_effect = picker
                def look(pids, press=None):
                    if press:
                        events.append(("press", set(pids)))
                        self.assertTrue(state["reopened"], "update polled before its picker was reopened")
                        if scenario == "activation":
                            raise WindowsActivationFailure({"attempted": True})
                        if scenario == "helper":
                            raise RuntimeError("synthetic uncertain helper")
                        state["pressed"] = True
                    return bool(press), [], None
                install.look.side_effect = look
                install.updated.side_effect = lambda: state["pressed"]
                install.facts.return_value = {}
                return install
            def launch(command, **_kwargs):
                events.append(("launch", list(command)))
                child = MagicMock()
                child.poll.return_value = 0
                def waited(timeout):
                    events.append(("reopen-exit", timeout))
                    if scenario == "io":
                        raise OSError("synthetic reopen failure")
                    if scenario == "timeout":
                        raise subprocess.TimeoutExpired(command, timeout)
                    state["reopened"] = True
                    return 1 if scenario == "refused" else 0
                child.wait.side_effect = waited
                return child
            def bounded(predicate, _timeout, message, **_kwargs):
                for _ in range(3):
                    value = predicate()
                    if value:
                        return value
                raise AssertionError(message)
            with patch.dict(SYSTEMS, {"Linux": installed}), patch("platform.system", return_value="Linux"), \
                    patch("subprocess.Popen", side_effect=launch), patch(__name__ + ".wait_for", side_effect=bounded), \
                    patch("time.sleep"), patch("os.kill"), patch("builtins.print"), patch(__name__ + ".feed_version", return_value="synthetic"):
                result = run(argparse.Namespace(output=Path(temporary)))
            report = json.loads((Path(temporary) / "update-e2e.json").read_text(encoding="utf-8"))
        return result, report, events

    def test_update_reopens_exact_install_once_before_single_owned_activation(self):
        result, report, events = self.update_consumer("success")
        self.assertEqual(result, 0, report)
        self.assertEqual([value for event, value in events if event == "launch"], [["synthetic-owned-install"]] * 2)
        self.assertEqual([value for event, value in events if event == "press"], [{42}])
        self.assertEqual(report["settings_preserved"], {"library": True, "shortcut": True})

    def test_update_accepts_an_owned_offer_already_focused_by_reopen(self):
        result, report, events = self.update_consumer("immediate-offer")
        self.assertEqual(result, 0, report)
        self.assertEqual(report["reopen_focus"], "offer")
        self.assertEqual([value for event, value in events if event == "press"], [{42}])
        self.assertEqual(sum(event == "launch" for event, _ in events), 2)

    def test_update_never_activates_after_reopen_refusal_io_timeout_or_lost_owner(self):
        for scenario in ("refused", "io", "timeout", "unfocused", "replacement"):
            with self.subTest(scenario=scenario):
                result, report, events = self.update_consumer(scenario)
                self.assertEqual(result, 1, report)
                self.assertEqual(sum(event == "launch" for event, _ in events), 2)
                self.assertFalse(any(event == "press" for event, _ in events))
                self.assertNotIn("settings_preserved", report)

    def test_update_never_retries_an_uncertain_activation_after_reopen(self):
        for scenario in ("activation", "helper"):
            with self.subTest(scenario=scenario):
                result, report, events = self.update_consumer(scenario)
                self.assertEqual(result, 1, report)
                self.assertEqual(sum(event == "launch" for event, _ in events), 2)
                self.assertEqual(sum(event == "press" for event, _ in events), 1)

    class Accessible:
        def __init__(self, pid, role, name, children=()):
            self.pid, self.role, self.name, self.children = pid, role, name, children
            self.action = MagicMock(nActions=1)
            self.action.doAction.return_value = True
        def __iter__(self):
            return iter(self.children)
        def get_process_id(self):
            return self.pid
        def getRoleName(self):
            return self.role
        def queryAction(self):
            return self.action

    def test_linux_update_action_is_unique_and_owned_before_any_invocation(self):
        for scenario in ("success", "foreign", "duplicate", "wrong-owner", "uncertain"):
            with self.subTest(scenario=scenario):
                button = self.Accessible(42, "push button", BUTTON)
                if scenario == "wrong-owner":
                    button.pid = 99
                if scenario == "uncertain":
                    button.action.doAction.side_effect = RuntimeError("synthetic uncertain activation")
                offer = self.Accessible(42, "dialog", "An update is available", [button] * (2 if scenario == "duplicate" else 1))
                foreign_button = self.Accessible(99, "push button", BUTTON)
                foreign = self.Accessible(99, "application", "Foreign", [foreign_button])
                owned = self.Accessible(42, "application", "Ibis", [] if scenario == "foreign" else [offer])
                linux = Linux.__new__(Linux)
                linux.atspi = MagicMock()
                linux.atspi.Registry.getDesktop.return_value = [foreign, owned]
                if scenario in ("duplicate", "wrong-owner", "uncertain"):
                    with self.assertRaises(AssertionError):
                        linux.look({42}, BUTTON)
                else:
                    self.assertEqual(linux.look({42}, BUTTON)[0], scenario == "success")
                foreign_button.action.doAction.assert_not_called()
                self.assertEqual(button.action.doAction.call_count, int(scenario in ("success", "uncertain")))

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
 public static class Focus { public static long Handle=101; public static IntPtr GetForegroundWindow() {return new IntPtr(Handle);} }
 public class Node {
  public string Name="", Type="Pane", AutomationId=""; public int ProcessId=42; public bool IsEnabled=true, IsOffscreen=false;
  public int NativeWindowHandle=101;
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
if ($Scenario.StartsWith('picker-')) {
 $window.Name='Ibis'
 switch($Scenario) {
  'picker-hidden' {$window.IsOffscreen=$true}
  'picker-title' {$window.Name='Ibis suffix'}
  'picker-duplicate' {[IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($window,$window)}
  'picker-foreign' {$window.ProcessId=99}
  'picker-unfocused' {[IbisUpdateContract.Focus]::Handle=102}
  'picker-unknown' {[IbisUpdateContract.Focus]::Handle=0}
  'picker-no-handle' {$window.NativeWindowHandle=0}
  'picker-empty' {[IbisUpdateContract.AutomationElement]::RootElement.Nodes=@()}
  'picker-offer' {
   $offer=[IbisUpdateContract.Node]::new(); $offer.Name='An update is available'; $offer.NativeWindowHandle=102
   [IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($window,$offer)
   [IbisUpdateContract.Focus]::Handle=102
  }
 }
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

            # Execute the production focus probe against the same private UIA
            # provider; substitute only the native foreground observation.
            picker_helper = WINDOWS_PICKER.replace("Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes", "")
            native_start = picker_helper.index("Add-Type @'")
            native_end = picker_helper.index("'@", native_start) + 2
            picker_helper = picker_helper[:native_start] + picker_helper[native_end:]
            picker_helper = picker_helper.replace("System.Windows.Automation.", "IbisUpdateContract.")
            picker_helper = picker_helper.replace("[IbisUpdateFocus]", "[IbisUpdateContract.Focus]")
            script.write_text(model + picker_helper, encoding="utf-8")
            for scenario in ("success", "hidden", "title", "duplicate", "foreign", "unfocused", "unknown", "no-handle", "empty", "offer"):
                with self.subTest(picker=scenario):
                    result = subprocess.run([shutil.which("pwsh"), "-NoProfile", "-File", str(script), "picker-" + scenario],
                                            capture_output=True, timeout=30)
                    if scenario in ("duplicate", "foreign"):
                        self.assertNotEqual(result.returncode, 0)
                    else:
                        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", errors="replace"))
                        expected = (42, "offer") if scenario == "offer" else 42 if scenario == "success" else None
                        self.assertEqual(picker_result(result.stdout, {42}), expected)

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
        windows.picker_ready = MagicMock(return_value=42)
        def two_polls(predicate, _timeout, message, **_kwargs):
            for _ in range(2):
                value = predicate()
                if value:
                    return value
            raise AssertionError(message)
        for problem in (RuntimeError("synthetic"), ValueError("synthetic"), subprocess.TimeoutExpired("synthetic", 120)):
            with self.subTest(problem=type(problem).__name__), tempfile.TemporaryDirectory() as temporary, \
                    patch.dict(SYSTEMS, {"Windows": lambda *_args: windows}), patch("platform.system", return_value="Windows"), \
                    patch(__name__ + ".powershell", side_effect=problem) as helper, \
                    patch(__name__ + ".wait_for", side_effect=two_polls), patch(__name__ + ".feed_version", return_value="synthetic"), \
                    patch("subprocess.Popen") as launch, patch("time.sleep"), patch("os.kill"), patch("builtins.print"):
                launch.return_value.wait.return_value = 0
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
            library = Path(temporary) / "synthetic 引用.bib"
            settings = seed_settings(Path(temporary), library)
            self.assertEqual(settings, Path(temporary) / "com.ibiscite.ibis.nightly/settings.json")
            self.assertEqual(json.loads(settings.read_text(encoding="utf-8")),
                             {"schema_version": 2, "update_checks": True,
                              "library": {"bibtex": str(library)}, "shortcut": "Control+Alt+Y"})

    def test_restarted_update_must_keep_choices_without_an_environment_override(self):
        for changed in ("library", "shortcut", None):
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as temporary:
                def installed(folder, _args, _library):
                    settings = folder / "settings.json"
                    choices = {"library": {"bibtex": str(folder / "synthetic.bib")}, "shortcut": "Control+Alt+Y"}
                    settings.write_text(json.dumps(choices), encoding="utf-8")
                    state = {"replaced": False}
                    def replaced():
                        state["replaced"] = True
                        if changed:
                            choices[changed] = "synthetic wrong choice"
                            settings.write_text(json.dumps(choices), encoding="utf-8")
                        return True
                    app = MagicMock(command=["synthetic"], config=None, settings=settings)
                    app.instances.side_effect = lambda: {43} if state["replaced"] else {42}
                    app.picker_ready.return_value = 42
                    app.look.return_value = (True, [], None)
                    app.updated.side_effect = replaced
                    app.facts.return_value = {}
                    return app
                process = MagicMock()
                process.poll.return_value = None
                process.wait.return_value = 0
                with patch.dict(SYSTEMS, {"Linux": installed}), patch("platform.system", return_value="Linux"), \
                        patch.dict(os.environ, {"IBIS_LIBRARY": "/synthetic outer override.bib"}), \
                        patch("subprocess.Popen", return_value=process) as launch, patch("time.sleep"), \
                        patch("os.kill"), patch("builtins.print"), patch(__name__ + ".feed_version", return_value="synthetic"):
                    self.assertEqual(run(argparse.Namespace(output=Path(temporary))), int(changed is not None))
                self.assertTrue("IBIS_LIBRARY" not in launch.call_args.kwargs["env"], "library override masks persistence")
                report = json.loads((Path(temporary) / "update-e2e.json").read_text(encoding="utf-8"))
                self.assertEqual(report["status"], "failed" if changed else "passed")
                if not changed:
                    self.assertEqual(report["settings_preserved"], {"library": True, "shortcut": True})

    def test_settings_verification_refuses_bad_files_without_disclosing_them(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            library = root / "synthetic 引用.bib"
            settings = seed_settings(root, library)
            original = settings.read_bytes()
            for contents in (b"", b"{private payload", b"\xff", b"[]", b"null", b"x" * 65537,
                             b"[" * 2000 + b"]" * 2000):
                settings.write_bytes(contents)
                with self.subTest(contents=contents[:8]), self.assertRaises(AssertionError) as failure:
                    preserved_settings(settings, library)
                self.assertNotIn(str(root), str(failure.exception))
                self.assertNotIn("private payload", str(failure.exception))
            settings.write_bytes(original)
            saved = json.loads(original)
            saved["unknown"] = "kept"
            settings.write_text(json.dumps(saved), encoding="utf-8")
            self.assertEqual(preserved_settings(settings, library), {"library": True, "shortcut": True})
            settings.unlink()
            settings.mkdir()
            with self.assertRaises(AssertionError):
                preserved_settings(settings, library)
            settings.rmdir()
            if hasattr(os, "mkfifo"):
                os.mkfifo(settings)
                with patch.object(Path, "open", side_effect=AssertionError("special file was opened")) as opened:
                    with self.assertRaisesRegex(AssertionError, "synthetic settings could not be read"):
                        preserved_settings(settings, library)
                    opened.assert_not_called()
                settings.unlink()
            if os.name != "nt":
                settings.symlink_to(root / "unread synthetic target")
                with patch.object(Path, "open", side_effect=AssertionError("symlink was opened")) as opened:
                    with self.assertRaisesRegex(AssertionError, "synthetic settings could not be read"):
                        preserved_settings(settings, library)
                    opened.assert_not_called()

    def test_claim_never_overwrites_existing_settings_and_clean_removes_only_its_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            settings = claim_settings(root, root / "synthetic.bib")
            (settings.parent / "settings.json.bak").write_text("{}", encoding="utf-8")
            (settings.parent / "other.json").write_text("{}", encoding="utf-8")
            with self.assertRaises(SystemExit):
                claim_settings(root, root / "synthetic.bib")
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
                    claim_settings(root / "config", root / "synthetic.bib")
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
        env = environment({"APPIMAGE": "/outer", "PATH": "/bin"}, Path("/c"))
        self.assertNotIn("APPIMAGE", env)
        self.assertEqual((env["XDG_CONFIG_HOME"], env["PATH"]), (str(Path("/c")), "/bin"))

    def test_environment_without_a_config_folder_uses_the_persisted_library(self):
        env = environment({"PATH": "/bin", "IBIS_GALLERY": "1", "IBIS_LIBRARY": "/synthetic outer.bib"}, None)
        self.assertEqual(env, {"PATH": "/bin"})

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
