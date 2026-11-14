import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./install-and-start.sh", import.meta.url));
const skip = process.platform === "win32";
const version = "ibis 7.8.9-preview.2+fixture\n";
const failures = {
  missing: {},
  "not executable": {},
  "failed process": { output: version, exit: 9 },
  empty: { output: "" },
  "wrong name": { output: "other 7.8.9-preview.2+fixture\n" },
  malformed: { output: "ibis not-a-version\n" },
  "extra line": { output: `${version}unexpected output\n` },
  "wrong version": { output: "ibis 7.8.10-preview.2+fixture\n" },
  "wrong prerelease": { output: "ibis 7.8.9-preview.3+fixture\n" },
  "wrong build": { output: "ibis 7.8.9-preview.2+other\n" },
};

// Synthetic packages and executable stubs exercise the script, not native installers.
function fixture(t, platform, brokenAt, failure) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-install-smoke-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  for (const folder of ["bin", "cli", "dist", "temp", "installed app", "packages"]) mkdirSync(join(dir, folder));
  const executable = (path, source, mode = 0o755) => writeFileSync(path, `#!/usr/bin/env node\n${source}`, { mode });
  const stub = (name, source) => executable(join(dir, "bin", name), source);
  const desktop = platform === "macOS" ? "custom-desktop" : "ibis-desktop.exe";
  const command = platform === "macOS" ? "ibis" : "ibis.exe";
  const commandFile = (folder, label) => {
    const kind = label === brokenAt ? failure : undefined;
    if (kind === "missing") return;
    const answer = failures[kind] ?? {};
    executable(join(folder, command), `
require("node:assert/strict").deepEqual(process.argv.slice(2), ["--version"]);
require("node:fs").appendFileSync(process.env.EVENTS, ${JSON.stringify(`${label}:version\n`)});
process.stdout.write(${JSON.stringify(answer.output ?? version)});
process.exit(${answer.exit ?? 0});`, kind === "not executable" ? 0o644 : 0o755);
  };
  commandFile(join(dir, "cli"), "reference");
  for (const stage of platform === "macOS" ? ["macOS"] : ["NSIS", "MSI"]) {
    const folder = join(dir, "packages", stage);
    mkdirSync(folder);
    commandFile(folder, stage);
    executable(join(folder, desktop), `
require("node:assert/strict").deepEqual(process.argv.slice(2), []);
require("node:fs").appendFileSync(process.env.EVENTS, ${JSON.stringify(`${stage}:desktop\n`)});`);
  }
  // Execute the selected desktop stub through the same startup boundary as the script.
  const smoke = `
require("node:assert/strict").equal(process.argv[2], "scripts/stays-up.py");
require("node:child_process").execFileSync(process.argv[3], [], { stdio: "inherit" });`;
  stub("python", smoke);
  stub("python3", smoke);

  if (platform === "macOS") {
    // Any find/head executable lookup must see ibis before CFBundleExecutable.
    stub("find", String.raw`const { execFileSync } = require("node:child_process");
const files = execFileSync("/usr/bin/find", process.argv.slice(2), { encoding: "utf8" }).trim().split("\n");
console.log(files.sort((a, b) => Number(b.endsWith("/ibis")) - Number(a.endsWith("/ibis"))).join("\n"));`);
    const bundle = join(dir, "temp/ibis-dmg/Ibis.app/Contents");
    mkdirSync(bundle, { recursive: true });
    writeFileSync(join(bundle, "Info.plist"), `<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>${desktop}</string></dict></plist>`);
    stub("hdiutil", `
if (process.argv[2] === "attach") require("node:fs").cpSync(
  require("node:path").join(process.env.FIXTURE, "packages/macOS"),
  require("node:path").join(process.env.FIXTURE, "temp/ibis-dmg/Ibis.app/Contents/MacOS"), { recursive: true });`);
    stub("codesign", "");
    stub("ditto", `require("node:fs").cpSync(process.argv[2], process.argv[3], { recursive: true });`);
    if (process.platform !== "darwin") stub("plutil", String.raw`
const args = process.argv.slice(2);
require("node:assert/strict").deepEqual(args.slice(0, -1), ["-extract", "CFBundleExecutable", "raw", "-o", "-"]);
console.log(require("node:fs").readFileSync(args.at(-1), "utf8").match(/<key>CFBundleExecutable<\/key><string>([^<]+)<\/string>/)[1]);`);
  } else {
    const common = `
const fs = require("node:fs"), path = require("node:path");
const dir = path.join(process.env.FIXTURE, "installed app");`;
    const copy = `fs.cpSync(path.join(process.env.FIXTURE, "packages", stage), dir, { recursive: true });`;
    stub("pwsh", `
${common}
const command = process.argv.at(-1);
if (command.includes("Get-ChildItem")) {
  if (fs.existsSync(path.join(process.env.FIXTURE, "installed-nsis"))) console.log("invented-key");
} else if (command.includes("Get-ItemProperty")) console.log(path.join(dir, "uninstall.exe"));
else if (process.env.MODE === "i") {
  const stage = "MSI";
  ${copy}
  fs.writeFileSync(process.env.LOG, "Property(S): INSTALLDIR = " + dir + "\\n");
} else if (process.env.MODE === "x") fs.rmSync(dir, { recursive: true });
else process.exit(2);`);
    stub("cygpath", "console.log(process.argv.at(-1));");
    executable(join(dir, "dist/Ibis-Windows-setup.exe"), `
require("node:assert/strict").deepEqual(process.argv.slice(2), ["//S"]);
${common}
const stage = "NSIS";
${copy}
fs.writeFileSync(path.join(dir, "uninstall.exe"), \`#!/usr/bin/env node
require("node:assert/strict").deepEqual(process.argv.slice(2), ["//S"]);
require("node:fs").rmSync(__dirname, { recursive: true });\`, { mode: 0o755 });
fs.writeFileSync(path.join(process.env.FIXTURE, "installed-nsis"), "yes");`);
    writeFileSync(join(dir, "dist/Ibis-Windows.msi"), "invented MSI");
  }

  const eventsFile = join(dir, "events");
  return {
    run: () => spawnSync("bash", [script], {
      cwd: dir, encoding: "utf8", timeout: 120_000,
      env: {
        ...process.env, PATH: `${join(dir, "bin")}:${dirname(process.execPath)}:${process.env.PATH}`,
        RUNNER_OS: platform, RUNNER_TEMP: join(dir, "temp"), FIXTURE: dir,
        EVENTS: eventsFile, IBIS_INSTALLED_JOURNEY: "0",
      },
    }),
    events: () => existsSync(eventsFile) ? readFileSync(eventsFile, "utf8").trim().split("\n") : [],
  };
}

for (const platform of ["macOS", "Windows"]) {
  test(`${platform} validates the installed commands before starting the desktop`, { skip }, (t) => {
    const f = fixture(t, platform);
    const result = f.run();
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.deepEqual(f.events(), (platform === "macOS" ? ["macOS"] : ["NSIS", "MSI"]).flatMap((stage) => [
      "reference:version", `${stage}:version`, `${stage}:desktop`,
    ]));
  });
}

for (const stage of ["macOS", "NSIS", "MSI"]) {
  for (const failure of Object.keys(failures)) {
    test(`${stage}: ${failure} command stops before its desktop starts`, { skip }, (t) => {
      const f = fixture(t, stage === "macOS" ? "macOS" : "Windows", stage, failure);
      const result = f.run();
      assert.equal(result.status, 1, result.stdout + result.stderr);
      assert.match(result.stdout + result.stderr, /::error::.*[Cc]ommand/);
      assert.deepEqual(f.events().filter((event) => event.endsWith(":desktop")), stage === "MSI" ? ["NSIS:desktop"] : []);
    });
  }
}

for (const platform of ["macOS", "Windows"]) {
  for (const failure of ["missing", "not executable", "failed process", "empty", "wrong name", "malformed", "extra line"]) {
    test(`${platform}: ${failure} reference stops before desktop startup`, { skip }, (t) => {
      const f = fixture(t, platform, "reference", failure);
      const result = f.run();
      assert.equal(result.status, 1, result.stdout + result.stderr);
      assert.match(result.stdout + result.stderr, /::error::.*[Cc]ommand/);
      assert.deepEqual(f.events().filter((event) => event.endsWith(":desktop")), []);
    });
  }
}
