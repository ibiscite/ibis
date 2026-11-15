import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./ci-scope.sh", import.meta.url));
const posix = { skip: process.platform === "win32" };
const all = 'os=["ubuntu-24.04","macos-15","windows-2025"]\n', ubuntu = 'os=["ubuntu-24.04"]\n';

function fixture(t) {
  const cwd = mkdtempSync(join(tmpdir(), "ibis-ci-scope-"));
  t.after(() => rmSync(cwd, { recursive: true, force: true }));
  writeFileSync(join(cwd, "git"), '#!/bin/sh\n[ "$DIFF_STATUS" = 0 ] || exit "$DIFF_STATUS"\nprintf "%s\\n" "$FILES"\n', { mode: 0o755 });
  return (env) => spawnSync("bash", [script], { cwd, encoding: "utf8", timeout: 5000,
    env: { ...process.env, PATH: `${cwd}${delimiter}${process.env.PATH}`, DIFF_STATUS: "0", FILES: "", EVENT: "pull_request", FULL_LABEL: "false", ...env } }).stdout;
}

test("pull requests run Ubuntu alone unless they touch platform code or ask for every system", posix, (t) => {
  const run = fixture(t);
  for (const files of ["docs/testing.md", "crates/ibis-engine/src/lib.rs\napps/desktop/src/settings/PreferenceWindows.test.tsx", "apps/desktop/src-tauri/src/settings_window.rs"]) {
    assert.equal(run({ FILES: files }), ubuntu, files);
  }
  for (const files of [
    "apps/desktop/src-tauri/src/paste/windows.rs", "apps/desktop/src-tauri/src/placement/mod.rs",
    "crates/ibis-zotero-local/src/closed/platform/windows.rs", "scripts/check-offline.ps1", "scripts/macos-sign.sh",
    "apps/desktop/src-tauri/tauri.bundle.conf.json", "apps/desktop/src-tauri/packaging/path.wxs", "apps/desktop/src-tauri/Info.plist",
    "Cargo.lock", ".github/workflows/ci.yml", "docs/readme.md\napps/desktop/gallery/webkit.swift",
  ]) {
    assert.equal(run({ FILES: files }), all, files);
  }
  assert.equal(run({ FULL_LABEL: "true" }), all);
  assert.equal(run({ DIFF_STATUS: "128", FILES: "docs/testing.md" }), all);
  for (const EVENT of ["push", "schedule", "workflow_dispatch"]) assert.equal(run({ EVENT, FILES: "docs/testing.md" }), all, EVENT);
});
