import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./install-and-start.sh", import.meta.url));
const skip = process.platform === "win32";

function fixture(t, platform) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-install-smoke-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  for (const folder of ["bin", "dist", "temp", "installed"]) mkdirSync(join(dir, folder));
  const stub = (name, source) => writeFileSync(join(dir, "bin", name), `#!/usr/bin/env node\n${source}`, { mode: 0o755 });
  const desktop = platform === "macOS" ? "custom-desktop" : "ibis-desktop.exe";
  // Force the old find/head lookup to see the command before the desktop app.
  stub("find", String.raw`const { execFileSync } = require("node:child_process");
const files = execFileSync("/usr/bin/find", process.argv.slice(2), { encoding: "utf8" }).trim().split("\n");
console.log(files.sort((a, b) => Number(b.endsWith("/ibis") || b.endsWith("/ibis.exe")) - Number(a.endsWith("/ibis") || a.endsWith("/ibis.exe"))).join("\n"));`);
  const smoke = `const { appendFileSync } = require("node:fs");
const { basename } = require("node:path");
const app = process.argv.at(-1);
appendFileSync(process.env.STARTED, basename(app) + "\\n");
if (basename(app) !== process.env.DESKTOP) process.exit(42);`;
  stub("python", smoke);
  stub("python3", smoke);
  return {
    dir, stub, desktop,
    run: () => spawnSync("bash", [script], { cwd: dir, encoding: "utf8", timeout: 120_000, // a hang guard, not a budget: stub starts are slow on a loaded machine
      env: { ...process.env, PATH: `${join(dir, "bin")}:${dirname(process.execPath)}:${process.env.PATH}`, RUNNER_OS: platform,
        RUNNER_TEMP: join(dir, "temp"), FIXTURE: dir, DESKTOP: desktop, STARTED: join(dir, "started") } }),
    started: () => readFileSync(join(dir, "started"), "utf8").trim().split("\n"),
  };
}

test("macOS install smoke starts CFBundleExecutable when the sidecar is listed first", { skip }, (t) => {
  const f = fixture(t, "macOS");
  const app = join(f.dir, "temp/ibis-dmg/Ibis.app");
  mkdirSync(join(app, "Contents/MacOS"), { recursive: true });
  writeFileSync(join(app, "Contents/Info.plist"), `<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>${f.desktop}</string></dict></plist>`);
  for (const name of ["ibis", f.desktop]) writeFileSync(join(app, "Contents/MacOS", name), "invented executable", { mode: 0o755 });
  f.stub("hdiutil", "");
  f.stub("codesign", "");
  f.stub("ditto", `require("node:fs").cpSync(process.argv[2], process.argv[3], { recursive: true });`);
  if (process.platform !== "darwin") f.stub("plutil", String.raw`const { readFileSync } = require("node:fs");
const args = process.argv.slice(2);
if (args.join(" ").startsWith("-extract CFBundleExecutable raw -o - ") === false) process.exit(2);
console.log(readFileSync(args.at(-1), "utf8").match(/<key>CFBundleExecutable<\/key><string>([^<]+)<\/string>/)[1]);`);
  const result = f.run();
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.deepEqual(f.started(), [f.desktop]);
});

test("Windows NSIS smoke starts ibis-desktop.exe with a CLI sidecar present", { skip }, (t) => {
  const f = fixture(t, "Windows");
  const populate = String.raw`for (const name of ["ibis.exe", "ibis-desktop.exe"]) fs.writeFileSync(path.join(dir, name), "invented executable");
fs.writeFileSync(path.join(dir, "uninstall.exe"), "#!/usr/bin/env node\nrequire('node:fs').rmSync(require('node:path').join(__dirname, 'ibis-desktop.exe'));\n");
fs.chmodSync(path.join(dir, "uninstall.exe"), 0o755);`;
  f.stub("pwsh", `const fs = require("node:fs"), path = require("node:path");
const dir = path.join(process.env.FIXTURE, "installed");
const command = process.argv.at(-1);
if (command.includes("Get-ChildItem")) {
  if (fs.existsSync(path.join(process.env.FIXTURE, "installed-nsis"))) console.log("invented-key");
} else if (command.includes("Get-ItemProperty")) console.log(path.join(dir, "uninstall.exe"));
else process.exit(2);`);
  f.stub("cygpath", "console.log(process.argv.at(-1));");
  // Synthetic installers only create/remove fixture files; no installer is built or run.
  writeFileSync(join(f.dir, "dist/Ibis-Windows-setup.exe"), `#!/usr/bin/env node
const fs = require("node:fs"), path = require("node:path");
const dir = path.join(process.env.FIXTURE, "installed");
${populate}
fs.writeFileSync(path.join(process.env.FIXTURE, "installed-nsis"), "yes");`, { mode: 0o755 });
  const result = f.run();
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.deepEqual(f.started(), [f.desktop]);
});
