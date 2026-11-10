import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

// Run the workflow's Windows helper with only pnpm and the delay replaced.
const script = fileURLToPath(new URL("./build-windows-packages.sh", import.meta.url));
const posix = { skip: process.platform === "win32" };
const target = "x86_64-pc-windows-msvc";
const assets = [
  "https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.13/nsis-3.13.zip",
  "https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.5.3/nsis_tauri_utils.dll",
];
const gateway = 'failed to bundle project: `http status: 504`';

function run(t, { nightly = false, failures = [], buildStatus = 0 } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-win-bundle-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const desktop = join(dir, "apps/desktop"), bin = join(dir, "bin"), calls = join(dir, "calls");
  mkdirSync(desktop, { recursive: true });
  mkdirSync(bin);
  const executable = join(dir, "target", target, "release", "ibis-desktop.exe");
  writeFileSync(join(bin, "pnpm"), `#!/usr/bin/env node
const { appendFileSync, mkdirSync, readFileSync, writeFileSync } = require("node:fs");
const { dirname } = require("node:path");
const args = process.argv.slice(2);
const binary = process.env.STUB_EXE;
const log = process.env.STUB_CALLS;
appendFileSync(log, JSON.stringify(args) + "\\n");
const original = "invented pristine executable";
if (args[1] === "build") {
  if (Number(process.env.STUB_BUILD_STATUS)) {
    console.log("Downloading " + process.env.STUB_ASSET);
    console.error(process.env.STUB_GATEWAY);
    process.exit(Number(process.env.STUB_BUILD_STATUS));
  }
  mkdirSync(dirname(binary), { recursive: true });
  writeFileSync(binary, original);
  if (args.includes("--no-bundle")) process.exit(0);
}
if (readFileSync(binary, "utf8") !== original) throw new Error("bundler received an already patched executable");
const attempt = readFileSync(log, "utf8").trim().split("\\n").map(JSON.parse).filter((call) => call[1] === "bundle").length;
const failure = JSON.parse(process.env.STUB_FAILURES)[Math.max(0, attempt - 1)];
if (failure) {
  // Tauri restores the binary only after the bundle succeeds.
  writeFileSync(binary, "invented patched executable");
  for (const asset of failure.downloads) process.stdout.write(" Downloading " + asset + "\\r\\n");
  console.error(failure.message);
  process.exit(43);
}
console.log("packaged pristine executable");
`, { mode: 0o755 });
  writeFileSync(join(bin, "sleep"), '#!/bin/sh\nprintf \'["sleep","%s"]\\n\' "$1" >> "$STUB_CALLS"\n', { mode: 0o755 });

  const config = nightly ? join(dir, "nightly config.json") : "";
  const result = spawnSync("bash", [script], {
    cwd: desktop, encoding: "utf8", timeout: 10_000,
    env: { ...process.env, PATH: `${bin}${delimiter}${process.env.PATH}`, RUNNER_OS: "Windows", TARGET: target,
      CONFIG: config, STUB_CALLS: calls, STUB_EXE: executable, STUB_FAILURES: JSON.stringify(failures),
      STUB_BUILD_STATUS: String(buildStatus), STUB_ASSET: assets[0], STUB_GATEWAY: gateway },
  });
  const flags = ["--ci", "--target", target, ...(nightly ? ["--config", config] : [])];
  return { ...result, calls: readFileSync(calls, "utf8").trim().split("\n").map(JSON.parse),
    build: ["tauri", "build", "--no-bundle", ...flags, "--", "--locked"],
    bundle: ["tauri", "bundle", ...flags, "--features", "tauri/custom-protocol"] };
}

test("a stable Windows build packages its compiled executable once", posix, (t) => {
  const result = run(t);
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.deepEqual(result.calls, [result.build, result.bundle]);
});

test("either pinned NSIS helper returning 504 retries packaging with the pristine nightly executable", posix, (t) => {
  for (const asset of assets) {
    const result = run(t, { nightly: true, failures: [{ downloads: [asset], message: gateway }] });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.match(result.stdout, /packaged pristine executable/);
    assert.deepEqual(result.calls, [result.build, result.bundle, ["sleep", "15"], result.bundle]);
  }
});

test("a compiler failure or a different bundle failure stops Windows packaging", posix, (t) => {
  const build = run(t, { buildStatus: 42 });
  assert.equal(build.status, 42, build.stdout + build.stderr);
  assert.deepEqual(build.calls, [build.build]);

  for (const failure of [
    { downloads: [assets[0], "https://example.invalid/other.zip"], message: gateway },
    { downloads: [assets[1]], message: "failed to bundle project: hash mismatch" },
  ]) {
    const result = run(t, { failures: [failure] });
    assert.equal(result.status, 43, result.stdout + result.stderr);
    assert.deepEqual(result.calls, [result.build, result.bundle]);
  }
});

test("a second gateway failure stops Windows packaging", posix, (t) => {
  const result = run(t, { failures: assets.map((asset) => ({ downloads: [asset], message: gateway })) });
  assert.equal(result.status, 43, result.stdout + result.stderr);
  assert.deepEqual(result.calls, [result.build, result.bundle, ["sleep", "15"], result.bundle]);
});
