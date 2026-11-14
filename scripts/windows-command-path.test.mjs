import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

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
