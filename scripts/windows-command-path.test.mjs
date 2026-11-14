import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

// Exercise the hook bodies in Tauri's uninstall lifecycle, without an installer,
// registry writes or NSIS. Unknown instructions fail instead of being ignored.
function uninstaller() {
  const source = readFileSync(new URL("../apps/desktop/src-tauri/packaging/path.nsh", import.meta.url), "utf8");
  const macros = new Map([...source.matchAll(/!macro (\w+)\s*\n([\s\S]*?)!macroend/g)].map(([, name, body]) => [name, body]));
  const installed = "$INSTDIR\\packaging\\path.ps1", temporary = "$PLUGINSDIR\\path.ps1";
  const files = new Set([installed]);
  let registered = true, errors = false, result = 0;
  const run = (name) => {
    const branches = [];
    for (const line of (macros.get(name) ?? "").split("\n").map((l) => l.trim()).filter((l) => l && !l.startsWith(";"))) {
      if (line.startsWith("${If}")) {
        assert.ok(["${If} ${Errors}", "${If} $0 != 0"].includes(line), line);
        branches.push(line.includes("${Errors}") ? errors : result !== 0);
      } else if (line === "${EndIf}") branches.pop();
      else if (branches.includes(false)) continue;
      else if (line === "InitPluginsDir") continue;
      else if (line === "ClearErrors") errors = false;
      else if (line.startsWith("CopyFiles /SILENT ")) {
        const [, from, to] = line.match(/^CopyFiles \/SILENT "([^"]+)" "([^"]+)"$/) ?? [];
        assert.ok(from && to, line);
        assert.equal(to, "$PLUGINSDIR");
        errors = !files.has(from);
        if (!errors) files.add(`${to}\\${from.split("\\").at(-1)}`);
      } else if (line.startsWith("nsExec::ExecToStack ")) {
        const [, helper, action] = line.match(/-File "([^"]+)" -Action (\w+)/) ?? [];
        assert.equal(action, "Remove");
        result = files.has(helper) ? 0 : 1;
        if (result === 0) registered = false;
      } else if (/^Pop \$[01]$/.test(line)) continue;
      else if (line.startsWith("Abort ")) throw new Error(line);
      else if (line.startsWith("Delete ")) files.delete(line.match(/^Delete "([^"]+)"$/)[1]);
      else assert.fail(`Unsupported NSIS instruction: ${line}`);
    }
    assert.equal(branches.length, 0);
  };
  return { run, files, installed, temporary, registered: () => registered };
}

test("cancelling the running-app prompt keeps the installed command on PATH", () => {
  const u = uninstaller();
  u.run("NSIS_HOOK_PREUNINSTALL");
  // Tauri checks for a running app after PREUNINSTALL; cancel skips deletion
  // and POSTUNINSTALL entirely.
  assert.ok(u.files.has(u.installed));
  assert.ok(u.registered());
});

test("successful uninstall removes PATH using the retained temporary helper", () => {
  const u = uninstaller();
  u.run("NSIS_HOOK_PREUNINSTALL");
  assert.ok(u.registered(), "PATH is unchanged until the app is removed");
  u.files.delete(u.installed);
  u.run("NSIS_HOOK_POSTUNINSTALL");
  assert.ok(!u.registered());
  assert.ok(!u.files.has(u.temporary), "the temporary helper is cleaned up");
});

test("a missing uninstall helper refuses removal while preserving PATH", () => {
  const u = uninstaller();
  u.files.delete(u.installed);
  assert.throws(() => u.run("NSIS_HOOK_PREUNINSTALL"), /Abort/);
  assert.ok(u.registered());
});

// Installers use Windows PowerShell 5.1. Require it on Windows, and exercise
// PowerShell 7 as well wherever pwsh is installed.
const shells = process.platform === "win32" ? ["powershell.exe", "pwsh"] : ["pwsh"];
for (const shell of shells) {
  const probe = spawnSync(shell, ["-NoProfile", "-NonInteractive", "-Command", "$PSVersionTable.PSVersion.ToString()"], {
    encoding: "utf8", timeout: 10_000,
  });
  test(`${shell}: PATH updates preserve edits, serialize channels, and reject overflow`, {
    skip: shell === "pwsh" && probe.error?.code === "ENOENT" ? "PowerShell 7 is not installed" : false,
  }, () => {
    assert.equal(probe.status, 0, probe.error?.message ?? probe.stderr);
    if (shell === "powershell.exe") assert.match(probe.stdout.trim(), /^5\.1(?:\.|$)/, "Installers require Windows PowerShell 5.1");
    const result = spawnSync(shell, ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", fileURLToPath(new URL("./command-path.tests.ps1", import.meta.url))], {
      encoding: "utf8", timeout: 60_000,
    });
    assert.equal(result.status, 0, result.error?.message ?? result.stdout + result.stderr);
  });
}
