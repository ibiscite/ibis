import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { budgetTest } from "./preview-budgets.mjs";

test("the approved release boundary reports before 0.3 and enforces every later tag", () => {
  for (const ref of ["refs/heads/main", "refs/heads/fix/preview", "refs/tags/v0.1.1", "refs/tags/v0.2.0", "refs/tags/v0.2.99"]) assert.equal(budgetTest(ref), "open_refresh_report", ref);
  for (const ref of ["refs/tags/v0.3.0", "refs/tags/v0.3.0-rc.1", "refs/tags/v0.10.0", "refs/tags/v1.0.0", "refs/tags/v12.0.0"]) assert.equal(budgetTest(ref), "open_refresh_budget", ref);
  for (const ref of ["", "v0.2.1", "refs/tags/v0.02.1", "refs/tags/vgarbage", "refs/tags/v0.2.1\nprivate", "refs/tags/nightly"]) assert.throws(() => budgetTest(ref), /Invalid build ref/, ref);
});

test("real command failures stay fatal in report mode and numbers reach the job summary", { skip: process.platform === "win32" }, (t) => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-budget-policy-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(join(dir, "cargo"), '#!/bin/sh\nprintf "%s\\n" "$*"\nprintf "%s\\n" "100k budget: input=21053739bytes open=4110ms refresh=3359ms"\nexit "$TEST_EXIT"\n', { mode: 0o755 });
  for (const code of [0, 101]) {
    const summary = join(dir, `summary-${code}`);
    const result = spawnSync(process.execPath, ["scripts/preview-budgets.mjs", "refs/heads/main"], { encoding: "utf8", timeout: 5000,
      env: { ...process.env, PATH: `${dir}:${process.env.PATH}`, TEST_EXIT: String(code), GITHUB_STEP_SUMMARY: summary } });
    assert.equal(result.status, code === 0 ? 0 : 1, result.stderr);
    assert.match(result.stdout, /open_refresh_report --ignored/);
    assert.match(readFileSync(summary, "utf8"), /Report only/);
    assert.match(readFileSync(summary, "utf8"), /open=4110ms refresh=3359ms/);
  }
});

test("Preview measures after preserving packages and keeps native paste outside packaging", () => {
  const preview = readFileSync(new URL("../.github/workflows/preview.yml", import.meta.url), "utf8");
  const build = preview.split("\n  build:\n")[1].split("\n  windows-paste:\n")[0];
  assert.ok(build.indexOf("node scripts/preview-budgets.mjs") > build.indexOf("- name: Keep the packages"));
  assert.doesNotMatch(build, /one_ctrl_v_pastes/);
  assert.match(preview, /windows-paste:[\s\S]*?needs: build[\s\S]*?one_ctrl_v_pastes_the_clipboard_into_the_app_in_front/);
  const install = preview.split("\n  install:\n")[1].split(/\n  [a-z][\w-]*:\n/)[0];
  assert.match(install, /needs: \[build, sign-macos, windows-paste\]/);
  assert.match(install, /needs.windows-paste.result == 'success'/);
  const ci = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
  assert.match(ci, /Paste into Notepad \(Windows\)[\s\S]*?runner.os == 'Windows'[\s\S]*?one_ctrl_v_pastes_the_clipboard_into_the_app_in_front/);
});
