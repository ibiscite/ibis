import assert from "node:assert/strict";
import { test } from "node:test";
import { lint } from "./pr-lint.mjs";

const body = (extra = "") => `Closes #12

## What and why

Duplicate keys no longer drop good entries.

## How it works

The parser keeps the first entry.

## Verification

- [x] A test fails without this change
${extra}`;
const ok = { title: "fix: keep good entries on duplicate keys", body: body(), branch: "fix/12-duplicate-keys" };

test("accepts a pull request that follows the standard", () => {
  assert.deepEqual(lint(ok), { errors: [], warnings: [] });
  assert.deepEqual(lint({ ...ok, title: "feat(cli)!: drop the old flag", branch: "feat/12-old-flag" }).errors, []);
});

test("rejects titles that are not short Conventional Commits", () => {
  for (const title of ["Fix duplicate keys", "fix:no space", "wip: thing", "fix(Desktop): x", "fix: ", `fix: ${"a".repeat(70)}`, "fix: keep entries."]) {
    assert.ok(lint({ ...ok, title }).errors.length > 0, title);
  }
  assert.deepEqual(lint({ ...ok, title: "fix: Keep entries" }).warnings, ["title should start in lower case after the colon"]);
  assert.deepEqual(lint({ ...ok, title: "fix: Zotero 7 keys" }).warnings, ["title should start in lower case after the colon"]);
  assert.deepEqual(lint({ ...ok, title: "fix: IEEE keys" }).warnings, []);
});

test("only suggests a branch name, and skips forks, bots and reverts", () => {
  assert.deepEqual(lint({ ...ok, branch: "claude/x" }).errors, []);
  assert.match(lint({ ...ok, branch: "claude/x" }).warnings[0], /could be named <type>\/<issue>-<short-name>/);
  assert.deepEqual(lint({ ...ok, branch: "feat/12-thing" }), { errors: [], warnings: [] });
  assert.deepEqual(lint({ ...ok, branch: "patch-1", fork: true }), { errors: [], warnings: [] });
  assert.deepEqual(lint({ title: "chore(main): release 0.2.0", body: "", branch: "release-please--branches--main" }), { errors: [], warnings: [] });
  assert.deepEqual(lint({ title: "build(deps): bump x", body: "", branch: "dependabot/cargo/x-1.2" }), { errors: [], warnings: [] });
  assert.deepEqual(lint({ title: 'Revert "fix: keep notes literal (#12)"', body: "Reverts ibiscite/ibis#12", branch: "revert-12-fix/11-notes" }), { errors: [], warnings: [] });
  assert.ok(lint({ title: 'Revert "fix: x"', body: "", branch: "fix/11-notes" }).errors.length > 0);
});

test("warns, but does not fail, without an issue link", () => {
  assert.deepEqual(lint({ ...ok, body: body().replace("Closes #12", "") }).errors, []);
  assert.match(lint({ ...ok, body: body().replace("Closes #12", "<!-- Closes #12 -->") }).warnings[0], /no "Closes #N"/);
  assert.deepEqual(lint({ ...ok, body: body().replace("Closes #12", "Part of #12") }), { errors: [], warnings: [] });
  assert.deepEqual(lint({ ...ok, body: `${body()}\nCloses #13` }).errors, []);
});

test("accepts short or long bodies, evidence links and any headings", () => {
  for (const extra of ["w ".repeat(600), "https://github.com/o/r/actions/runs/123", `${"a".repeat(40)} ${"b".repeat(40)}`]) {
    assert.deepEqual(lint({ ...ok, body: body(extra) }).errors, [], extra.slice(0, 20));
  }
  assert.deepEqual(lint({ ...ok, body: "Closes #12\n\nShort." }), { errors: [], warnings: [] });
  assert.deepEqual(lint({ ...ok, body: null }).errors, []);
});

test("keeps session notes out of the body", () => {
  assert.match(lint({ ...ok, body: `_Requested by **Someone**_\n${body()}` }).warnings[0], /session or pause notes/);
  assert.match(lint({ ...ok, body: body("## Paused here") }).warnings[0], /session or pause notes/);
});
