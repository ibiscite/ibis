import assert from "node:assert/strict";
import { test } from "node:test";
import { countWords, lint } from "./pr-lint.mjs";

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

test("checks the branch name and its type, except for forks and bots", () => {
  assert.match(lint({ ...ok, branch: "claude/x" }).errors[0], /must be <type>\/<issue>-<short-name>/);
  assert.match(lint({ ...ok, branch: "wip/12-thing" }).errors[0], /must be/);
  assert.match(lint({ ...ok, branch: "feat/12-thing" }).errors[0], /differs from title type/);
  assert.deepEqual(lint({ ...ok, branch: "patch-1", fork: true }).errors, []);
  assert.deepEqual(lint({ title: "chore(main): release 0.2.0", body: "", branch: "release-please--branches--main" }), { errors: [], warnings: [] });
  assert.deepEqual(lint({ title: "build(deps): bump x", body: "", branch: "dependabot/cargo/x-1.2" }), { errors: [], warnings: [] });
  assert.deepEqual(lint({ title: 'Revert "fix: keep notes literal (#12)"', body: "Reverts ibiscite/ibis#12", branch: "revert-12-fix/11-notes" }), { errors: [], warnings: [] });
  assert.ok(lint({ title: 'Revert "fix: x"', body: "", branch: "fix/11-notes" }).errors.length > 0);
});

test("needs exactly one issue link and warns when it differs from the branch", () => {
  assert.match(lint({ ...ok, body: body().replace("Closes #12", "") }).errors[0], /found 0/);
  assert.match(lint({ ...ok, body: `${body()}\nCloses #13` }).errors[0], /found 2/);
  assert.match(lint({ ...ok, body: body().replace("Closes #12", "<!-- Closes #12 -->") }).errors[0], /found 0/);
  assert.deepEqual(lint({ ...ok, body: body().replace("Closes #12", "Part of #12") }).errors, []);
  assert.deepEqual(lint({ ...ok, body: body().replace("Closes #12", "Closes #9") }).warnings, ["body links #9 but the branch names #12"]);
});

test("counts prose words only and limits the body", () => {
  assert.equal(countWords("<!-- a b c -->\n## Heading words\n- [ ] box words\n![alt text](x.png) one two — 三"), 3);
  assert.equal(countWords(""), 0);
  assert.deepEqual(lint({ ...ok, body: body("w ".repeat(240)) }).warnings, [`body has ${countWords(body("w ".repeat(240)))} words; aim for at most 250`]);
  assert.match(lint({ ...ok, body: body("w ".repeat(300)) }).errors[0], /words; at most 250/);
  assert.equal(countWords(`<!-- unclosed ${"w ".repeat(500)}`), 0);
});

test("keeps evidence and session notes out of the body", () => {
  assert.match(lint({ ...ok, body: body("https://github.com/o/r/actions/runs/123") }).errors[0], /workflow run/);
  const a = "a".repeat(40);
  const b = "b".repeat(40);
  assert.deepEqual(lint({ ...ok, body: body(`Review of ${a}, again ${a}`) }).errors, []);
  assert.match(lint({ ...ok, body: body(`${a} ${b}`) }).errors[0], /2 full commit hashes/);
  assert.match(lint({ ...ok, body: `_Requested by **Someone**_\n${body()}` }).warnings[0], /session or pause notes/);
  assert.match(lint({ ...ok, body: body("## Paused here") }).warnings[0], /session or pause notes/);
});

test("needs the template sections", () => {
  const r = lint({ ...ok, body: "Closes #12\n\nShort." });
  assert.deepEqual(r.errors, ["What and why", "How it works", "Verification"].map((s) => `body is missing the "## ${s}" section from the template`));
  assert.equal(lint({ ...ok, body: null }).errors.length, 4);
});
