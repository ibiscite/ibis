#!/usr/bin/env python3
"""End-to-end update test for Ibis Nightly (#319, #493).

Installs the previous Ibis Nightly with automatic update checks on, waits for
it to offer the newly published nightly, accepts one owned native install/restart
action and checks that the installed app was
replaced by the new version and that the new version started in its place.
The seeded synthetic library and shortcut must remain in its settings.

- Linux: the AppImage, under a virtual display, with one deliberate Install activation in the
  exact owned, managed, visible, focused X11 offer window.
- Windows: the setup executable, pressed through UI Automation; the updater
  runs the new setup executable, which starts the app again. The isolated test
  forces renderer accessibility; normal screen-reader startup is not qualified.
- macOS: the .app from its update archive, pressed through native AX; the
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
import importlib
import json
import os
import platform
import plistlib
import re
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

# Use the passing Linux smoke's EWMH parser, including its refusal of an
# unavailable client list. A same-process GTK auxiliary is not the picker.
managed_client_windows = importlib.import_module("smoke-native").managed_client_windows

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
WINDOWS_WINDOW_SCOPE = r"""
function Stop-UpdateScope($scope, $stage) {
  # Explicit scope assertions are terminal even if a later census is clean.
  # Native UIA exceptions do not set this marker and may be observed again.
  $scope.failure_stage = $stage
  throw 'Update window scope assertion failed'
}
function Get-OwnedWindows($ids, $scope) {
  $owned = [System.Collections.Generic.List[object]]::new()
  if ($ids.Count -gt 20) { Stop-UpdateScope $scope 'picker-bounds' }
  foreach ($id in $ids) {
    if ($id -notmatch '^[1-9][0-9]{0,9}$') { Stop-UpdateScope $scope 'scope' }
    $scope.pids++
    $condition = [System.Windows.Automation.PropertyCondition]::new($A::ProcessIdProperty, [int]$id)
    $roots = @($A::RootElement.FindAll($S::Children, $condition))
    if ($roots.Count -gt 20) { Stop-UpdateScope $scope 'picker-bounds' }
    foreach ($root in $roots) {
      if ($root.Current.ProcessId -ne [int]$id) { Stop-UpdateScope $scope 'picker-owner' }
      $scope.root_windows++
      $owned.Add($root)
      # Owned modal windows need not be desktop children. As on macOS, inspect
      # each owned window's native dialog/window descendants, never other apps.
      $windowType = [System.Windows.Automation.PropertyCondition]::new(
        $A::ControlTypeProperty, [System.Windows.Automation.ControlType]::Window)
      $nested = @($root.FindAll($S::Descendants, $windowType))
      if ($nested.Count -gt 20) { Stop-UpdateScope $scope 'picker-bounds' }
      foreach ($window in $nested) {
        if ($window.Current.ProcessId -ne [int]$id) {
          if ($window.Current.Name -ceq 'An update is available') { Stop-UpdateScope $scope 'picker-owner' }
          continue # A renderer window cannot supply the native updater action.
        }
        $scope.nested_windows++
        $owned.Add($window)
      }
      $scope.windows = $owned.Count
      if ($scope.windows -gt 20) { Stop-UpdateScope $scope 'picker-bounds' }
    }
  }
  $scope.tree_complete = $true
  return $owned.ToArray()
}
function In-OwnedWindow($item, $window) {
  $wanted = @($window.GetRuntimeId()) -join ','
  if (!$wanted -or $window.Current.NativeWindowHandle -eq 0) { return $false }
  $node = $item
  for ($depth = 0; $depth -lt 32; $depth++) {
    $node = [System.Windows.Automation.TreeWalker]::RawViewWalker.GetParent($node)
    if (!$node) { return $false }
    if ($node.Current.ControlType.ProgrammaticName -ceq 'ControlType.Window') {
      return ((@($node.GetRuntimeId()) -join ',') -ceq $wanted -and
        $node.Current.ProcessId -eq $window.Current.ProcessId -and
        $node.Current.NativeWindowHandle -eq $window.Current.NativeWindowHandle)
    }
  }
  throw 'Update action ancestry bound exceeded'
}
"""

WINDOWS_NATIVE_INSTALL = r"""
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class IbisUpdateTaskDialog {
  [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
  [DllImport("user32.dll")] static extern bool IsWindowEnabled(IntPtr window);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder text, int count);
  [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", CharSet=CharSet.Unicode)] static extern IntPtr SendMessageTimeout(
    IntPtr window, uint message, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out IntPtr result);
  public static bool Ready(long handle, int owner) {
    var dialog=new IntPtr(handle); uint dialogOwner;
    GetWindowThreadProcessId(dialog,out dialogOwner);
    if(dialog==IntPtr.Zero || GetForegroundWindow()!=dialog || dialogOwner!=(uint)owner ||
       !IsWindowVisible(dialog) || !IsWindowEnabled(dialog)) return false;
    var caption=new StringBuilder(128); var kind=new StringBuilder(64);
    GetWindowText(dialog,caption,caption.Capacity); GetClassName(dialog,kind,kind.Capacity);
    return caption.ToString()=="An update is available" && kind.ToString()=="#32770";
  }
  public static bool Click(long handle, int owner) {
    if(!Ready(handle,owner)) return false;
    IntPtr result;
    // TaskDialog actions can be virtual controls without a Button HWND or an
    // InvokePattern. Locked rfd's second custom action is Install, ID 1008.
    // The caller rechecks its exact UIA name/identity before this one bounded
    // TDM_CLICK_BUTTON (WM_USER+102), never a default-button/key activation.
    return SendMessageTimeout(new IntPtr(handle),0x466,new IntPtr(1008),IntPtr.Zero,3,5000,out result)!=IntPtr.Zero;
  }
}
'@
"""

WINDOWS_LOOK = WINDOWS_WINDOW_SCOPE + WINDOWS_NATIVE_INSTALL + r"""
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]
$S = [System.Windows.Automation.TreeScope]
$seen = [System.Collections.Generic.List[string]]::new()
$probe = @{ pids = 0; windows = 0; elements = 0; errors = [System.Collections.Generic.List[string]]::new()
  root_windows = 0; nested_windows = 0; tree_complete = $false
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
  $owned = @(Get-OwnedWindows $ids $probe); $offers = @()
  foreach ($window in $owned) {
    $seen.Add('window: ' + $window.Current.Name)
    if ($window.Current.Name -ceq 'An update is available') {
      if ($env:IBIS_PRESS -and $window.Current.NativeWindowHandle -eq 0) { throw 'Update offer native window unavailable' }
      $offers += $window
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
        # Select the exact custom Install label. Older standard offers use
        # OS-localized Yes (IDYES=6), never No or the dialog's default action.
        $custom = $name -ceq $env:IBIS_PRESS
        $standard = !$custom -and $item.Current.ControlType.ProgrammaticName -ceq 'ControlType.Button' -and $item.Current.AutomationId -ceq '6'
        if (($custom -or $standard) -and (In-OwnedWindow $item $window)) { $matches += $item }
      }
      if ($name -and $seen.Count -lt 200) { $seen.Add($item.Current.ControlType.ProgrammaticName + ': ' + $name) }
    }
  }
  $probe.action_matches = $matches.Count
  if ($env:IBIS_PRESS -and $offers.Count -eq 1 -and $matches.Count -gt 0) {
    if ($matches.Count -ne 1) { throw 'Update action is ambiguous' }
    $item = $matches[0]; $window = $offers[0]
    $owner = $window.Current.ProcessId
    if ($ids -cnotcontains [string]$owner) { throw 'Update offer left the exact process scope' }
    $handle = $window.Current.NativeWindowHandle
    $windowIdentity = @($window.GetRuntimeId()) -join ','
    $actionIdentity = @($item.GetRuntimeId()) -join ','
    if ($item.Current.ProcessId -ne $window.Current.ProcessId) { throw 'Update action owner changed' }
    $probe.enabled = $item.Current.IsEnabled -and $window.Current.IsEnabled
    $probe.on_screen = !$item.Current.IsOffscreen -and !$window.Current.IsOffscreen
    $invoke = $null
    $probe.invoke_available = $item.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$invoke)
    if (!$probe.enabled -or !$probe.on_screen -or ($probe.invoke_available -and !$invoke)) { throw 'Update action unavailable' }
    if (!$probe.invoke_available) {
      if ($item.Current.Name -cne $env:IBIS_PRESS -or ![IbisUpdateTaskDialog]::Ready($handle,$owner)) { throw 'Update task dialog action unavailable' }
    }
    $stillAction = $item.Current.Name -ceq $env:IBIS_PRESS -or
      ($item.Current.ControlType.ProgrammaticName -ceq 'ControlType.Button' -and $item.Current.AutomationId -ceq '6')
    if ($window.Current.NativeWindowHandle -ne $handle -or $window.Current.Name -cne 'An update is available' -or
        (@($window.GetRuntimeId()) -join ',') -cne $windowIdentity -or
        $window.Current.ProcessId -ne $owner -or $item.Current.ProcessId -ne $owner -or
        (@($item.GetRuntimeId()) -join ',') -cne $actionIdentity -or !$stillAction -or
        !$window.Current.IsEnabled -or !$item.Current.IsEnabled -or $window.Current.IsOffscreen -or $item.Current.IsOffscreen -or
        !(In-OwnedWindow $item $window)) { throw 'Update action or window changed' }
    # Only absence polls. An uncertain invocation must never reach a retry,
    # and a successful invocation may immediately destroy the AX element.
    $probe.attempted = $true
    if ($probe.invoke_available) {
      $invoke.Invoke()
      Answer $true 'invoke'
    } else {
      if (![IbisUpdateTaskDialog]::Click($handle,$owner)) { throw 'Update task dialog action outcome unknown' }
      Answer $true 'task-dialog'
    }
    exit 0
  }
} catch { $probe.blocked = $true; Note 0 $_ }
Answer $false $null
"""

# Read-only native focus probes. Reopening goes through the installed app's
# single-instance Show command, never a global shortcut or a foreign window.
WINDOWS_PICKER = WINDOWS_WINDOW_SCOPE + r"""
$ErrorActionPreference = 'Stop'
$probe = @{stage='picker-frameworks'; code=$null; failed=$false; trusted=$null; windows=0; pickers=0; offers=0}
$owner = $null; $focus = $null
try {
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$probe.stage='picker-native-focus'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class IbisUpdateFocus {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
'@
$A = [System.Windows.Automation.AutomationElement]
$S = [System.Windows.Automation.TreeScope]
$pickerWindows = @(); $offerWindows = @()
$scope = @{pids=0; windows=0; root_windows=0; nested_windows=0; tree_complete=$false}
$probe.stage='scope'
$ids = @($env:IBIS_PIDS -split ',' | Where-Object { $_ } | Select-Object -Unique)
$probe.stage='picker-windows'
foreach ($window in @(Get-OwnedWindows $ids $scope)) {
  if ($window.Current.Name -ceq 'Ibis' -and !$window.Current.IsOffscreen) { $pickerWindows += $window }
  if ($window.Current.Name -ceq 'An update is available' -and !$window.Current.IsOffscreen) { $offerWindows += $window }
}
$probe.windows=$scope.windows; $probe.pickers=$pickerWindows.Count; $probe.offers=$offerWindows.Count
$probe.stage='picker-unique'
if ($pickerWindows.Count -gt 1 -or $offerWindows.Count -gt 1) { throw 'Picker or update offer is ambiguous' }
if ($pickerWindows.Count -eq 1) {
  $picker = $pickerWindows[0]
  $targets = @(@{window=$picker; kind='picker'})
  if ($offerWindows.Count -eq 1) {
    if ($offerWindows[0].Current.ProcessId -ne $picker.Current.ProcessId) { Stop-UpdateScope $scope 'picker-owner' }
    $targets += @{window=$offerWindows[0]; kind='offer'}
  }
  foreach ($target in $targets) {
    $probe.stage='picker-foreground'
    $handle = $target.window.Current.NativeWindowHandle
    if ($handle -ne 0 -and [IbisUpdateFocus]::GetForegroundWindow().ToInt64() -eq [long]$handle) {
      if ($target.kind -ceq 'picker') {
        # A foreground host can still expose no interactive Search control.
        # Observe only this unique owned main; never click/focus to repair it.
        $probe.stage='picker-search'
        $items = @($picker.FindAll($S::Descendants, [System.Windows.Automation.Condition]::TrueCondition))
        if ($items.Count -gt 4000) { Stop-UpdateScope $scope 'picker-bounds' }
        $search = @($items | Where-Object {
          $_.Current.Name -ceq 'Search references' -and
          $_.Current.ControlType.ProgrammaticName -cin @('ControlType.ComboBox', 'ControlType.Edit')
        })
        $probe.stage='picker-search-unique'
        if ($search.Count -gt 1) { throw 'Picker Search is ambiguous' }
        $probe.stage='picker-search'
        if ($search.Count -ne 1 -or !$search[0].Current.IsEnabled -or $search[0].Current.IsOffscreen -or !$search[0].Current.HasKeyboardFocus) { continue }
      }
      $owner = $target.window.Current.ProcessId; $focus = $target.kind
    }
  }
}
$probe.stage='complete'
} catch {
  $owner=$null; $focus=$null; $probe.failed=$true
  if ($scope -and $scope.failure_stage) { $probe.stage=$scope.failure_stage }
  $probe.code=$_.Exception.HResult
}
@{pid=$owner; focus=$focus; probe=$probe} | ConvertTo-Json -Compress
"""

# Independent, read-only failure census. A native caption plus PID can tell a
# missing dialog from one that UIA did not expose. It never authorizes a press.
WINDOWS_NATIVE_CENSUS = r"""
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class IbisUpdateCensus {
  private delegate bool Visitor(IntPtr window, IntPtr data);
  [DllImport("user32.dll")] private static extern bool EnumWindows(Visitor visitor, IntPtr data);
  [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode, ExactSpelling=true)]
  private static extern int GetWindowTextW(IntPtr window, StringBuilder text, int count);
  [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
  [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
  public static Dictionary<string,int> Read(int[] ids) {
    var result = new Dictionary<string,int>();
    foreach (var key in new string[]{"windows","offers","visible_offers","foreground_offers","pickers",
      "visible_pickers","foreground_pickers","owner_changes","bound_hit","unavailable"}) result[key]=0;
    var wanted = new HashSet<int>(ids);
    var foreground = GetForegroundWindow();
    bool complete = EnumWindows((window,data) => {
      uint pid, after;
      GetWindowThreadProcessId(window,out pid);
      if (!wanted.Contains((int)pid)) return true;
      if (++result["windows"] > 20) { result["bound_hit"]=1; return false; }
      var text = new StringBuilder(256);
      GetWindowTextW(window,text,text.Capacity);
      bool visible = IsWindowVisible(window);
      GetWindowThreadProcessId(window,out after);
      if (after != pid) { result["owner_changes"]++; return true; }
      var title = text.ToString();
      string kind = title == "An update is available" ? "offers" : title == "Ibis" ? "pickers" : null;
      if (kind != null) {
        result[kind]++;
        if (visible) result["visible_"+kind]++;
        if (foreground == window) result["foreground_"+kind]++;
      }
      return true;
    },IntPtr.Zero);
    if (!complete && result["bound_hit"] == 0) result["unavailable"]=1;
    return result;
  }
}
'@
$ids = @($env:IBIS_PIDS -split ',' | Where-Object { $_ } | Select-Object -Unique)
if ($ids.Count -gt 20) { throw 'Native census process bound exceeded' }
foreach ($id in $ids) { if ($id -notmatch '^[1-9][0-9]{0,9}$') { throw 'Invalid native census process' } }
[IbisUpdateCensus]::Read([int[]]$ids) | ConvertTo-Json -Compress
"""

MACOS_NATIVE = Path(__file__).with_name("update-macos.js").read_text(encoding="utf-8")
MACOS_PICKER = MACOS_NATIVE + r"""
function run(pids) {
  const result = macosUpdate(pids, false);
  return JSON.stringify({pid: result.pid, focus: result.focus, probe: result.probe});
}
"""
MACOS_LOOK = MACOS_NATIVE + r"""
function run(argv) {
  const [press, ...pids] = argv;
  const result = macosUpdate(pids, press !== '');
  return JSON.stringify({pressed: result.pressed, seen: result.seen,
    probe: Object.assign(result.probe, {pid: result.pid, focus: result.focus})});
}
"""


class PickerObservationFailure(AssertionError):
    def __init__(self, probe):
        self.probe = probe
        self.retryable = probe["stage"] not in ("scope", "unique", "picker-owner", "picker-bounds", "picker-unique", "picker-search-unique", "offer-recheck", "ax-press") and probe.get("windows", 0) <= 20
        super().__init__(f"native picker observation unavailable ({probe['stage']}, code {probe.get('code')})")


def picker_result(stdout, pids):
    result = json.loads(stdout)
    if not isinstance(result, dict) or set(result) not in ({"pid", "focus"}, {"pid", "focus", "probe"}):
        raise AssertionError("picker helper returned an unexpected answer")
    if "probe" in result:
        probe = result["probe"]
        stages = {"scope", "frameworks", "cg-copy", "cg-bridge", "cg-decode", "cg-windows", "cg-window-title", "unique", "ax-system", "ax-owner", "offer-recheck", "ax-press",
                  "ax-owner-read", "ax-app", "ax-title-decode", "complete"}
        stages.update(("picker-frameworks", "picker-native-focus", "picker-windows", "picker-owner", "picker-bounds", "picker-unique", "picker-search", "picker-search-unique", "picker-foreground"))
        stages.update(f"ax-{kind}-{step}" for kind in ("focused", "window", "title") for step in ("copy", "read", "bridge"))
        stages.update(f"ax-{kind}-{step}" for kind in ("windows", "sheets", "role", "parent", "value", "children", "enabled") for step in ("copy", "read", "bridge"))
        base = {"stage", "code", "failed", "trusted", "windows", "pickers", "offers"}
        macos = {"unnamed", "sheets", "action_matches", "attempted"}
        if (not isinstance(probe, dict) or set(probe) not in (base, base | macos)
                or type(probe["stage"]) is not str or probe["stage"] not in stages or type(probe["failed"]) is not bool
                or (probe["code"] is not None and (type(probe["code"]) is not int or not -2147483648 <= probe["code"] <= 2147483647))
                or (probe["trusted"] is not None and type(probe["trusted"]) is not bool)
                or any(type(probe[key]) is not int or not 0 <= probe[key] <= 21 for key in ("windows", "pickers", "offers"))
                or (macos.issubset(probe) and (type(probe["attempted"]) is not bool
                    or any(type(probe[key]) is not int or not 0 <= probe[key] <= 21 for key in ("unnamed", "sheets", "action_matches"))))):
            raise AssertionError("picker helper returned an unexpected probe")
        if probe["failed"]:
            raise PickerObservationFailure(probe)
    owner = result["pid"]
    focus = result["focus"]
    if owner is None and focus is None:
        return None
    if type(owner) is not int or owner not in pids or focus not in ("picker", "offer"):
        raise AssertionError("picker helper returned an unowned process")
    return owner, focus


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
        raise SystemExit("Ibis Nightly settings already exist; use a disposable account")
    return seed_settings(config, library)


def environment(inherited, config):
    env = dict(inherited)
    if platform.system() == "Windows":
        # Precise observer mode: an inherited disable flag overrides force in
        # WebView2. Product browser arguments stay in the app configuration.
        env["WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"] = "--force-renderer-accessibility"
    if config is not None:
        env.update(XDG_CONFIG_HOME=str(config), NO_AT_BRIDGE="0", GTK_MODULES="gail:atk-bridge")
        probe = config / "update-gtk-probe.so"
        if probe.is_file():
            env["GTK_MODULES"] += ":" + str(probe.resolve())
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
    choice = saved.get("library") if isinstance(saved, dict) else None
    same_library = False
    if isinstance(choice, dict) and set(choice) == {"bibtex"} and isinstance(choice["bibtex"], str):
        try:
            selected = Path(choice["bibtex"])
            # Both nightlies persist Rust's canonical path during startup:
            # /var -> /private/var on macOS, and a verbatim prefix on Windows.
            # Require the same native file, not the original path spelling or
            # merely equal contents. Missing/unreadable identity fails closed.
            same_library = selected.is_absolute() and library.is_file() and selected.samefile(library)
        except (OSError, ValueError):
            pass
    if not same_library:
        raise AssertionError("the synthetic library choice was not preserved")
    if saved.get("shortcut") != SHORTCUT:
        raise AssertionError("the synthetic shortcut choice was not preserved")
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


def linux_process_roles(pids, proc=Path("/proc")):
    """APPIMAGE is inherited by WebKit children; count roles, never log paths."""
    roles = {"app": 0, "webkit_web": 0, "webkit_network": 0, "other": 0, "unavailable": 0}
    labels = {"ibis-nightly": "app", "ibis-desktop": "app", "WebKitWebProcess": "webkit_web",
              "WebKitNetworkProcess": "webkit_network"}
    for pid in pids:
        try:
            name = Path(os.readlink(proc / str(pid) / "exe")).name
            roles[labels.get(name, "other")] += 1
        except OSError:
            roles["unavailable"] += 1
    return roles


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
        raise AssertionError("the setup executable installed no app")
    return found[0]


def wait_for(predicate, timeout, message, step=1.0, retry_observation=False):
    deadline = time.monotonic() + timeout
    unavailable = None
    while time.monotonic() < deadline:
        try:
            result = predicate()
            unavailable = None
        except PickerObservationFailure as error:
            if not retry_observation or not error.retryable:
                raise
            unavailable, result = error, None
        if result and time.monotonic() < deadline:
            return result
        time.sleep(min(step, max(0, deadline - time.monotonic())))
    if unavailable is not None:
        raise unavailable
    raise AssertionError(message() if callable(message) else message)


def remaining(deadline, limit=10):
    value = deadline - time.monotonic()
    if value <= 0:
        raise AssertionError("update qualification deadline expired")
    return min(limit, value)


def feed_version(url="https://github.com/ibiscite/ibis/releases/download/nightly/latest.json"):
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            return json.load(response).get("version")
    except Exception as error:  # Only for the failure report.
        return f"unavailable ({type(error).__name__})"


def automatic_check_facts(path, expected):
    """Availability is not presentation. Retain only fixed outcome counts."""
    facts = {"expected_available": 0, "other_available": 0, "up_to_date": 0, "failed": 0}
    try:
        with path.open("rb") as source:
            contents = source.read(1024 * 1024 + 1)
        if len(contents) > 1024 * 1024:
            return dict(facts, unavailable=True)
        for line in contents.decode("utf-8", errors="replace").splitlines():
            if line == f"ibis: automatic update check: {expected} is available":
                facts["expected_available"] += 1
            elif re.fullmatch(r"ibis: automatic update check: [0-9A-Za-z.+-]+ is available", line):
                facts["other_available"] += 1
            elif line == "ibis: automatic update check: up to date":
                facts["up_to_date"] += 1
            elif re.fullmatch(r"ibis: automatic update check failed: (network|no update file|no package for this system|unreadable update file|other)", line):
                facts["failed"] += 1
    except OSError:
        return dict(facts, unavailable=True)
    return facts


class LinuxActivationFailure(AssertionError):
    def __init__(self, probe, stage):
        self.probe = dict(probe, stage=stage)
        super().__init__(f"Linux update accessibility unavailable ({stage})")


class Linux:
    """The AppImage is copied into the test folder; the updater replaces that file."""

    def __init__(self, folder, args, library):
        self.config = folder / "config"
        self.settings = seed_settings(self.config, library)
        self.appimage = folder / "Ibis Nightly.AppImage"
        shutil.copyfile(args.previous, self.appimage)
        self.appimage.chmod(0o755)
        self.before, self.expected = digest(self.appimage), digest(args.expected)
        if self.before == self.expected:
            raise AssertionError("the previous and the new AppImage are identical")
        self.command = [str(self.appimage)]
        self.wm = None
        deadline = time.monotonic() + PICKER_TIMEOUT
        if self.supporting_window(deadline, "-root") is not None:
            raise AssertionError("the update desktop already has a window manager")
        try:
            self.wm = subprocess.Popen(["openbox", "--sm-disable"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            wait_for(lambda: self.window_manager_ready(deadline), max(0, deadline - time.monotonic()),
                     "the owned Linux window manager did not become ready", step=.1)
            self.prepare_control_probe()
        except BaseException:
            try:
                self.close()
            except (OSError, subprocess.SubprocessError):
                print("owned Linux window manager cleanup failed", file=sys.stderr)
            raise

    def prepare_control_probe(self):
        try:
            result = subprocess.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-shared", "-fPIC",
                                     str(Path(__file__).with_name("update-gtk-probe.c")), "-ldl", "-o",
                                     str(self.config / "update-gtk-probe.so")], capture_output=True, timeout=30)
            helper_stdout(result, "GTK control observer build")
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
            raise LinuxActivationFailure(helper_failure("linux", error), "gtk-probe-build") from None

    def install_control(self, scope, deadline):
        try:
            result = subprocess.run(["xprop", "-id", scope[2], "_IBIS_UPDATE_INSTALL_CONTROL"],
                                    capture_output=True, timeout=remaining(deadline), env=dict(os.environ, LC_ALL="C"))
            text = helper_stdout(result, "GTK control observer")
            match = re.fullmatch(r"_IBIS_UPDATE_INSTALL_CONTROL\(CARDINAL\) = ([0-9]+(?:, [0-9]+){6})\s*", text)
            if not match:
                return None  # Module is not loaded or has not observed this dialog yet.
            version, pid, installs, focused, ordinal, serial, valid = map(int, match[1].split(", "))
            if version != 1 or pid != scope[0] or installs != 1 or valid != 1:
                raise LinuxActivationFailure({"attempted": False}, "install-control-scope")
            if focused not in (0, 1, 2) or not 0 <= ordinal <= 128 or not 0 < serial <= 0xffffffff:
                raise LinuxActivationFailure({"attempted": False}, "install-control-scope")
            return focused == 1, ordinal, serial
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
            raise LinuxActivationFailure(helper_failure("linux", error), "install-control-observation") from None

    def x11_property(self, deadline, *args):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise AssertionError("Linux window manager observation timed out")
        try:
            result = subprocess.run(["xprop", *args], capture_output=True, timeout=remaining,
                                    env=dict(os.environ, LC_ALL="C"))
            if result.returncode or result.stderr or len(result.stdout) > 256:
                raise ValueError("invalid property answer")
            return result.stdout.decode("utf-8", errors="strict").strip()
        except (OSError, ValueError, subprocess.SubprocessError):
            raise AssertionError("Linux window manager property unavailable") from None

    def supporting_window(self, deadline, *target):
        text = self.x11_property(deadline, *target, "_NET_SUPPORTING_WM_CHECK")
        if text in ("_NET_SUPPORTING_WM_CHECK:  no such atom on any window.",
                    "_NET_SUPPORTING_WM_CHECK:  not found."):
            return None
        match = re.fullmatch(r"_NET_SUPPORTING_WM_CHECK\(WINDOW\): window id # (0x[0-9a-fA-F]{1,8})", text)
        if not match or int(match[1], 16) == 0:
            raise AssertionError("Linux window manager identity unavailable")
        return match[1]

    def window_manager_ready(self, deadline):
        if self.wm.poll() is not None:
            raise AssertionError("the owned Linux window manager exited")
        window = self.supporting_window(deadline, "-root")
        if window is None:
            return False
        back = self.supporting_window(deadline, "-id", window)
        if back is None:
            return False
        if back != window:
            raise AssertionError("Linux window manager identity changed")
        # Openbox publishes its own PID on the root, before its EWMH hints.
        text = self.x11_property(deadline, "-root", "_OPENBOX_PID")
        owner = re.fullmatch(r"_OPENBOX_PID\(CARDINAL\) = ([1-9][0-9]{0,9})", text)
        if not owner or int(owner[1]) != self.wm.pid or self.wm.poll() is not None:
            raise AssertionError("Linux window manager has no live owned process")
        self.wm_window = window
        return True

    def instances(self):
        return instances(self.appimage)

    def updated(self):
        return self.appimage.exists() and digest(self.appimage) == self.expected

    def facts(self):
        return {"appimage_matches_new_package": self.updated(), "process_roles": linux_process_roles(self.instances())}

    def picker_ready(self, pids, *, timeout=10):
        # Native input focus resolves to the owning top-level via WM_CLASS;
        # AT-SPI focus alone is stale. The desktop must manage focus normally.
        self.native_scope = None
        self.picker_probe = probe = {"stage": "scope", "pids": len(pids), "windows": 0, "pickers": 0, "offers": 0,
                                     "visible_pickers": 0, "visible_offers": 0, "focus": "not-observed"}
        budget = min(5, timeout)
        deadline = time.monotonic() + budget
        def query(*args, program="xdotool"):
            # Float rounding can make deadline - now exceed the budget by a hair.
            remaining = min(budget, deadline - time.monotonic())
            if remaining <= 0:
                raise PickerObservationFailure(dict(probe))
            try:
                result = subprocess.run([program, *args], capture_output=True, timeout=remaining,
                                        env=dict(os.environ, LC_ALL="C"))
                if result.returncode == 1 and args[0] == "search" and not result.stdout and not result.stderr:
                    return ""
                return helper_stdout(result, "X11 picker").strip()
            except (OSError, ValueError, RuntimeError, subprocess.SubprocessError):
                raise PickerObservationFailure(dict(probe, stage="x11-helper-" + probe["stage"])) from None
        if len(pids) > 20 or any(type(pid) is not int or not 0 < pid < 10000000000 for pid in pids):
            raise AssertionError("picker process bound exceeded")
        windows, offers, count = set(), set(), 0
        for pid in sorted(pids):
            probe["stage"] = "search"
            # Keep hidden main/offer counts too. --onlyvisible alone cannot
            # distinguish a never-created picker from one dismissed on blur.
            all_windows = query("search", "--pid", str(pid)).splitlines()
            visible = set(query("search", "--onlyvisible", "--pid", str(pid)).splitlines())
            if not visible.issubset(all_windows):
                # An offer can map between the two read-only searches. Discard
                # this whole observation; the next poll must prove fresh focus.
                probe["stage"] = "census-changed"
                return None
            # As in native smoke, read management after mapping. An offer
            # arriving since the previous observation must get a fresh list.
            probe["stage"] = "managed-clients"
            managed = managed_client_windows(query("-root", "_NET_CLIENT_LIST", program="xprop"))
            if managed is None:
                raise PickerObservationFailure(dict(probe))
            for window in all_windows:
                count += 1
                probe["windows"] = count
                if not re.fullmatch(r"[1-9][0-9]{0,9}", window) or count > 20:
                    raise AssertionError("picker window bound exceeded")
                probe["stage"] = "owner"
                if query("getwindowpid", window) != str(pid):
                    raise AssertionError("picker window owner changed")
                probe["stage"] = "title"
                name = query("getwindowname", window)
                if name not in ("Ibis", "An update is available"):
                    continue
                kind = "pickers" if name == "Ibis" else "offers"
                probe[kind] += 1
                if window not in visible:
                    continue
                probe["visible_" + kind] += 1
                # The same exact-title, PID and managed-client facts used by
                # smoke-native.py. Unmanaged GTK auxiliaries cannot qualify.
                probe["stage"] = "managed-window"
                if int(window) not in managed:
                    raise AssertionError("the visible picker or update offer is not a managed client")
                (windows if kind == "pickers" else offers).add((pid, window))
        if len(windows) > 1 or len(offers) > 1:
            raise AssertionError("picker or update offer is ambiguous")
        probe["stage"] = "focus"
        targets = {window: "picker" for _, window in windows}
        targets.update({window: "offer" for _, window in offers})
        focused = query("getwindowfocus") if targets else None
        probe["focus"] = targets.get(focused, "other") if targets else "not-observed"
        if not windows:
            return None
        pid, window = windows.pop()
        targets = {window: ("picker", "Ibis")}
        offer = None
        if offers:
            offer_pid, offer = offers.pop()
            if offer_pid != pid:
                raise AssertionError("update offer owner differs from picker")
            targets[offer] = ("offer", "An update is available")
        if focused not in targets:
            return None
        # A closed/replaced window cannot lend its earlier focus to a retry.
        probe["stage"] = "recheck"
        if query("getwindowname", window) != "Ibis" or query("getwindowpid", window) != str(pid):
            raise AssertionError("picker window changed during focus observation")
        focus, title = targets[focused]
        if query("getwindowname", focused) != title or query("getwindowpid", focused) != str(pid):
            raise AssertionError("focused window changed during observation")
        probe["stage"] = "complete"
        self.native_scope = (pid, window, offer, focused)
        return pid, focus

    def look(self, pids, press=None, *, timeout=10):
        # rfd's GTK dialog exists as a managed X11 client even when AT-SPI
        # exposes no dialog at all. The oracle is the native window and owner,
        # followed by GTK focused-control identity and replacement/restart,
        # not an accessibility provider or a presumed button order.
        if press not in (None, BUTTON):
            raise AssertionError("unexpected Linux update action")
        deadline = time.monotonic() + timeout
        ready = self.picker_ready(pids, timeout=remaining(deadline))
        scope = self.native_scope
        probe = dict(self.picker_probe, attempted=False, pressed_by=None)
        if not press or ready is None or ready[1] != "offer":
            return False, [], probe
        # Discard the earlier census, including its focus. A replaced dialog
        # B cannot consume a decision made for A in the same process.
        current = self.picker_ready(pids, timeout=remaining(deadline))
        if current != ready or self.native_scope != scope:
            raise LinuxActivationFailure(probe, "offer-recheck")
        def key(name, attempted):
            probe.update(stage="x11-key", attempted=attempted)
            try:
                # Use the same server keyboard path as native smoke. --window
                # uses directed XSendEvent instead of normal keyboard input;
                # its successful exit does not prove GTK activated a button.
                # The owned native focus/control checks above select the target.
                result = subprocess.run(["xdotool", "key", "--clearmodifiers", name],
                                        capture_output=True, timeout=remaining(deadline), env=dict(os.environ, LC_ALL="C"))
                if result.returncode or result.stdout or result.stderr:
                    raise RuntimeError("native update key was refused or its outcome is unknown")
            except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
                probe.update(helper_failure("linux", error))
                raise LinuxActivationFailure(probe, probe["stage"]) from None
        # Old rfd offers put Install first; the safe-default offer puts Later
        # first. Observe the actual GTK focus, not a presumed tab position.
        previous, tabs = None, 0
        while True:
            if time.monotonic() >= deadline:
                raise LinuxActivationFailure(probe, "install-control-unavailable")
            control = self.install_control(scope, deadline)
            if self.picker_ready(pids, timeout=remaining(deadline)) != ready or self.native_scope != scope:
                raise LinuxActivationFailure(probe, "offer-recheck")
            if control is None or (previous is not None and (control[2] == previous[2] or control[1] == previous[1])):
                time.sleep(min(.05, remaining(deadline)))
                continue
            if control[0]:
                break
            if tabs >= 6:
                raise LinuxActivationFailure(probe, "install-control-unavailable")
            previous = control
            key("Tab", False)
            tabs += 1
        # Prove the in-process observer is still live, even if Install was
        # already focused in the old layout. A stale property cannot consent.
        confirmed = self.install_control(scope, deadline)
        while confirmed is not None and confirmed[2] == control[2]:
            if time.monotonic() >= deadline:
                raise LinuxActivationFailure(probe, "install-control-unavailable")
            time.sleep(min(.05, remaining(deadline)))
            confirmed = self.install_control(scope, deadline)
        if confirmed is None or confirmed[:2] != control[:2]:
            raise LinuxActivationFailure(probe, "install-control-changed")
        if self.picker_ready(pids, timeout=remaining(deadline)) != ready or self.native_scope != scope:
            raise LinuxActivationFailure(probe, "offer-recheck")
        final = self.install_control(scope, deadline)
        if final is None or final[:2] != confirmed[:2]:
            raise LinuxActivationFailure(probe, "install-control-changed")
        key("space", True)
        return True, [], dict(probe, stage="complete", pressed_by="x11-install-space")

    def close(self):
        manager = getattr(self, "wm", None)
        if manager is not None and manager.poll() is None:
            manager.terminate()
            try:
                manager.wait(timeout=5)
            except subprocess.TimeoutExpired:
                manager.kill()
                manager.wait(timeout=5)
        self.wm = None


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
            subprocess.run(["tar", "-xzf", str(archive.resolve()), "-C", str(target)], check=True, timeout=300)
        self.bundle = only_app(applications)
        executable, self.previous_version = bundle_executable(self.bundle)
        new_executable, self.expected_version = bundle_executable(only_app(expected))
        self.expected = digest(new_executable)
        if digest(executable) == self.expected:
            raise AssertionError("the previous and the new app are identical")
        self.command = [str(executable)]

    def instances(self):
        ps = subprocess.run(["ps", "-axww", "-o", "pid=,args="], capture_output=True, text=True, check=True, timeout=10)
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

    def picker_ready(self, pids, *, timeout=10):
        self.picker_probe = {"stage": "macos-helper", "code": None}
        try:
            result = subprocess.run(["osascript", "-l", "JavaScript", "-e", MACOS_PICKER, *map(str, pids)],
                                    capture_output=True, timeout=timeout)
            stdout = helper_stdout(result, "macOS CG/AX picker")
            ready = picker_result(stdout, pids)
            self.picker_probe = json.loads(stdout).get("probe")
            return ready
        except PickerObservationFailure as error:
            self.picker_probe = error.probe
            raise
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
            self.picker_probe = helper_failure("macos", error)
            raise PickerObservationFailure(self.picker_probe) from None

    def look(self, pids, press=None, *, timeout=60):
        if press not in (None, BUTTON):
            raise AssertionError("unexpected macOS update action")
        probe = None
        try:
            result = subprocess.run(["osascript", "-l", "JavaScript", "-e", MACOS_LOOK, press or "", *map(str, pids)],
                                    capture_output=True, timeout=timeout)
            answer = look_result(helper_stdout(result, "macOS AX action"))
            pressed, _, probe = answer
            if not isinstance(probe, dict) or not {"pid", "focus"}.issubset(probe):
                raise ValueError("missing macOS native facts")
            observation = {"pid": probe["pid"], "focus": probe["focus"],
                           "probe": {key: value for key, value in probe.items() if key not in ("pid", "focus")}}
            ready = picker_result(json.dumps(observation), pids)
            if (type(probe.get("attempted")) is not bool or probe["attempted"] != pressed or probe["stage"] != "complete" or probe["code"] is not None
                    or (pressed and (not press or ready is None or ready[1] != "offer"
                                     or probe["trusted"] is not True or probe["windows"] < 1
                                     or probe["pickers"] != 1 or probe["offers"] != 1 or probe["action_matches"] != 1))):
                raise ValueError("invalid macOS native action facts")
            return answer
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, AssertionError) as error:
            if isinstance(error, PickerObservationFailure):
                probe = error.probe
            else:
                probe = helper_failure("macos", error)
            raise MacOSActivationFailure(probe) from None

    def close(self):
        clean_settings(self.settings)


class MacOSActivationFailure(AssertionError):
    def __init__(self, probe):
        self.probe = probe
        super().__init__("macOS update activation failed or its outcome is unknown")


class WindowsActivationFailure(AssertionError):
    def __init__(self, probe):
        super().__init__("Windows update activation failed or its outcome is unknown")
        flags = {"attempted", "blocked", "invoke_available", "enabled", "on_screen", "tree_complete"}
        counts = {"pids", "windows", "elements", "offer_matches", "action_matches", "root_windows", "nested_windows"}
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
                         timeout=10, IBIS_EXE=str(self.executable))
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
                "installed_version": self.installed_version(), "executable_changed": changed,
                "native_windows": self.native_windows()}

    def native_windows(self):
        fields = {"windows", "offers", "visible_offers", "foreground_offers", "pickers", "visible_pickers",
                  "foreground_pickers", "owner_changes", "bound_hit", "unavailable"}
        try:
            facts = json.loads(powershell(WINDOWS_NATIVE_CENSUS, timeout=10, IBIS_PIDS=",".join(map(str, self.instances()))))
            if (isinstance(facts, dict) and set(facts) == fields
                    and all(type(value) is int and 0 <= value <= 21 for value in facts.values())):
                return facts
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError):
            pass
        return {"unavailable": True}

    def picker_ready(self, pids, *, timeout=10):
        try:
            stdout = powershell(WINDOWS_PICKER, timeout=timeout, IBIS_PIDS=",".join(map(str, pids)))
            ready = picker_result(stdout, pids)
            self.picker_probe = json.loads(stdout).get("probe")
            return ready
        except PickerObservationFailure as error:
            self.picker_probe = error.probe
            raise
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
            self.picker_probe = helper_failure("windows", error)
            raise PickerObservationFailure(self.picker_probe) from None

    def look(self, pids, press=None, *, timeout=120):
        probe = None
        try:
            result = look_result(powershell(WINDOWS_LOOK, timeout=timeout, IBIS_PIDS=",".join(map(str, pids)), IBIS_PRESS=press or ""))
            pressed, _, probe = result
            if press:
                flags = ("attempted", "blocked", "invoke_available", "enabled", "on_screen", "tree_complete")
                limits = {"pids": 20, "windows": 20, "elements": 4000, "offer_matches": 1, "action_matches": 1,
                          "root_windows": 20, "nested_windows": 20}
                valid = (isinstance(probe, dict) and all(type(probe.get(key)) is bool for key in flags)
                         and all(type(probe.get(key)) is int and 0 <= probe[key] <= limit for key, limit in limits.items())
                         and probe.get("errors") == [] and not probe["blocked"] and probe["attempted"] == pressed
                         and probe["tree_complete"] and probe["windows"] == probe["root_windows"] + probe["nested_windows"])
                if not valid or (pressed and (not all(probe[key] for key in ("enabled", "on_screen"))
                                              or any(probe[key] < 1 for key in ("pids", "windows", "elements", "offer_matches", "action_matches", "root_windows"))
                                              or probe.get("pressed_by") != ("invoke" if probe["invoke_available"] else "task-dialog"))) \
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


class HelperProcessFailure(RuntimeError):
    def __init__(self, name, result, stderr):
        self.returncode = result.returncode
        super().__init__(f"{name} helper failed: {stderr.strip()[-500:]}")


def helper_failure(system, error):
    # The old picker-helper/code None wrapper erased this distinction. Keep
    # only fixed operation names and numeric status, never native stderr.
    kind, code = "unavailable", None
    if isinstance(error, subprocess.TimeoutExpired):
        kind = "timeout"
    elif isinstance(error, OSError):
        kind = "launch"
        number = getattr(error, "winerror", None)
        if number is None:
            number = error.errno
        if type(number) is int and -2147483648 <= number <= 2147483647:
            code = number
    elif isinstance(error, HelperProcessFailure):
        kind, code = "exit", error.returncode
    elif isinstance(error, ValueError):
        kind = "decode"
    return {"stage": f"{system}-helper-{kind}", "code": code}


def helper_stdout(result, name):
    """Decode in this thread: Windows text-pipe readers can lose decode errors."""
    stdout = result.stdout.decode("utf-8", errors="strict")
    stderr = result.stderr.decode("utf-8", errors="strict")
    if result.returncode:
        raise HelperProcessFailure(name, result, stderr)
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
    report = {"status": "failed", "system": platform.system(), "steps": [], "picker_observations": [],
              "harness_sha256": digest(Path(__file__))}
    if report["system"] == "Windows":
        report["observer_mode"] = "forced-renderer-accessibility"
    if report["system"] == "Darwin":
        report["macos_provider_sha256"] = digest(Path(__file__).with_name("update-macos.js"))
    if report["system"] == "Linux":
        report["linux_control_provider_sha256"] = digest(Path(__file__).with_name("update-gtk-probe.c"))
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
                launched_at = time.monotonic()

                def observe_picker(pids, phase, deadline):
                    sample = {"phase": phase, "processes": len(pids), "ready": False, "focus": None}
                    try:
                        ready = install.picker_ready(pids, timeout=remaining(deadline))
                        if ready is None:
                            return None
                        if not isinstance(ready, tuple) or len(ready) != 2:
                            raise AssertionError("the reopened picker returned invalid focus facts")
                        owner, focus = ready
                        if type(owner) is not int or owner not in pids or focus not in ("picker", "offer"):
                            raise AssertionError("the reopened picker has no original process owner")
                        sample.update(ready=True, focus=focus)
                        return ready
                    except PickerObservationFailure as error:
                        sample["probe"] = error.probe
                        raise
                    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
                        sample["probe"] = helper_failure("picker", error)
                        raise PickerObservationFailure(sample["probe"]) from None
                    finally:
                        probe = getattr(install, "picker_probe", None)
                        if isinstance(probe, dict) and "probe" not in sample:
                            sample["probe"] = probe.copy()
                        if isinstance(install, Linux):
                            sample["process_roles"] = linux_process_roles(pids)
                        elapsed = round((time.monotonic() - launched_at) * 1000)
                        observations = report["picker_observations"]
                        last = observations[-1] if observations else None
                        if last and {key: value for key, value in last.items() if key not in ("first_ms", "last_ms", "samples")} == sample:
                            last.update(last_ms=elapsed, samples=last["samples"] + 1)
                        elif len(observations) < 128:
                            observations.append(dict(sample, first_ms=elapsed, last_ms=elapsed, samples=1))
                        else:
                            report["picker_observations_omitted"] = report.get("picker_observations_omitted", 0) + 1

                startup_deadline = time.monotonic() + PICKER_TIMEOUT
                wait_for(install.instances, remaining(startup_deadline, PICKER_TIMEOUT), "the previous nightly did not start")
                # Let first-launch registration settle, then issue Show exactly
                # once through the same owned command. The child must exit;
                # a second persistent instance is never a successful reopen.
                time.sleep(remaining(startup_deadline, 5))
                initial = install.instances()
                if not initial:
                    raise AssertionError("the previous nightly exited before its reopen")
                # Observe without repairing focus or opening another window.
                # This separates a missing startup picker from a reopen blur.
                # A valid absent answer is diagnostic. Only unavailable
                # observations retry, within the original startup deadline.
                def before_reopen():
                    pids = initial & install.instances()
                    if not pids:
                        raise AssertionError("the previous nightly exited before its reopen")
                    return True, observe_picker(pids, "before-reopen", startup_deadline)
                wait_for(before_reopen, remaining(startup_deadline, PICKER_TIMEOUT),
                         "the startup picker could not be observed", retry_observation=True)
                reopen = subprocess.Popen(install.command, env=environment(os.environ, install.config),
                                          stdout=log, stderr=log, cwd=folder)
                started.append(reopen)
                if reopen.wait(timeout=15) != 0:
                    raise AssertionError("the previous nightly refused its single-instance reopen")
                report["steps"].append("single-instance reopen exited successfully")
                picker_deadline = time.monotonic() + PICKER_TIMEOUT
                def picker_ready():
                    pids = initial & install.instances()
                    return observe_picker(pids, "after-reopen", picker_deadline)
                picker, focus = wait_for(picker_ready, remaining(picker_deadline, PICKER_TIMEOUT),
                                         "the previous nightly's owned picker was not visible with picker or update-offer focus",
                                         retry_observation=True)
                report["reopen_focus"] = focus
                report["steps"].append("previous nightly reopened once with its owned picker visible and "
                                       + ("picker focused" if focus == "picker" else "owned update offer focused"))

                # The processes running just before the press, so helper
                # processes the old version starts later never count as new.
                seen["old"] = set()
                seen["native_offer"] = False
                offer_deadline = time.monotonic() + OFFER_TIMEOUT
                def offered():
                    seen["old"] = install.instances()
                    if picker not in seen["old"]:
                        raise AssertionError("the reopened picker exited before offering the update")
                    # A late offer can move focus after reopen was proved.
                    # Accept its owned modal focus only while the picker is
                    # still visible. Never reopen or activate to repair it.
                    ready = observe_picker({picker}, "before-activation", offer_deadline)
                    if ready is None:
                        return False
                    seen["native_offer"] |= ready[1] == "offer"
                    try:
                        pressed, _, seen["probe"] = install.look({picker}, BUTTON, timeout=remaining(offer_deadline))
                        return pressed
                    except (RuntimeError, ValueError, subprocess.TimeoutExpired) as problem:
                        seen["helper"] = type(problem).__name__
                        raise AssertionError("update activation helper failed or its outcome is unknown") from None
                wait_for(offered, remaining(offer_deadline, OFFER_TIMEOUT),
                         lambda: "the visible native update offer never supplied a unique owned action"
                         if seen["native_offer"] else "the previous nightly never offered the update",
                         step=2, retry_observation=True)
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
                report["error"] = str(error) if isinstance(error, AssertionError) else f"{type(error).__name__}: update qualification failed"
                if isinstance(error, PickerObservationFailure):
                    report["last_picker_probe"] = error.probe
                if isinstance(error, (WindowsActivationFailure, LinuxActivationFailure, MacOSActivationFailure)) and error.probe:
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
                        _, items, report["native_probe"] = install.look(pids)
                        # Names and text are unnecessary for diagnostics and
                        # can be private, regardless of the native provider.
                        report["native_items"] = len(items)
                    except Exception as problem:
                        report["native_observation_unavailable"] = type(problem).__name__
                report["published_feed"] = feed_version()
                report["automatic_checks"] = automatic_check_facts(args.output / "update-e2e-app.log",
                                                                  report.get("expected_version", report["published_feed"]))
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
                        print(f"cleanup incomplete: {type(problem).__name__}", file=sys.stderr)
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
    def macos_probe(self, *, attempted=False, **changes):
        return dict({"stage": "complete", "code": None, "failed": False, "trusted": True,
                     "windows": 2, "pickers": 1, "offers": 1, "unnamed": 1, "sheets": 1,
                     "action_matches": 1, "attempted": attempted, "pid": 42, "focus": "offer"}, **changes)

    def test_macos_success_requires_complete_owned_native_facts(self):
        for change in ({"pid": 99}, {"focus": "picker"}, {"attempted": False}, {"offers": 0}, {"offers": 2},
                       {"action_matches": 0}, {"failed": True}, {"pickers": 0}, {"windows": True}, {"windows": 0},
                       {"trusted": False}, {"stage": "ax-press"}, {"code": 1}, {"private": "synthetic-private-sentinel"}):
            with self.subTest(change=change), patch("subprocess.run") as helper:
                payload = {"pressed": True, "seen": [], "probe": self.macos_probe(**dict({"attempted": True}, **change))}
                helper.return_value = subprocess.CompletedProcess([], 0, stdout=json.dumps(payload).encode(), stderr=b"")
                with self.assertRaises(MacOSActivationFailure) as failure:
                    MacOS.__new__(MacOS).look({42}, BUTTON)
                self.assertNotIn("synthetic-private-sentinel", json.dumps(failure.exception.probe))
                helper.assert_called_once()

    def test_macos_refused_or_uncertain_press_preserves_fixed_attempt_facts(self):
        for stage in ("ax-press", "offer-recheck"):
            probe = self.macos_probe(attempted=stage == "ax-press", failed=True, stage=stage, pid=None, focus=None, code=-25204)
            payload = json.dumps({"pressed": False, "seen": [], "probe": probe}).encode()
            with self.subTest(stage=stage), patch("subprocess.run", return_value=subprocess.CompletedProcess([], 0, stdout=payload, stderr=b"")) as helper:
                with self.assertRaises(MacOSActivationFailure) as failure:
                    MacOS.__new__(MacOS).look({42}, BUTTON)
                self.assertEqual(failure.exception.probe["attempted"], stage == "ax-press")
                self.assertEqual(failure.exception.probe["stage"], stage)
                helper.assert_called_once()

    def linux_native_offer(self, scenario):
        native = Linux.__new__(Linux)
        clock = {"now": 0.0}
        events, reads, tabs, controls = [], 0, 0, 0
        def query(command, **kwargs):
            nonlocal reads, tabs, controls
            self.assertGreater(kwargs["timeout"], 0)
            events.append(tuple(command))
            if command == ["xprop", "-id", "102", "_IBIS_UPDATE_INSTALL_CONTROL"]:
                controls += 1
                focused = 1 if scenario == "old-layout" or tabs else 2
                if scenario == "changed-control" and controls > 2:
                    focused = 2
                if scenario == "missing-control":
                    return subprocess.CompletedProcess(command, 0, stdout=b"_IBIS_UPDATE_INSTALL_CONTROL: not found.", stderr=b"")
                if scenario == "wrong-control":
                    focused = 2
                pid = 99 if scenario == "foreign-control" else 42
                installs = 2 if scenario == "duplicate-control" else 1
                facts = f"_IBIS_UPDATE_INSTALL_CONTROL(CARDINAL) = 1, {pid}, {installs}, {focused}, {tabs + 1}, {controls}, 1"
                return subprocess.CompletedProcess(command, 0, stdout=facts.encode(), stderr=b"")
            args = tuple(command[1:])
            if args in (("key", "--clearmodifiers", "Tab"), ("key", "--clearmodifiers", "space")):
                if args[-1] == "Tab":
                    tabs += 1
                if scenario == "selection-timeout" and args[-1] == "Tab":
                    raise subprocess.TimeoutExpired(command, 1)
                if scenario == "uncertain" and args[-1] == "space":
                    raise subprocess.TimeoutExpired(command, 1)
                return subprocess.CompletedProcess(command, 1 if scenario == "refused" and args[-1] == "space" else 0, stdout=b"", stderr=b"")
            if args == ("search", "--pid", "42"):
                reads += 1
            if scenario == "io":
                raise OSError("synthetic-private-sentinel")
            offer = scenario != "absent"
            late = reads > 1
            offered_id = "103" if late and scenario == "reopened" else "102"
            ids = "101" + ("\n" + offered_id if offer else "") + ("\n103" if scenario == "duplicate" else "")
            if scenario == "hidden":
                visible = "101"
            else:
                visible = ids
            values = {("search", "--pid", "42"): ids,
                      ("search", "--onlyvisible", "--pid", "42"): visible,
                      ("-root", "_NET_CLIENT_LIST"): "_NET_CLIENT_LIST(WINDOW): window id # 0x65, 0x66, 0x67",
                      ("getwindowname", "101"): "Ibis",
                      ("getwindowname", "102"): "Other" if late and scenario == "renamed" else "An update is available",
                      ("getwindowname", "103"): "An update is available",
                      ("getwindowfocus",): "999" if scenario == "unfocused" or (late and scenario == "focus-lost") or (reads > 3 and scenario == "selection-focus-lost") else offered_id if offer else "101"}
            if args[0] == "getwindowpid":
                value = "99" if args[-1] == offered_id and (scenario == "foreign" or (late and scenario == "owner-changed")) else "42"
            else:
                self.assertIn(args, values, "activation must use only the verified native window, without AT-SPI")
                value = values[args]
            return subprocess.CompletedProcess(command, 0, stdout=value.encode(), stderr=b"")
        with patch("subprocess.run", side_effect=query), \
                patch("time.monotonic", side_effect=lambda: clock["now"]), \
                patch("time.sleep", side_effect=lambda step: clock.update(now=clock["now"] + step)):
            try:
                result = native.look({42}, BUTTON, timeout=3)
            except AssertionError as error:
                result = error
        return result, events

    def test_linux_native_offer_verifies_install_in_the_exact_window_once_without_atspi(self):
        result, events = self.linux_native_offer("success")
        self.assertNotIsInstance(result, Exception)
        self.assertTrue(result[0])
        self.assertEqual([event for event in events if event[1] == "key"],
                         [("xdotool", "key", "--clearmodifiers", "Tab"),
                          ("xdotool", "key", "--clearmodifiers", "space")])

    def test_linux_old_layout_verifies_install_without_tabbing_to_later(self):
        result, events = self.linux_native_offer("old-layout")
        self.assertNotIsInstance(result, Exception)
        self.assertTrue(result[0])
        self.assertEqual([event[-1] for event in events if event[1] == "key"], ["space"])
        self.assertTrue(any(event == ("xprop", "-id", "102", "_IBIS_UPDATE_INSTALL_CONTROL") for event in events))

    def test_linux_never_activates_an_unverified_or_wrong_control(self):
        for scenario in ("missing-control", "wrong-control", "foreign-control", "duplicate-control", "changed-control"):
            with self.subTest(scenario=scenario):
                result, events = self.linux_native_offer(scenario)
                self.assertIsInstance(result, LinuxActivationFailure)
                self.assertFalse(any(event[1] == "key" and event[-1] in ("space", "Return") for event in events))
                self.assertLessEqual(sum(event[1] == "key" for event in events), 6)

    def test_linux_offer_fake_does_not_consult_the_runner_clock(self):
        with patch("time.monotonic", side_effect=AssertionError("runner clock was consulted")):
            for scenario in ("success", "missing-control", "wrong-control", "io", "reopened"):
                with self.subTest(scenario=scenario):
                    result, events = self.linux_native_offer(scenario)
                    if scenario == "success":
                        self.assertNotIsInstance(result, Exception)
                        self.assertTrue(result[0])
                    elif scenario in ("missing-control", "wrong-control"):
                        self.assertIsInstance(result, LinuxActivationFailure)
                    else:
                        self.assertIsInstance(result, AssertionError)
                    if scenario != "success":
                        self.assertFalse(any(event[1] == "key" and event[-1] in ("space", "Return") for event in events))

    def test_linux_native_offer_refuses_absence_ambiguity_and_stale_or_foreign_window(self):
        for scenario in ("absent", "hidden", "unfocused", "duplicate", "foreign", "io", "reopened", "renamed", "focus-lost", "owner-changed"):
            with self.subTest(scenario=scenario):
                result, events = self.linux_native_offer(scenario)
                self.assertTrue(isinstance(result, AssertionError) or not result[0])
                self.assertFalse(any(event[1] == "key" for event in events))

    def test_linux_native_activation_refusal_or_uncertainty_never_sends_a_second_key(self):
        for scenario in ("refused", "uncertain"):
            with self.subTest(scenario=scenario):
                result, events = self.linux_native_offer(scenario)
                self.assertIsInstance(result, AssertionError)
                self.assertEqual(sum(event[1] == "key" and event[-1] == "space" for event in events), 1)

    def test_linux_selection_failure_or_lost_focus_never_activates_or_uses_return(self):
        for scenario in ("selection-timeout", "selection-focus-lost"):
            with self.subTest(scenario=scenario):
                result, events = self.linux_native_offer(scenario)
                self.assertIsInstance(result, AssertionError)
                self.assertEqual([event[-1] for event in events if event[1] == "key"], ["Tab"])

    def test_update_job_keeps_nightly_metadata_but_executes_the_current_qualification_tools(self):
        workflow = (Path(__file__).resolve().parents[1] / ".github/workflows/preview.yml").read_text(encoding="utf-8")
        job = workflow.split("\n  update-e2e:\n", 1)[1]
        steps = re.split(r"(?m)^      - name: ", job)[1:]
        checkouts = [step for step in steps if "uses: actions/checkout@" in step]
        # Artifact version calculation still runs in the publisher's checkout;
        # fixes to the qualification tools must not wait for another nightly.
        source = [step for step in checkouts if "ref: ${{ steps.source.outputs.sha }}" in step]
        harness = [step for step in checkouts if "ref: ${{ github.sha }}" in step]
        self.assertEqual(len(source), 1)
        self.assertNotRegex(source[0], r"(?m)^          path:")
        self.assertEqual(len(harness), 1, "the update job must check out its current qualification tools")
        path = re.search(r"(?m)^          path: ([a-z-]+)$", harness[0])
        self.assertIsNotNone(path)
        self.assertIn("persist-credentials: false", harness[0])
        commands = re.findall(r"(?m)^          .*?((?:[a-z-]+/)?scripts/update-e2e\.py) (.*)$", job)
        self.assertEqual(len(commands), 4, "both platforms run self-tests and the real qualifier")
        self.assertTrue(all(script == f"{path[1]}/scripts/update-e2e.py" for script, _ in commands))
        self.assertEqual(sum(arguments == "--self-test" for _, arguments in commands), 2)
        self.assertIn("version=$(node scripts/update-feed.mjs version)", job)
        self.assertEqual(job.count("run-id: ${{ steps.source.outputs.run }}"), 2)

    def linux_desktop(self, scenario):
        events = []
        clock = {"now": 0.0}
        manager = MagicMock(pid=77)
        manager.poll.return_value = None
        def stopped(*_args, **_kwargs):
            manager.poll.return_value = 0
            return 0
        manager.wait.side_effect = stopped
        root_reads = 0
        absent = "_NET_SUPPORTING_WM_CHECK:  no such atom on any window."
        support = "_NET_SUPPORTING_WM_CHECK(WINDOW): window id # 0x20011f"
        def query(command, **kwargs):
            nonlocal root_reads
            self.assertGreater(kwargs["timeout"], 0)
            self.assertLessEqual(kwargs["timeout"], PICKER_TIMEOUT)
            self.assertTrue(kwargs["capture_output"])
            events.append(tuple(command))
            args = tuple(command[1:])
            if args == ("-root", "_NET_SUPPORTING_WM_CHECK"):
                root_reads += 1
                value = support if scenario == "existing" or root_reads > 2 else absent
                if scenario == "never-ready": value = absent
                if scenario == "malformed": value = "synthetic-private-sentinel"
            elif args == ("-id", "0x20011f", "_NET_SUPPORTING_WM_CHECK"):
                value = support.replace("0x20011f", "0x999") if scenario == "identity" else support
            elif args == ("-root", "_OPENBOX_PID"):
                value = "_OPENBOX_PID(CARDINAL) = " + ("99" if scenario == "foreign" else "77")
            else:
                self.fail("unexpected desktop action")
            if scenario == "cancelled" and root_reads > 2:
                raise KeyboardInterrupt()
            if scenario == "query-error" or (scenario == "late-query-error" and root_reads > 2):
                return subprocess.CompletedProcess(command, 1, stdout=b"", stderr=b"synthetic-private-sentinel")
            if scenario == "query-timeout":
                raise subprocess.TimeoutExpired(command, kwargs["timeout"])
            return subprocess.CompletedProcess(command, 0, stdout=value.encode(), stderr=b"")
        def launch(command, **_kwargs):
            events.append(tuple(command))
            self.assertEqual(command, ["openbox", "--sm-disable"])
            if scenario == "start-error": raise OSError("synthetic start error")
            if scenario == "exited": manager.poll.return_value = 1
            return manager
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            previous, expected, library = (folder / name for name in ("previous", "expected", "synthetic.bib"))
            previous.write_bytes(b"synthetic previous"); expected.write_bytes(b"synthetic next")
            library.write_text("@article{synthetic, title={Synthetic}}")
            args = argparse.Namespace(previous=previous, expected=expected)
            with patch.object(Linux, "prepare_control_probe"), patch("subprocess.Popen", side_effect=launch), \
                    patch("subprocess.run", side_effect=query), patch("time.monotonic", side_effect=lambda: clock["now"]), \
                    patch("time.sleep", side_effect=lambda _step: clock.update(now=clock["now"] + 1)):
                if scenario == "success":
                    native = Linux(folder, args, library)
                    self.assertIs(native.wm, manager)
                    self.assertEqual(native.wm_window, "0x20011f")
                    self.assertEqual(events[0], ("xprop", "-root", "_NET_SUPPORTING_WM_CHECK"))
                    self.assertEqual(events[1], ("openbox", "--sm-disable"))
                    manager.terminate.assert_not_called()
                    native.close(); native.close()
                    manager.terminate.assert_called_once_with()
                    manager.wait.assert_called_once_with(timeout=5)
                else:
                    expected_error = KeyboardInterrupt if scenario == "cancelled" else (AssertionError, OSError, RuntimeError, subprocess.TimeoutExpired)
                    with self.assertRaises(expected_error) as failure:
                        Linux(folder, args, library)
                    self.assertNotIn("synthetic-private-sentinel", str(failure.exception))
                    started = ("openbox", "--sm-disable") in events and scenario != "start-error"
                    self.assertEqual(manager.terminate.call_count, int(started and scenario != "exited"))
                    self.assertLessEqual(clock["now"], PICKER_TIMEOUT)
        return events

    def test_linux_establishes_owned_window_manager_before_app_launch_and_cleans_it_once(self):
        self.linux_desktop("success")

    def test_linux_refuses_existing_unknown_failed_or_unready_desktop_before_app_actions(self):
        for scenario in ("existing", "malformed", "query-error", "query-timeout", "start-error", "exited",
                         "identity", "foreign", "never-ready", "late-query-error", "cancelled"):
            with self.subTest(scenario=scenario):
                self.linux_desktop(scenario)

    def test_linux_late_cleanup_of_old_desktop_never_stops_reopened_owner(self):
        first, second = Linux.__new__(Linux), Linux.__new__(Linux)
        first.wm, second.wm = MagicMock(pid=77), MagicMock(pid=88)
        old, current = first.wm, second.wm
        old.poll.return_value = current.poll.return_value = None
        first.close(); first.close()
        old.terminate.assert_called_once_with()
        current.terminate.assert_not_called()
        second.close()
        current.terminate.assert_called_once_with()

    def test_linux_kills_only_the_owned_manager_when_termination_does_not_finish(self):
        native = Linux.__new__(Linux)
        native.wm = MagicMock(pid=77)
        manager = native.wm
        manager.poll.return_value = None
        manager.wait.side_effect = [subprocess.TimeoutExpired("openbox", 5), 0]
        native.close(); native.close()
        manager.terminate.assert_called_once_with()
        manager.kill.assert_called_once_with()
        self.assertEqual(manager.wait.call_count, 2)

    @unittest.skipUnless(shutil.which("node"), "Node is required for the macOS provider contract")
    def test_macos_native_provider_contract(self):
        result = subprocess.run([shutil.which("node"), "--test", str(Path(__file__).with_name("update-macos.test.mjs"))],
                                capture_output=True, timeout=60)
        self.assertEqual(result.returncode, 0, (result.stdout + result.stderr).decode("utf-8", errors="replace"))

    def test_linux_picker_accepts_an_immediate_owned_offer_without_accepting_foreign_or_duplicate_modals(self):
        for scenario in ("offer", "foreign", "duplicate"):
            def observe(command, **_kwargs):
                args = command[1:]
                values = {("search", "--onlyvisible", "--pid", "42"): "101\n102" + ("\n103" if scenario == "duplicate" else ""),
                          ("search", "--pid", "42"): "101\n102" + ("\n103" if scenario == "duplicate" else ""),
                          ("-root", "_NET_CLIENT_LIST"): "_NET_CLIENT_LIST(WINDOW): window id # 0x65, 0x66, 0x67",
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
        self.assertIsNone(picker_result('{"pid":null,"focus":null}', {42}))
        self.assertEqual(picker_result('{"pid":42,"focus":"picker"}', {42}), (42, "picker"))
        self.assertEqual(picker_result('{"pid":42,"focus":"offer"}', {42}), (42, "offer"))
        for focus in (None, True, "unknown"):
            with self.subTest(focus=focus), self.assertRaises(AssertionError):
                picker_result(json.dumps({"pid": 42, "focus": focus}), {42})

    def test_macos_picker_launch_failure_and_bad_output_are_named_without_native_text(self):
        mac = MacOS.__new__(MacOS)
        for outcome in (FileNotFoundError("synthetic-private-sentinel"),
                        subprocess.TimeoutExpired("synthetic-private-sentinel", 10),
                        subprocess.CompletedProcess([], 1, stdout=b"", stderr=b"synthetic-private-sentinel"),
                        subprocess.CompletedProcess([], 0, stdout=b"synthetic-private-sentinel", stderr=b"")):
            with self.subTest(outcome=type(outcome).__name__), patch("subprocess.run") as helper:
                if isinstance(outcome, Exception):
                    helper.side_effect = outcome
                else:
                    helper.return_value = outcome
                with self.assertRaisesRegex(AssertionError, "macos-helper") as failure:
                    mac.picker_ready({42})
                self.assertNotIn("synthetic-private-sentinel", str(failure.exception))
                self.assertEqual(helper.call_count, 1)

    def test_linux_picker_requires_unique_owned_visible_main_and_native_input_focus(self):
        for scenario in ("success", "absent", "foreign", "duplicate", "wrong-title", "unfocused", "unknown", "closed", "replaced"):
            queries, names = [], 0
            def observed(command, **kwargs):
                nonlocal names
                self.assertGreater(kwargs["timeout"], 0)
                self.assertLessEqual(kwargs["timeout"], 5)
                args = command[1:]
                queries.append(args)
                if args == ["-root", "_NET_CLIENT_LIST"]:
                    value = "_NET_CLIENT_LIST(WINDOW): window id # 0x65, 0x66"
                elif args[0] == "search":
                    self.assertIn(args, (["search", "--onlyvisible", "--pid", "42"], ["search", "--pid", "42"]))
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
                    self.assertEqual(linux.picker_ready({42}), (42, "picker") if scenario == "success" else None)
            self.assertTrue(all(args[0] in ("-root", "search", "getwindowpid", "getwindowname", "getwindowfocus") for args in queries))

    def test_linux_picker_uses_the_smoke_managed_client_oracle(self):
        for scenario in ("settled", "unmanaged", "missing-helper", "hidden-by-offer"):
            def observed(command, **_kwargs):
                args = tuple(command[1:])
                if scenario == "missing-helper":
                    raise FileNotFoundError("synthetic-private-sentinel")
                values = {("search", "--onlyvisible", "--pid", "42"): "102" if scenario == "hidden-by-offer" else "101",
                          ("search", "--pid", "42"): "101\n102" if scenario == "hidden-by-offer" else "101",
                          ("-root", "_NET_CLIENT_LIST"): "_NET_CLIENT_LIST(WINDOW): window id # " + ("0x66" if scenario == "unmanaged" else "0x65, 0x66"),
                          ("getwindowname", "101"): "Ibis", ("getwindowname", "102"): "An update is available",
                          ("getwindowfocus",): "102" if scenario == "hidden-by-offer" else "101"}
                return subprocess.CompletedProcess(command, 0, stdout=values.get(args, "42").encode(), stderr=b"")
            with self.subTest(scenario=scenario), patch("subprocess.run", side_effect=observed):
                linux = Linux.__new__(Linux)
                if scenario in ("unmanaged", "missing-helper"):
                    with self.assertRaisesRegex(AssertionError, "managed|helper") as failure:
                        linux.picker_ready({42})
                    self.assertNotIn("synthetic-private-sentinel", str(failure.exception))
                else:
                    self.assertEqual(linux.picker_ready({42}), (42, "picker") if scenario == "settled" else None)
                    self.assertEqual(linux.picker_probe["visible_pickers"], int(scenario == "settled"))
                    self.assertEqual(linux.picker_probe["pickers"], 1)

    def test_linux_late_offer_during_census_cannot_borrow_earlier_picker_focus(self):
        phase = "picker"
        queries = []
        def observed(command, **_kwargs):
            args = tuple(command[1:])
            queries.append(tuple(command))
            values = {("-root", "_NET_CLIENT_LIST"): "_NET_CLIENT_LIST(WINDOW): window id # 0x65, 0x66",
                      ("search", "--pid", "42"): "101\n102" if phase == "offer" else "101",
                      ("search", "--onlyvisible", "--pid", "42"): "101" if phase == "picker" else "101\n102",
                      ("getwindowname", "101"): "Ibis", ("getwindowname", "102"): "An update is available",
                      ("getwindowfocus",): "101" if phase == "picker" else "102"}
            return subprocess.CompletedProcess(command, 0, stdout=values.get(args, "42").encode(), stderr=b"")
        linux = Linux.__new__(Linux)
        with patch("subprocess.run", side_effect=observed):
            self.assertEqual(linux.picker_ready({42}), (42, "picker"))
            phase = "arriving"
            self.assertIsNone(linux.picker_ready({42}))
            phase = "offer"
            self.assertEqual(linux.picker_ready({42}), (42, "offer"))
        self.assertTrue(all(command[1] in ("-root", "search", "getwindowpid", "getwindowname", "getwindowfocus") for command in queries))

    def test_linux_late_offer_reads_management_after_mapping_like_native_smoke(self):
        mapped = False
        def observed(command, **_kwargs):
            nonlocal mapped
            args = tuple(command[1:])
            if args == ("search", "--onlyvisible", "--pid", "42"):
                mapped = True
            values = {("-root", "_NET_CLIENT_LIST"): "_NET_CLIENT_LIST(WINDOW): window id # " + ("0x65, 0x66" if mapped else "0x65"),
                      ("search", "--pid", "42"): "101\n102", ("search", "--onlyvisible", "--pid", "42"): "101\n102",
                      ("getwindowname", "101"): "Ibis", ("getwindowname", "102"): "An update is available",
                      ("getwindowfocus",): "102"}
            return subprocess.CompletedProcess(command, 0, stdout=values.get(args, "42").encode(), stderr=b"")
        with patch("subprocess.run", side_effect=observed):
            self.assertEqual(Linux.__new__(Linux).picker_ready({42}), (42, "offer"))

    def update_consumer(self, scenario, system="Linux"):
        events = []
        state = {"reopened": False, "pressed": False, "offer": False, "picker_proved": False}
        clock = {"now": 0.0}
        observations = {}
        with tempfile.TemporaryDirectory() as temporary:
            def installed(folder, _args, library):
                install = MagicMock(command=["synthetic-owned-install"], config=None)
                install.settings = seed_settings(folder, library)
                def processes():
                    if state["pressed"]:
                        if scenario == "not-restarted":
                            return {42}
                        if scenario == "exited-after-restart" and clock["now"] >= state["pressed_at"] + 5:
                            return set()
                        return {43}
                    return {43} if scenario == "replacement" and state["reopened"] else {42}
                install.instances.side_effect = processes
                def picker(pids, **_kwargs):
                    events.append(("picker", set(pids)))
                    phase = "initial" if not state["reopened"] else "offer" if state["picker_proved"] else "reopen"
                    observations[phase] = observations.get(phase, 0) + 1
                    if scenario in ("transient-" + phase, "persistent-" + phase) and (scenario.startswith("persistent-") or observations[phase] == 1):
                        raise PickerObservationFailure({"stage": "cg-window-title", "code": None})
                    if scenario == "lost-owner-after-observation" and phase == "offer":
                        install.instances.return_value = set()
                        install.instances.side_effect = None
                        raise PickerObservationFailure({"stage": "cg-window-title", "code": None})
                    if scenario == "hidden-by-offer" or (scenario == "late-hidden" and state["offer"]):
                        return None
                    if scenario == "late-offer" and state["offer"]:
                        return 42, "offer"
                    if scenario == "native-offer-without-action" and state["reopened"]:
                        state["picker_proved"] = True
                        return 42, "offer"
                    if state["reopened"] and scenario == "immediate-offer":
                        state["picker_proved"] = True
                        return 42, "offer"
                    if state["reopened"] and scenario != "unfocused":
                        state["picker_proved"] = True
                        return 42, "picker"
                    return None
                install.picker_ready.side_effect = picker
                def look(pids, press=None, **_kwargs):
                    if press:
                        if scenario == "native-offer-without-action":
                            return False, [], None
                        if scenario in ("late-offer", "late-hidden") and not state["offer"]:
                            state["offer"] = True
                            events.append(("wait-offer", set(pids)))
                            return False, [], None
                        events.append(("press", set(pids)))
                        self.assertTrue(state["reopened"], "update polled before its picker was reopened")
                        if scenario == "activation":
                            raise WindowsActivationFailure({"attempted": True})
                        if scenario == "macos-activation":
                            raise MacOSActivationFailure({"attempted": True})
                        if scenario == "linux-activation":
                            raise LinuxActivationFailure({"attempted": True}, "x11-key")
                        if scenario == "helper":
                            raise RuntimeError("synthetic uncertain helper")
                        state["pressed"] = True
                        state["pressed_at"] = clock["now"]
                    return bool(press), ["synthetic-private-sentinel"] if not press else [], None
                install.look.side_effect = look
                install.updated.side_effect = lambda: state["pressed"] and scenario != "not-replaced"
                install.facts.return_value = {}
                return install
            def launch(command, **_kwargs):
                events.append(("launch", list(command)))
                events.append(("environment", _kwargs["env"]))
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
            with patch.dict(SYSTEMS, {system: installed}), patch("platform.system", return_value=system), \
                    patch("subprocess.Popen", side_effect=launch), patch("time.monotonic", side_effect=lambda: clock["now"]), \
                    patch("time.sleep", side_effect=lambda step: clock.update(now=clock["now"] + step)), \
                    patch("os.kill"), patch("builtins.print"), patch(__name__ + ".feed_version", return_value="synthetic"):
                result = run(argparse.Namespace(output=Path(temporary)))
            report = json.loads((Path(temporary) / "update-e2e.json").read_text(encoding="utf-8"))
        return result, report, events

    def test_transient_observations_retry_without_repeating_reopen_or_activation(self):
        for system in ("Darwin", "Windows", "Linux"):
            for phase in ("initial", "reopen", "offer"):
                with self.subTest(system=system, phase=phase):
                    result, report, events = self.update_consumer("transient-" + phase, system)
                    self.assertEqual(result, 0, report)
                    self.assertEqual(sum(event == "launch" for event, _ in events), 2)
                    self.assertEqual([pids for event, pids in events if event == "press"], [{42}])
                    self.assertEqual(report["settings_preserved"], {"library": True, "shortcut": True})

    def test_persistent_observation_failure_keeps_fixed_cause_and_never_activates(self):
        for phase in ("initial", "reopen", "offer"):
            with self.subTest(phase=phase):
                result, report, events = self.update_consumer("persistent-" + phase)
                self.assertEqual(result, 1, report)
                self.assertIn("native picker observation unavailable", report["error"])
                self.assertEqual(report["last_picker_probe"], {"stage": "cg-window-title", "code": None})
                self.assertFalse(any(event == "press" for event, _ in events))
                self.assertEqual(sum(event == "launch" for event, _ in events), 1 if phase == "initial" else 2)
                self.assertGreater(sum(sample["samples"] for sample in report["picker_observations"]), 1)

    def test_observation_retry_cannot_borrow_an_owner_that_exited(self):
        result, report, events = self.update_consumer("lost-owner-after-observation")
        self.assertEqual(result, 1, report)
        self.assertIn("exited before offering", report["error"])
        self.assertFalse(any(event == "press" for event, _ in events))

    def test_visible_native_offer_without_an_accessible_action_is_not_reported_as_never_offered(self):
        result, report, events = self.update_consumer("native-offer-without-action")
        self.assertEqual(result, 1, report)
        self.assertIn("native update offer", report["error"])
        self.assertNotIn("never offered", report["error"])
        self.assertFalse(any(event == "press" for event, _ in events))

    def test_update_reopens_exact_install_once_before_single_owned_activation(self):
        result, report, events = self.update_consumer("success")
        self.assertEqual(result, 0, report)
        self.assertEqual([value for event, value in events if event == "launch"], [["synthetic-owned-install"]] * 2)
        self.assertEqual([value for event, value in events if event == "press"], [{42}])
        self.assertEqual(report["settings_preserved"], {"library": True, "shortcut": True})
        self.assertEqual([sample["phase"] for sample in report["picker_observations"]],
                         ["before-reopen", "after-reopen", "before-activation"])

    def test_update_accepts_an_owned_offer_already_focused_by_reopen(self):
        result, report, events = self.update_consumer("immediate-offer")
        self.assertEqual(result, 0, report)
        self.assertEqual(report["reopen_focus"], "offer")
        self.assertEqual([value for event, value in events if event == "press"], [{42}])
        self.assertEqual(sum(event == "launch" for event, _ in events), 2)

    def test_update_late_offer_rechecks_focus_and_still_activates_only_once(self):
        result, report, events = self.update_consumer("late-offer")
        self.assertEqual(result, 0, report)
        self.assertEqual([value for event, value in events if event == "press"], [{42}])
        self.assertEqual(sum(event == "launch" for event, _ in events), 2)
        self.assertGreaterEqual(sum(event == "picker" for event, _ in events), 3)

    def test_update_never_activates_when_an_early_or_late_offer_hides_the_picker(self):
        for scenario in ("hidden-by-offer", "late-hidden"):
            with self.subTest(scenario=scenario):
                result, report, events = self.update_consumer(scenario)
                self.assertEqual(result, 1, report)
                self.assertFalse(any(event == "press" for event, _ in events))
                self.assertEqual(sum(event == "launch" for event, _ in events), 2)

    def test_update_never_activates_after_reopen_refusal_io_timeout_or_lost_owner(self):
        for scenario in ("refused", "io", "timeout", "unfocused", "replacement"):
            with self.subTest(scenario=scenario):
                result, report, events = self.update_consumer(scenario)
                self.assertEqual(result, 1, report)
                self.assertEqual(sum(event == "launch" for event, _ in events), 2)
                self.assertFalse(any(event == "press" for event, _ in events))
                self.assertNotIn("settings_preserved", report)
                self.assertNotIn("synthetic-private-sentinel", json.dumps(report))

    def test_update_never_retries_an_uncertain_activation_after_reopen(self):
        for scenario in ("activation", "macos-activation", "linux-activation", "helper"):
            with self.subTest(scenario=scenario):
                result, report, events = self.update_consumer(scenario)
                self.assertEqual(result, 1, report)
                self.assertEqual(sum(event == "launch" for event, _ in events), 2)
                self.assertEqual(sum(event == "press" for event, _ in events), 1)

    def test_update_still_requires_replacement_restart_and_the_fresh_process_to_stay_up(self):
        for scenario, message in (("not-replaced", "not replaced"), ("not-restarted", "did not start"),
                                  ("exited-after-restart", "exited right after")):
            with self.subTest(scenario=scenario):
                result, report, events = self.update_consumer(scenario)
                self.assertEqual(result, 1, report)
                self.assertIn(message, report["error"])
                self.assertEqual(sum(event == "launch" for event, _ in events), 2)
                self.assertEqual([pids for event, pids in events if event == "press"], [{42}])
                self.assertNotIn("settings_preserved", report)

    def test_observation_retry_uses_one_deadline_and_rejects_late_success(self):
        clock = {"now": 0.0}
        attempts = []
        def failed():
            attempts.append(clock["now"])
            clock["now"] += .6
            raise PickerObservationFailure({"stage": "cg-window-title", "code": None})
        with patch("time.monotonic", side_effect=lambda: clock["now"]), \
                patch("time.sleep", side_effect=lambda step: clock.update(now=clock["now"] + step)):
            with self.assertRaisesRegex(PickerObservationFailure, "cg-window-title"):
                wait_for(failed, 3, "synthetic deadline", step=.25, retry_observation=True)
            self.assertLessEqual(clock["now"], 3.6)
            self.assertEqual(len(attempts), 4)
            clock["now"] = 0
            def late():
                clock["now"] = 4
                return 42, "picker"
            with self.assertRaisesRegex(AssertionError, "synthetic deadline"):
                wait_for(late, 3, "synthetic deadline", retry_observation=True)

    def test_observation_retry_never_retries_ambiguous_windows_or_uses_old_failure_after_valid_absence(self):
        for stage in ("unique", "picker-unique", "picker-search-unique"):
            predicate = MagicMock(side_effect=PickerObservationFailure({"stage": stage, "code": None}))
            with self.subTest(stage=stage), self.assertRaises(PickerObservationFailure):
                wait_for(predicate, 1, "synthetic deadline", retry_observation=True)
            predicate.assert_called_once_with()
        clock = {"now": 0}
        predicate = MagicMock(side_effect=[PickerObservationFailure({"stage": "cg-window-title", "code": None}), None])
        with patch("time.monotonic", side_effect=lambda: clock["now"]), \
                patch("time.sleep", side_effect=lambda step: clock.update(now=clock["now"] + step)):
            with self.assertRaisesRegex(AssertionError, "synthetic absence"):
                wait_for(predicate, 2, "synthetic absence", retry_observation=True)

    def test_windows_scope_assertions_cannot_be_replaced_by_a_later_clean_observation(self):
        for stage in ("picker-owner", "picker-bounds", "picker-windows", "picker-search"):
            failed = json.dumps({"pid": None, "focus": None, "probe": {
                "stage": stage, "code": -1, "failed": True, "trusted": None,
                "windows": 0, "pickers": 0, "offers": 0}})
            clean = '{"pid":42,"focus":"picker"}'
            clock = {"now": 0}
            with self.subTest(stage=stage), patch(__name__ + ".powershell", side_effect=[failed, clean]) as helper, \
                    patch("time.monotonic", side_effect=lambda: clock["now"]), \
                    patch("time.sleep", side_effect=lambda step: clock.update(now=clock["now"] + step)):
                observe = lambda: Windows.__new__(Windows).picker_ready({42})
                if stage in ("picker-owner", "picker-bounds"):
                    with self.assertRaises(PickerObservationFailure) as failure:
                        wait_for(observe, 3, "synthetic deadline", retry_observation=True)
                    self.assertFalse(failure.exception.retryable)
                    self.assertEqual(failure.exception.probe["stage"], stage)
                    helper.assert_called_once()
                    self.assertEqual(clock["now"], 0)
                else:
                    self.assertEqual(wait_for(observe, 3, "synthetic deadline", retry_observation=True), (42, "picker"))
                    self.assertEqual(helper.call_count, 2)

    def test_windows_picker_failure_preserves_fixed_helper_operation_and_exit_code(self):
        windows = Windows.__new__(Windows)
        outcomes = ((FileNotFoundError("synthetic-private-sentinel"), "windows-helper-launch", None),
                    (FileNotFoundError(2, "synthetic-private-sentinel"), "windows-helper-launch", 2),
                    (PermissionError(13, "synthetic-private-sentinel"), "windows-helper-launch", 13),
                    (subprocess.TimeoutExpired("synthetic-private-sentinel", 10), "windows-helper-timeout", None),
                    (subprocess.CompletedProcess([], 23, stdout=b"", stderr=b"synthetic-private-sentinel"), "windows-helper-exit", 23),
                    (subprocess.CompletedProcess([], 0, stdout=b"synthetic-private-sentinel", stderr=b""), "windows-helper-decode", None))
        for outcome, stage, code in outcomes:
            with self.subTest(stage=stage), patch("subprocess.run") as helper:
                if isinstance(outcome, Exception):
                    helper.side_effect = outcome
                else:
                    helper.return_value = outcome
                with self.assertRaises(PickerObservationFailure) as failure:
                    windows.picker_ready({42})
                self.assertEqual(failure.exception.probe, {"stage": stage, "code": code})
                self.assertNotIn("synthetic-private-sentinel", str(failure.exception))
                self.assertEqual(helper.call_count, 1)

    @staticmethod
    def windows_probe(pressed=False):
        return {"attempted": pressed, "blocked": False, "pids": 1, "windows": int(pressed), "elements": int(pressed),
                "offer_matches": int(pressed), "action_matches": int(pressed), "invoke_available": pressed,
                "enabled": pressed, "on_screen": pressed, "errors": [], "pressed_by": "invoke" if pressed else None,
                "tree_complete": True, "root_windows": int(pressed), "nested_windows": 0}

    def test_windows_root_only_or_partial_scan_never_proves_the_offer_absent(self):
        for change in ({"tree_complete": False}, {"root_windows": -1}, {"nested_windows": "0"}):
            probe = dict(self.windows_probe(False), **change)
            payload = json.dumps({"pressed": False, "seen": [], "probe": probe})
            with self.subTest(change=change), patch(__name__ + ".powershell", return_value=payload):
                with self.assertRaises(WindowsActivationFailure):
                    Windows.__new__(Windows).look({42}, BUTTON)

    def test_windows_native_census_never_reports_window_text_or_unknown_metadata(self):
        native = Windows.__new__(Windows)
        native.instances = MagicMock(return_value={42})
        fields = {"windows", "offers", "visible_offers", "foreground_offers", "pickers", "visible_pickers",
                  "foreground_pickers", "owner_changes", "bound_hit", "unavailable"}
        good = dict.fromkeys(fields, 0)
        good.update(windows=3, pickers=1, visible_pickers=1)
        for payload in (good, dict(good, private="synthetic-private-sentinel"), dict(good, offers="1")):
            with self.subTest(valid=payload == good), patch(__name__ + ".powershell", return_value=json.dumps(payload)):
                facts = native.native_windows()
                self.assertEqual(facts, good if payload == good else {"unavailable": True})
                self.assertNotIn("synthetic-private-sentinel", json.dumps(facts))

    def test_automatic_check_diagnostics_distinguish_the_expected_result_without_text(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "synthetic.log"
            expected = "0.2.2-nightly.20990101000000"
            path.write_text("synthetic-private-sentinel\nibis: automatic update check: " + expected + " is available\n", encoding="utf-8")
            facts = automatic_check_facts(path, expected)
            self.assertEqual(facts, {"expected_available": 1, "other_available": 0, "up_to_date": 0, "failed": 0})
            self.assertNotIn("synthetic-private-sentinel", json.dumps(facts))
            path.write_text("ibis: automatic update check: up to date\nibis: automatic update check failed: network\n", encoding="utf-8")
            self.assertEqual(automatic_check_facts(path, expected),
                             {"expected_available": 0, "other_available": 0, "up_to_date": 1, "failed": 1})

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

    def test_windows_custom_native_click_never_accepts_default_key_metadata(self):
        for method in ("task-dialog", "native-button", "return", "default"):
            probe = dict(self.windows_probe(True), invoke_available=False, pressed_by=method)
            payload = json.dumps({"pressed": True, "seen": [], "probe": probe})
            with self.subTest(method=method), patch(__name__ + ".powershell", return_value=payload) as helper:
                if method == "task-dialog":
                    self.assertTrue(Windows.__new__(Windows).look({42}, BUTTON)[0])
                else:
                    with self.assertRaises(WindowsActivationFailure):
                        Windows.__new__(Windows).look({42}, BUTTON)
                helper.assert_called_once()

    def test_windows_task_dialog_action_without_an_invoke_pattern_is_one_deliberate_activation(self):
        probe = dict(self.windows_probe(True), invoke_available=False, pressed_by="task-dialog")
        with patch(__name__ + ".powershell", return_value=json.dumps({"pressed": True, "seen": [], "probe": probe})) as helper:
            self.assertTrue(Windows.__new__(Windows).look({42}, BUTTON)[0])
            helper.assert_called_once()

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
using System.Text;
namespace IbisUpdateContract {
 public enum TreeScope { Children, Descendants }
 public class Condition { public static object TrueCondition=new object(); }
 public class PropertyCondition { public object Key,Value; public PropertyCondition(object key,object value) {Key=key;Value=value;} }
 public class AndCondition { public AndCondition(object a,object b) {} }
 public class ControlType {
  public string ProgrammaticName; public ControlType(string name) {ProgrammaticName="ControlType."+name;}
  public static ControlType Button=new ControlType("Button"), Pane=new ControlType("Pane"), Window=new ControlType("Window");
 }
 public class InvokePattern { public static object Pattern=new object(); }
 public class AutomationElement {
  public static object ProcessIdProperty=new object(), ControlTypeProperty=new object(), NameProperty=new object();
  public static Node RootElement=new Node();
 }
 public static class Focus { public static long Handle=101; public static IntPtr GetForegroundWindow() {return new IntPtr(Handle);} }
 public class TreeWalker {
  public static TreeWalker RawViewWalker=new TreeWalker();
  public Node GetParent(Node node) {return node.Parent;}
 }
 public static class Native {
  public static Node Target;
  public static long Foreground=101; public static uint Owner=42;
  public static bool Visible=true, Enabled=true, Refused=false, Delivered=true;
  public static string Caption="An update is available", Class="#32770";
  public static int Clicks=0;
 }
 public class Node {
  private static int NextId=0; private int Identity=++NextId;
  public string Name="", Type="Pane", AutomationId=""; public int ProcessId=42; public bool IsEnabled=true, IsOffscreen=false;
  public int NativeWindowHandle=101;
  public bool HasKeyboardFocus=true; public bool Supported=true, ThrowOnRead=false, Fail=false, ChangeOwnerOnActionScan=false, FailFindAll=false; public int Invocations=0;
  private Node[] Children=new Node[0]; public Node Parent;
  public Node ChangeWindow; public string PatternMutation="";
  public Node[] Nodes {get {return Children;} set {Children=value; foreach(var node in value) node.Parent=this;}}
  public Node Current { get {if(ThrowOnRead && Invocations>0) throw new Exception("synthetic stale element"); return this;} }
  public ControlType ControlType { get {return new IbisUpdateContract.ControlType(Type);} }
  public int[] GetRuntimeId() {return new int[]{Identity};}
  public Node[] FindAll(TreeScope scope,object condition) {
   if(FailFindAll) throw new Exception("synthetic-private-sentinel");
   if(ChangeOwnerOnActionScan && condition==Condition.TrueCondition) {
    ProcessId=99; foreach(var node in Nodes) node.ProcessId=99;
   }
   var nodes=new System.Collections.Generic.List<Node>();
   foreach(var node in Nodes) {
    var test=condition as PropertyCondition;
    // Deliberately return a stale/foreign root for PID conditions so callers
    // must revalidate ownership; descendant Window filtering remains real.
    if(test==null || test.Key==AutomationElement.ProcessIdProperty ||
       (test.Key==AutomationElement.ControlTypeProperty && node.ControlType.ProgrammaticName==((ControlType)test.Value).ProgrammaticName)) nodes.Add(node);
    if(scope==TreeScope.Descendants) nodes.AddRange(node.FindAll(scope,condition));
   }
   return nodes.ToArray();
  }
  public Node FindFirst(TreeScope scope,object condition) {return Nodes.Length>0 ? Nodes[0] : null;}
  public bool TryGetCurrentPattern(object pattern,ref object value) {
   value=Supported && pattern==InvokePattern.Pattern ? this : null;
   if(PatternMutation=="window-handle") ChangeWindow.NativeWindowHandle=999;
   if(PatternMutation=="window-owner") ChangeWindow.ProcessId=99;
   if(PatternMutation=="action-owner") ProcessId=99;
   if(PatternMutation=="detach") Parent=null;
   return value!=null;
  }
  public object GetCurrentPattern(object pattern) {if(!Supported) throw new Exception("unsupported"); return this;}
  public void Invoke() {Invocations++; if(Fail) throw new Exception("synthetic uncertain invocation");}
 }
}
'@
$window=[IbisUpdateContract.Node]::new(); $window.Name='An update is available'; $window.Type='Window'
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
 'nested' {
  $window.NativeWindowHandle=102
  $parent=[IbisUpdateContract.Node]::new(); $parent.Name='Ibis'; $parent.Type='Window'; $parent.Nodes=@($window)
  [IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($parent)
 }
 'nested-foreign' {
  $window.ProcessId=99
  $parent=[IbisUpdateContract.Node]::new(); $parent.Name='Ibis'; $parent.Type='Window'; $parent.Nodes=@($window)
  [IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($parent)
 }
 'other-window-action' {
  $other=[IbisUpdateContract.Node]::new(); $other.Name='Other'; $other.Type='Window'; $other.NativeWindowHandle=103
  $other.Nodes=@($item); $window.Nodes=@($other)
 }
 'window-replaced' {$item.ChangeWindow=$window; $item.PatternMutation='window-handle'}
 'window-owner-late' {$item.ChangeWindow=$window; $item.PatternMutation='window-owner'}
 'action-owner-late' {$item.PatternMutation='action-owner'}
 'action-detached' {$item.PatternMutation='detach'}
 'owner-before-press' {$window.ChangeOwnerOnActionScan=$true}
}
if ($Scenario.StartsWith('native-')) {
 $item.Supported=$false; $item.NativeWindowHandle=0; $item.AutomationId='1008'
 [IbisUpdateContract.Native]::Target=$item
 $later=[IbisUpdateContract.Node]::new(); $later.Type='Button'; $later.Name='Later'; $later.AutomationId='1004'
 $window.Nodes=@($later,$item) # The default is Later; Install is a virtual second action.
 switch($Scenario) {
  'native-uncertain' {$item.Fail=$true}
  'native-refused' {[IbisUpdateContract.Native]::Refused=$true}
  'native-timeout' {[IbisUpdateContract.Native]::Delivered=$false}
  'native-background' {[IbisUpdateContract.Native]::Foreground=999}
  'native-foreign' {[IbisUpdateContract.Native]::Owner=99}
  'native-hidden' {[IbisUpdateContract.Native]::Visible=$false}
  'native-disabled' {[IbisUpdateContract.Native]::Enabled=$false}
  'native-caption' {[IbisUpdateContract.Native]::Caption='Other dialog'}
  'native-class' {[IbisUpdateContract.Native]::Class='Other'}
  'native-wrong-name' {$item.Name='Install and Restart suffix'}
  'native-reopened' {$item.ChangeWindow=$window; $item.PatternMutation='window-handle'}
 }
}
if ($Scenario.StartsWith('picker-')) {
 $window.Name='Ibis'
 $search=[IbisUpdateContract.Node]::new(); $search.Name='Search references'; $search.Type='ComboBox'
 $window.Nodes=@($search)
 switch($Scenario) {
  'picker-hidden' {$window.IsOffscreen=$true}
  'picker-edit' {$search.Type='Edit'}
  'picker-loading' {$window.Nodes=@()}
  'picker-search-title' {$search.Name='Search references suffix'}
  'picker-search-role' {$search.Type='Pane'}
  'picker-search-disabled' {$search.IsEnabled=$false}
  'picker-search-hidden' {$search.IsOffscreen=$true}
  'picker-search-unfocused' {$search.HasKeyboardFocus=$false}
  'picker-search-duplicate' {$window.Nodes=@($search,$search)}
  'picker-search-bound' {$window.Nodes=@($search)*4001}
  'picker-title' {$window.Name='Ibis suffix'}
  'picker-duplicate' {[IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($window,$window)}
  'picker-foreign' {$window.ProcessId=99}
  'picker-root-bound' {[IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($window)*21}
  'picker-nested-bound' {
   $aux=[IbisUpdateContract.Node]::new(); $aux.Name='Other'; $aux.Type='Window'
   $window.Nodes=@($aux)*21
  }
  'picker-total-bound' {
   $aux=[IbisUpdateContract.Node]::new(); $aux.Name='Other'; $aux.Type='Window'
   $window.Nodes=@($aux)*10
   [IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($window,$window)
  }
  'picker-native-error' {[IbisUpdateContract.AutomationElement]::RootElement.FailFindAll=$true}
  'picker-unfocused' {[IbisUpdateContract.Focus]::Handle=102}
  'picker-unknown' {[IbisUpdateContract.Focus]::Handle=0}
  'picker-no-handle' {$window.NativeWindowHandle=0}
  'picker-empty' {[IbisUpdateContract.AutomationElement]::RootElement.Nodes=@()}
  'picker-offer' {
   $window.Nodes=@() # A real immediate modal can hide Search from UIA.
   $offer=[IbisUpdateContract.Node]::new(); $offer.Name='An update is available'; $offer.NativeWindowHandle=102; $offer.Type='Window'
   [IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($window,$offer)
   [IbisUpdateContract.Focus]::Handle=102
  }
  'picker-offer-duplicate' {
   $offer=[IbisUpdateContract.Node]::new(); $offer.Name='An update is available'; $offer.NativeWindowHandle=102; $offer.Type='Window'
   [IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($window,$offer,$offer)
   [IbisUpdateContract.Focus]::Handle=102
  }
  'picker-offer-foreign' {
   $offer=[IbisUpdateContract.Node]::new(); $offer.Name='An update is available'; $offer.NativeWindowHandle=102; $offer.ProcessId=99; $offer.Type='Window'
   [IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($window,$offer)
   [IbisUpdateContract.Focus]::Handle=102
  }
  'picker-offer-hidden' {
   $offer=[IbisUpdateContract.Node]::new(); $offer.Name='An update is available'; $offer.NativeWindowHandle=102; $offer.IsOffscreen=$true; $offer.Type='Window'
   [IbisUpdateContract.AutomationElement]::RootElement.Nodes=@($window,$offer)
   [IbisUpdateContract.Focus]::Handle=102
  }
  'picker-nested-offer' {
   $offer=[IbisUpdateContract.Node]::new(); $offer.Name='An update is available'; $offer.Type='Window'; $offer.NativeWindowHandle=102
   $window.Nodes=@($offer); [IbisUpdateContract.Focus]::Handle=102
  }
  'picker-nested-offer-foreign' {
   $offer=[IbisUpdateContract.Node]::new(); $offer.Name='An update is available'; $offer.Type='Window'; $offer.NativeWindowHandle=102; $offer.ProcessId=99
   $window.Nodes=@($offer); [IbisUpdateContract.Focus]::Handle=102
  }
  'picker-nested-offer-duplicate' {
   $offer=[IbisUpdateContract.Node]::new(); $offer.Name='An update is available'; $offer.Type='Window'; $offer.NativeWindowHandle=102
   $window.Nodes=@($offer,$offer); [IbisUpdateContract.Focus]::Handle=102
  }
 }
}
$env:IBIS_PIDS='42'; $env:IBIS_PRESS='Install and Restart'
if ($Scenario -ceq 'picker-process-bound') { $env:IBIS_PIDS=(1..21) -join ',' }
function Contract-Result($pressed,$how) {
 $expected=$Scenario -in @('pane','button','native-button','standard','standard-localized','standard-rtl','nested')
 $invoked=$expected -or $Scenario -in @('uncertain','standard-uncertain','native-uncertain','native-timeout')
 if($pressed -ne $expected -or $item.Invocations -ne [int]$invoked) {throw 'Unsafe or missing activation'}
 if($probe.attempted -ne ($invoked -or $Scenario -ceq 'native-refused')) {throw 'Incorrect attempt metadata'}
 if($probe.blocked -ne ($Scenario -notin @('pane','button','native-button','standard','standard-localized','standard-rtl','nested','other-window-action','wrong-window','wrong-name','case-name','case-window','missing-window','standard-no','standard-wrong-id','standard-pane','native-wrong-name'))) {throw 'Unsafe observation can poll'}
 if([IbisUpdateContract.Native]::Clicks -ne [int]($Scenario -in @('native-button','native-uncertain','native-refused','native-timeout'))) {throw 'Incorrect native activation count'}
 if($expected -and $how -ne $(if($Scenario -ceq 'native-button') {'task-dialog'} else {'invoke'})) {throw 'Incorrect invocation metadata'}
}
'''
        # Run the production Ready/Click implementation too. Replace only its
        # OS boundary; virtual actions intentionally have no native Button HWND.
        native = WINDOWS_NATIVE_INSTALL.split("public static class ", 1)[1].rsplit("'@", 1)[0]
        native = "public static class " + native
        api_start = native.index("  [DllImport")
        api_end = native.index("  public static bool Ready")
        native = native[:api_start] + r'''
  static IntPtr GetForegroundWindow() {return new IntPtr(Native.Foreground);}
  static bool IsWindowVisible(IntPtr window) {return window.ToInt64()==101 && Native.Target!=null && Native.Visible;}
  static bool IsWindowEnabled(IntPtr window) {return window.ToInt64()==101 && Native.Enabled;}
  static uint GetWindowThreadProcessId(IntPtr window,out uint pid) {pid=Native.Owner; return 1;}
  static int GetWindowText(IntPtr window,StringBuilder text,int count) {text.Append(Native.Caption); return text.Length;}
  static int GetClassName(IntPtr window,StringBuilder text,int count) {text.Append(Native.Class); return text.Length;}
  static IntPtr SendMessageTimeout(IntPtr window,uint message,IntPtr wparam,IntPtr lparam,uint flags,uint timeout,out IntPtr result) {
    if(window.ToInt64()!=101 || message!=0x466 || wparam.ToInt64()!=1008 || lparam!=IntPtr.Zero || flags!=3 || timeout!=5000)
      throw new Exception("Wrong task-dialog action");
    Native.Clicks++; result=IntPtr.Zero;
    if(Native.Refused) return IntPtr.Zero;
    Native.Target.Invoke();
    return Native.Delivered ? new IntPtr(1) : IntPtr.Zero;
  }
''' + native[api_end:]
        model = model.replace("namespace IbisUpdateContract {", "namespace IbisUpdateContract {\n" + native)
        # Assert activation counters, retaining real serialization for the caller.
        helper = WINDOWS_LOOK.replace(WINDOWS_NATIVE_INSTALL, "").replace("Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes", "")
        helper = helper.replace("System.Windows.Automation.", "IbisUpdateContract.").replace("[IbisUpdateTaskDialog]", "[IbisUpdateContract.IbisUpdateTaskDialog]")
        helper = helper.replace("function Answer($pressed, $how) {",
                                "function Answer($pressed, $how) {\n  Contract-Result $pressed $how")
        with tempfile.TemporaryDirectory() as temporary:
            script = Path(temporary) / "update-contract.ps1"
            script.write_text(model + helper, encoding="utf-8")
            for scenario in ("pane", "button", "native-button", "native-uncertain", "native-refused", "native-timeout", "native-background",
                             "native-foreign", "native-hidden", "native-disabled", "native-caption", "native-class", "native-wrong-name", "native-reopened",
                             "standard", "standard-localized", "standard-rtl", "standard-no", "standard-wrong-id", "standard-pane",
                             "standard-disabled", "standard-hidden", "standard-uncertain", "standard-unsupported", "standard-duplicate",
                             "standard-wrong-owner", "mixed-actions", "wrong-owner", "wrong-window", "wrong-name", "disabled", "hidden",
                             "unsupported", "duplicate-action", "duplicate-window", "missing-window", "wrong-window-owner",
                             "window-bound", "case-name", "case-window", "bound", "uncertain", "nested", "nested-foreign",
                             "other-window-action", "window-replaced", "window-owner-late", "action-owner-late", "action-detached", "owner-before-press"):
                with self.subTest(scenario=scenario):
                    expected = scenario in ("pane", "button", "native-button", "standard", "standard-localized", "standard-rtl", "nested")
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
            for scenario in ("success", "hidden", "title", "duplicate", "foreign", "unfocused", "unknown", "no-handle", "empty",
                             "offer", "offer-duplicate", "offer-foreign", "offer-hidden", "edit", "loading", "search-title",
                             "search-role", "search-disabled", "search-hidden", "search-unfocused", "search-duplicate", "search-bound",
                             "nested-offer", "nested-offer-foreign", "nested-offer-duplicate", "process-bound", "root-bound",
                             "nested-bound", "total-bound", "native-error"):
                with self.subTest(picker=scenario):
                    result = subprocess.run([shutil.which("pwsh"), "-NoProfile", "-File", str(script), "picker-" + scenario],
                                            capture_output=True, timeout=30)
                    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", errors="replace"))
                    if scenario in ("duplicate", "foreign", "offer-duplicate", "offer-foreign", "search-duplicate", "search-bound",
                                    "nested-offer-foreign", "nested-offer-duplicate", "process-bound", "root-bound",
                                    "nested-bound", "total-bound", "native-error"):
                        with self.assertRaises(PickerObservationFailure) as failure:
                            picker_result(result.stdout, {42})
                        transient = scenario == "native-error"
                        self.assertEqual(failure.exception.retryable, transient)
                        if scenario in ("foreign", "offer-foreign", "nested-offer-foreign"):
                            self.assertEqual(failure.exception.probe["stage"], "picker-owner")
                        if scenario.endswith("bound"):
                            self.assertEqual(failure.exception.probe["stage"], "picker-bounds")
                        # The offending window can disappear on the next poll.
                        # Only a native read error may consume that clean answer.
                        clock = {"now": 0}
                        with patch(__name__ + ".powershell", side_effect=[result.stdout, '{"pid":42,"focus":"picker"}']) as helper, \
                                patch("time.monotonic", side_effect=lambda: clock["now"]), \
                                patch("time.sleep", side_effect=lambda step: clock.update(now=clock["now"] + step)):
                            observe = lambda: Windows.__new__(Windows).picker_ready({42})
                            if transient:
                                self.assertEqual(wait_for(observe, 3, "synthetic deadline", retry_observation=True), (42, "picker"))
                                self.assertEqual(helper.call_count, 2)
                            else:
                                with self.assertRaises(PickerObservationFailure):
                                    wait_for(observe, 3, "synthetic deadline", retry_observation=True)
                                helper.assert_called_once()
                    else:
                        expected = (42, "offer") if scenario in ("offer", "nested-offer") else (42, "picker") if scenario in ("success", "edit") else None
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
        windows.picker_ready = MagicMock(return_value=(42, "picker"))
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

    def test_settings_verification_accepts_the_canonical_library_identity_after_startup(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "selected").mkdir()
            library = root / "selected" / ".." / "synthetic 引用.bib"
            library.write_text("@misc{synthetic}\n", encoding="utf-8")
            settings = seed_settings(root, library)
            self.assertEqual(preserved_settings(settings, library), {"library": True, "shortcut": True})
            canonical = str(library.resolve(strict=True))
            if os.name == "nt" and not canonical.startswith("\\\\?\\"):
                # Rust's canonicalize retains the verbatim prefix on Windows.
                canonical = "\\\\?\\UNC\\" + canonical[2:] if canonical.startswith("\\\\") else "\\\\?\\" + canonical
            self.assertNotEqual(canonical, str(library))
            saved = json.loads(settings.read_text(encoding="utf-8"))
            saved["library"] = {"bibtex": canonical}
            saved["picker_windows"] = {}  # Added by #1500, within the same schema.
            settings.write_text(json.dumps(saved), encoding="utf-8")
            self.assertEqual(preserved_settings(settings, library), {"library": True, "shortcut": True})

    @unittest.skipIf(os.name == "nt", "creating symlinks needs extra rights on Windows")
    def test_settings_verification_accepts_a_resolved_directory_alias_like_macos_var(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "canonical"
            target.mkdir()
            alias = root / "selected"
            alias.symlink_to(target, target_is_directory=True)
            library = alias / "synthetic.bib"
            library.write_text("@misc{synthetic}\n", encoding="utf-8")
            settings = seed_settings(root, library)
            saved = json.loads(settings.read_text(encoding="utf-8"))
            saved["library"] = {"bibtex": str(library.resolve(strict=True))}
            settings.write_text(json.dumps(saved), encoding="utf-8")
            self.assertEqual(preserved_settings(settings, library), {"library": True, "shortcut": True})

    def test_settings_verification_refuses_a_different_file_even_with_identical_contents(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            library, other = root / "synthetic.bib", root / "different.bib"
            for path in (library, other):
                path.write_text("@misc{synthetic}\n", encoding="utf-8")
            settings = seed_settings(root, library)
            saved = json.loads(settings.read_text(encoding="utf-8"))
            saved["library"] = {"bibtex": str(other)}
            settings.write_text(json.dumps(saved), encoding="utf-8")
            with self.assertRaisesRegex(AssertionError, "synthetic library choice was not preserved"):
                preserved_settings(settings, library)

    def test_settings_verification_refuses_malformed_or_missing_library_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            library = root / "synthetic.bib"
            library.write_text("@misc{synthetic}\n", encoding="utf-8")
            settings = seed_settings(root, library)
            saved = json.loads(settings.read_text(encoding="utf-8"))
            choices = (None, str(library), {}, {"zotero": "user"}, {"bibtex": None},
                       {"bibtex": True}, {"bibtex": [str(library)]}, {"bibtex": ""},
                       {"bibtex": "synthetic.bib"}, {"bibtex": "synthetic\0private-sentinel"},
                       {"bibtex": str(library), "zotero": "user"}, {"bibtex": str(root / "missing.bib")})
            for index, choice in enumerate(choices):
                saved["library"] = choice
                settings.write_text(json.dumps(saved), encoding="utf-8")
                with self.subTest(choice=index), self.assertRaisesRegex(AssertionError, "synthetic library choice was not preserved") as failure:
                    preserved_settings(settings, library)
                self.assertNotIn(str(root), str(failure.exception))
                self.assertNotIn("private-sentinel", str(failure.exception))
            settings = seed_settings(root, library)
            library.unlink()
            with self.assertRaisesRegex(AssertionError, "synthetic library choice was not preserved"):
                preserved_settings(settings, library)
            library.mkdir()
            with self.assertRaisesRegex(AssertionError, "synthetic library choice was not preserved"):
                preserved_settings(settings, library)

    def test_settings_verification_fails_closed_when_file_identity_cannot_be_read(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            library = root / "synthetic.bib"
            library.write_text("@misc{synthetic}\n", encoding="utf-8")
            settings = seed_settings(root, library)
            with patch.object(Path, "samefile", side_effect=PermissionError("synthetic-private-sentinel")):
                with self.assertRaisesRegex(AssertionError, "synthetic library choice was not preserved") as failure:
                    preserved_settings(settings, library)
            self.assertNotIn("synthetic-private-sentinel", str(failure.exception))

    def test_restarted_update_must_keep_choices_without_an_environment_override(self):
        for changed in ("library", "shortcut", "canonical", None):
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as temporary:
                runtime = Path(temporary) / "runtime"
                (runtime / "selected").mkdir(parents=True)
                failed = changed in ("library", "shortcut")
                def installed(folder, _args, library):
                    settings = folder / "settings.json"
                    choices = {"library": {"bibtex": str(folder / "synthetic.bib")}, "shortcut": "Control+Alt+Y"}
                    settings.write_text(json.dumps(choices), encoding="utf-8")
                    state = {"replaced": False}
                    def replaced():
                        state["replaced"] = True
                        if changed == "canonical":
                            canonical = str(library.resolve(strict=True))
                            self.assertNotEqual(canonical, str(library))
                            choices["library"] = {"bibtex": canonical}
                        elif changed:
                            choices[changed] = "synthetic wrong choice"
                        if changed:
                            settings.write_text(json.dumps(choices), encoding="utf-8")
                        return True
                    app = MagicMock(command=["synthetic"], config=None, settings=settings)
                    app.instances.side_effect = lambda: {43} if state["replaced"] else {42}
                    app.picker_ready.return_value = (42, "picker")
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
                        patch("os.kill"), patch("builtins.print"), patch(__name__ + ".feed_version", return_value="synthetic"), \
                        patch("tempfile.TemporaryDirectory") as sandbox:
                    sandbox.return_value.__enter__.return_value = str(runtime / "selected" / "..")
                    self.assertEqual(run(argparse.Namespace(output=Path(temporary))), int(failed))
                self.assertTrue("IBIS_LIBRARY" not in launch.call_args.kwargs["env"], "library override masks persistence")
                report = json.loads((Path(temporary) / "update-e2e.json").read_text(encoding="utf-8"))
                self.assertEqual(report["status"], "failed" if failed else "passed")
                if not failed:
                    self.assertEqual(report["settings_preserved"], {"library": True, "shortcut": True})

    def test_settings_verification_refuses_bad_files_without_disclosing_them(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            library = root / "synthetic 引用.bib"
            library.write_text("@misc{synthetic}\n", encoding="utf-8")
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

    def test_windows_observer_has_precise_isolated_arguments_without_inherited_disables(self):
        key = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"
        for inherited_args in (None, "--disable-renderer-accessibility --no-sandbox", "synthetic-private-sentinel" * 10000):
            inherited = {"PATH": "/synthetic/bin"}
            if inherited_args is not None:
                inherited[key] = inherited_args
            before = dict(inherited)
            with self.subTest(inherited=inherited_args is not None), patch("platform.system", return_value="Windows"):
                env = environment(inherited, None)
                self.assertEqual(env[key], "--force-renderer-accessibility")
                self.assertEqual(env["PATH"], inherited["PATH"])
                self.assertNotIn("IBIS_LIBRARY", env)
                self.assertEqual(inherited, before)

    def test_windows_observer_mode_reaches_both_owned_launches_and_is_reported(self):
        result, report, events = self.update_consumer("success", "Windows")
        self.assertEqual(result, 0, report)
        self.assertEqual(report["observer_mode"], "forced-renderer-accessibility")
        envs = [value for event, value in events if event == "environment"]
        self.assertEqual(len(envs), 2)
        self.assertTrue(all(env["WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"] == "--force-renderer-accessibility" for env in envs))
        self.assertEqual([value for event, value in events if event == "press"], [{42}])
        self.assertEqual(report["settings_preserved"], {"library": True, "shortcut": True})

    def test_environment_drops_an_outer_appimage(self):
        env = environment({"APPIMAGE": "/outer", "PATH": "/bin"}, Path("/c"))
        self.assertNotIn("APPIMAGE", env)
        self.assertEqual((env["XDG_CONFIG_HOME"], env["PATH"]), (str(Path("/c")), "/bin"))

    def test_environment_without_a_config_folder_uses_the_persisted_library(self):
        for system in ("Darwin", "Linux"):
            with self.subTest(system=system), patch("platform.system", return_value=system):
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

    @unittest.skipIf(os.name == "nt", "AppImage process roles use Linux symlinks")
    def test_linux_process_census_distinguishes_helpers_without_exposing_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            proc = Path(temporary)
            for pid, executable in ((42, "ibis-nightly"), (43, "WebKitWebProcess"), (44, "WebKitNetworkProcess"), (45, "synthetic-private-sentinel")):
                (proc / str(pid)).mkdir()
                (proc / str(pid) / "exe").symlink_to(Path("/synthetic-private-sentinel") / executable)
            facts = linux_process_roles({42, 43, 44, 45, 46}, proc)
            self.assertEqual(facts, {"app": 1, "webkit_web": 1, "webkit_network": 1, "other": 1, "unavailable": 1})
            self.assertNotIn("synthetic-private-sentinel", json.dumps(facts))

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
        probe = self.macos_probe(attempted=False, pid=None, focus=None)
        payload = json.dumps({"pressed": False, "seen": [expected], "probe": probe}, ensure_ascii=False).encode("utf-8")
        run_child = subprocess.run
        command = [sys.executable, "-c", f"import sys; sys.stdout.buffer.write(bytes.fromhex('{payload.hex()}'))"]
        mac = MacOS.__new__(MacOS)
        for helper in (lambda: look_result(powershell("Write-Output 'synthetic'")), lambda: mac.look([123])):
            with self.subTest(helper=helper), patch("locale.getencoding", return_value="cp1252"), \
                    patch("subprocess.run", side_effect=lambda _command, **kwargs: run_child(command, **kwargs)):
                self.assertEqual(helper(), (False, [expected], probe))

    def test_native_helpers_decode_binary_pipes_in_the_caller(self):
        probe = self.macos_probe(attempted=False, pid=None, focus=None)
        payload = json.dumps({"pressed": False, "seen": ["Synthetic 合成"], "probe": probe}, ensure_ascii=False).encode("utf-8")
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
                self.assertEqual(helper(), (False, ["Synthetic 合成"], probe))

    def test_native_helper_failure_keeps_decoded_utf8_diagnostics(self):
        failed = subprocess.CompletedProcess([], 1, stdout=b"", stderr="Synthetic 合成".encode("utf-8"))
        with patch("subprocess.run", return_value=failed):
            with self.assertRaisesRegex(RuntimeError, "helper failed: Synthetic 合成"):
                powershell("synthetic")
            with self.assertRaises(MacOSActivationFailure) as failure:
                MacOS.__new__(MacOS).look({123}, BUTTON)
            self.assertEqual(failure.exception.probe, {"stage": "macos-helper-exit", "code": 1})

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
        with patch("subprocess.run", side_effect=lambda _command, **kwargs: run_child(command, **kwargs)):
            with self.assertRaises(UnicodeDecodeError):
                powershell("synthetic")
            with self.assertRaises(MacOSActivationFailure) as failure:
                MacOS.__new__(MacOS).look({123}, BUTTON)
            self.assertEqual(failure.exception.probe, {"stage": "macos-helper-decode", "code": None})


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
