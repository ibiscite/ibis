import assert from "node:assert/strict";
import { test } from "node:test";
import { findings } from "./hygiene.mjs";

const now = Date.parse("2026-11-01T00:00:00Z");
const base = { now, branches: [], pulls: [], issues: [], labels: [] };
const pr = (number, head, state, closedAt, extra = {}) => ({
  number, head, headSha: `sha-${head}`, base: "main", state, closedAt, updatedAt: closedAt ?? "2026-10-31T00:00:00Z", ...extra,
});
const branch = (name, sha = `sha-${name}`) => ({ name, sha });

test("deletes only branches whose pull requests all finished over a day ago", () => {
  const r = findings({
    ...base,
    branches: ["main", "fix/1-done", "fix/2-open", "fix/3-just-merged", "test/4-no-pr", "dependabot/cargo/x", "release-please--branches--main"].map((n) => branch(n)),
    pulls: [
      pr(1, "fix/1-done", "closed", "2026-10-20T00:00:00Z"),
      pr(2, "fix/2-open", "open", null),
      pr(5, "fix/2-open", "closed", "2026-10-01T00:00:00Z"),
      pr(3, "fix/3-just-merged", "closed", "2026-10-31T12:00:00Z"),
      pr(6, "dependabot/cargo/x", "closed", "2026-10-01T00:00:00Z"),
    ],
  });
  assert.deepEqual(r.deleteBranches, ["fix/1-done"]);
});

test("keeps branches with commits after the pull request closed or with open pull requests based on them", () => {
  const r = findings({
    ...base,
    branches: [branch("fix/7-more-work", "newer-commit"), branch("feat/8-stack-base")],
    pulls: [
      pr(7, "fix/7-more-work", "closed", "2026-10-01T00:00:00Z"),
      pr(8, "feat/8-stack-base", "closed", "2026-10-01T00:00:00Z"),
      pr(9, "feat/9-stacked", "open", null, { base: "feat/8-stack-base" }),
    ],
  });
  assert.deepEqual(r.deleteBranches, []);
});

test("reports stale pull requests, untyped or unscheduled issues, prefixed titles and stray labels", () => {
  const r = findings({
    ...base,
    pulls: [
      pr(7, "x", "open", null, { updatedAt: "2026-10-01T00:00:00Z" }),
      pr(70, "release-please--branches--main", "open", null, { updatedAt: "2026-10-01T00:00:00Z" }),
    ],
    issues: [
      { number: 8, title: "Paste into the previous app", labels: ["type:feature"], milestone: "0.4 Insertion" },
      { number: 9, title: "fix: crash on empty file", labels: [], milestone: undefined },
      { number: 10, title: "Repository hygiene report", labels: [], milestone: undefined },
    ],
    labels: ["type:bug", "area:core", "tracking", "dependencies", "bug"],
  });
  assert.deepEqual(r.problems, [
    "#7: pull request with no activity for 14 days",
    "#9: no type: label",
    "#9: no milestone",
    "#9: title starts with a commit prefix",
    "label `bug` is outside the naming standard",
  ]);
});
