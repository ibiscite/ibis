#!/usr/bin/env python3
"""End-to-end update test for Ibis Nightly (#319, #493).

Installs the previous Ibis Nightly with automatic update checks on, waits for
it to offer the newly published nightly, presses "Install and Restart" through
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
import urllib.request
from pathlib import Path

IDENTIFIER = "io.github.ibiscite.ibis.nightly"
BUTTON = "Install and Restart"
# The app's first check comes 30 seconds after start; the download is ~100 MB.
OFFER_TIMEOUT = 180
REPLACE_TIMEOUT = 300
RESTART_TIMEOUT = 60
UNINSTALL = r"Software\Microsoft\Windows\CurrentVersion\Uninstall"

# Lists the named processes' windows, buttons and texts, or presses the named
# button. Prints {"pressed": bool, "seen": [...]}.
WINDOWS_LOOK = r"""
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]
$T = [System.Windows.Automation.ControlType]
$seen = [System.Collections.Generic.List[string]]::new()
foreach ($id in ($env:IBIS_PIDS -split ',' | Where-Object { $_ })) {
  $condition = [System.Windows.Automation.PropertyCondition]::new($A::ProcessIdProperty, [int]$id)
  foreach ($window in $A::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $condition)) {
    try {
      $seen.Add("[$id] window: " + $window.Current.Name)
      foreach ($item in $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) {
        $type = $item.Current.ControlType
        $name = $item.Current.Name
        if (-not $name -or ($type -ne $T::Button -and $type -ne $T::Text)) { continue }
        if ($type -eq $T::Button -and $env:IBIS_PRESS -and $name.Replace('&', '') -eq $env:IBIS_PRESS) {
          $item.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
          @{ pressed = $true; seen = @() } | ConvertTo-Json -Compress
          exit 0
        }
        if ($seen.Count -lt 200) { $seen.Add("[$id] " + $type.ProgrammaticName.Replace('ControlType.', '') + ": " + $name) }
      }
    } catch { continue }  # Elements vanish while windows change.
  }
}
@{ pressed = $false; seen = @($seen) } | ConvertTo-Json -Compress
"""

MACOS_LOOK = r"""
function run(argv) {
  const [press, ...pids] = argv;
  const events = Application('System Events');
  const seen = [];
  const text = (item) => { try { return String(item.name() || item.value() || ''); } catch (e) { return ''; } };
  const windows = (pid) => {
    try { return [].concat(...events.processes.whose({ unixId: Number(pid) })().map((app) => app.windows())); }
    catch (e) { return []; }  // The process quit while being read.
  };
  for (const pid of pids) {
    for (const window of windows(pid)) {
      try {
        seen.push(`[${pid}] window: ${text(window)}`);
        for (const place of [window].concat(window.sheets())) {
          for (const button of place.buttons()) {
            if (press && text(button) === press) {
              button.click();
              return JSON.stringify({ pressed: true, seen: [] });
            }
            seen.push(`[${pid}] button: ${text(button)}`);
          }
          for (const label of place.staticTexts()) seen.push(`[${pid}] text: ${text(label)}`);
        }
      } catch (e) { continue; }  // Windows change while being read.
    }
  }
  return JSON.stringify({ pressed: false, seen: seen.slice(0, 200) });
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
    """The helpers' answer; anything else is a helper failure, never a pass."""
    result = json.loads(stdout)
    if not isinstance(result, dict) or type(result.get("pressed")) is not bool or not isinstance(result.get("seen"), list):
        raise RuntimeError("accessibility helper returned an unexpected answer")
    return result["pressed"], [str(item) for item in result["seen"]]


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
                        return True, []
                    if role in ("frame", "dialog", "alert", "push button", "label") and item.name and len(seen) < 200:
                        seen.append(f"{app.name}[{app.get_process_id()}] {role}: {item.name[:200]}")
                    stack.extend(child for child in item if child)
                except Exception:  # Accessibility objects vanish while windows change.
                    continue
        return False, seen

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
                                capture_output=True, text=True, timeout=60)
        if result.returncode:
            raise RuntimeError(f"System Events helper failed: {result.stderr.strip()[-500:]}")
        return look_result(result.stdout)

    def close(self):
        clean_settings(self.settings)


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
        return look_result(powershell(WINDOWS_LOOK, IBIS_PIDS=",".join(map(str, pids)), IBIS_PRESS=press or ""))

    def close(self):
        uninstaller = self.folder / "uninstall.exe"
        if uninstaller.exists():
            subprocess.run([str(uninstaller), "/S"], timeout=120)
        clean_settings(self.settings)


def powershell(script, **env):
    """Runs a script from a file: commands read from standard input end at the
    first line break inside a block. Values arrive as environment variables."""
    with tempfile.TemporaryDirectory(prefix="ibis update helper ") as folder:
        path = Path(folder) / "helper.ps1"
        path.write_text(script, encoding="utf-8")
        result = subprocess.run(["pwsh", "-NoProfile", "-NonInteractive", "-File", str(path)],
                                capture_output=True, text=True, timeout=120, env=dict(os.environ, **env))
    if result.returncode:
        raise RuntimeError(f"PowerShell helper failed: {result.stderr.strip()[-500:]}")
    return result.stdout


def uninstall_entries():
    import winreg
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, UNINSTALL) as root:
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
            seen = {"helper": None}
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
                        return install.look(seen["old"], BUTTON)[0]
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
                if seen["helper"]:
                    report["last_helper_problem"] = seen["helper"]
                if install:
                    report.update(install.facts())
                    try:
                        report["on_screen"] = install.look(install.instances())[1]
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

    def test_helper_answers_are_checked(self):
        self.assertEqual(look_result('{"pressed": false, "seen": ["[1] button: Later"]}'), (False, ["[1] button: Later"]))
        self.assertEqual(look_result('{"pressed": true, "seen": []}'), (True, []))
        for bad in ('{"pressed": "yes", "seen": []}', '{"seen": []}', '[]', '{"pressed": true, "seen": "x"}'):
            with self.assertRaises(RuntimeError):
                look_result(bad)


def main():
    if sys.argv[1:] == ["--self-test"]:
        unittest.main(argv=[sys.argv[0]])
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
