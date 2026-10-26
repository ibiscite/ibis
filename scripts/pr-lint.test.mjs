import assert from "node:assert/strict";
import { test } from "node:test";
import { lint } from "./pr-lint.mjs";

const clean = { errors: [], warnings: [] };
test("accepts release-relevant Conventional Commit titles", () => {
  for (const type of ["feat", "fix", "perf", "docs", "test", "ci", "build", "refactor", "chore", "revert"]) {
    assert.deepEqual(lint({ title: `${type}: keep good entries` }), clean);
    assert.deepEqual(lint({ title: `${type}(cli)!: drop the old flag` }), clean);
  }
});

test("keeps the existing length, punctuation, scope and case rules", () => {
  for (const title of ["Fix duplicate keys", "fix:no space", "wip: thing", "fix(Desktop): x", "fix: ", `fix: ${"a".repeat(70)}`, "fix: keep entries."]) {
    assert.ok(lint({ title }).errors.length > 0, title);
  }
  assert.deepEqual(lint({ title: `fix: ${"a".repeat(67)}` }), clean);
  for (const title of ["fix: Keep entries", "fix: Zotero 7 keys"]) {
    assert.deepEqual(lint({ title }).warnings, ["title should start in lower case after the colon"]);
  }
  assert.deepEqual(lint({ title: "fix: IEEE keys" }), clean);
});

test("retains bot and GitHub Revert handling", () => {
  for (const branch of ["release-please--branches--main", "dependabot/cargo/x-1.2"]) {
    assert.deepEqual(lint({ title: "bot title", branch }), clean);
  }
  assert.deepEqual(lint({ title: 'Revert "fix: keep notes literal (#12)"', branch: "revert-12-fix/11-notes" }), clean);
  assert.ok(lint({ title: 'Revert "fix: x"', branch: "fix/11-notes" }).errors.length > 0);
});

test("body, files, forks and ordinary branch names have no rules", () => {
  for (const body of [null, "", "Requested by someone\n## Paused here\nTest changes: removed", "x".repeat(10000)]) {
    assert.deepEqual(lint({ title: "fix: keep entries", body, branch: "anything", fork: true, files: [{ filename: "tests/example.rs", patch: "-assert!(false);" }] }), clean);
  }
});
