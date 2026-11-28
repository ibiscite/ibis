import assert from "node:assert/strict";
import test from "node:test";
import { END, START, handoff, replace, section } from "./inflight.mjs";

test("a one-line handoff is read as written", () => {
  assert.equal(handoff("What\n\nHandoff: nothing left. Next: none. Blocker: none.\n\nRefs #1"), "nothing left. Next: none. Blocker: none.");
});

test("a handoff under a heading or in bold runs to the end of its paragraph", () => {
  const body = "## Handoff\nLeft: the shell.\nNext command: `bash scripts/land.sh`.\nBlocker: #9.\n\nOther text.";
  assert.equal(handoff(body), "Left: the shell. Next command: `bash scripts/land.sh`. Blocker: #9.");
  assert.equal(handoff("**Handoff**\n\nResume with a review.\n\nRefs #2014."), "Resume with a review.");
  assert.equal(handoff("No section here."), "");
});

test("a long handoff is cut at a word", () => {
  const text = handoff(`Handoff: ${"word ".repeat(150)}`);
  assert.ok(text.length <= 502 && text.endsWith(" …"), text);
});

const pull = (number, login, body, draft = false, repo = "ibiscite/ibis") => ({
  number, draft, body, title: `fix: change ${number}`, updated_at: "2026-11-28T09:00:00Z",
  user: { login }, head: { ref: `branch-${number}`, repo: { full_name: repo } },
});

test("the list holds only the maintainer's and the release app's own branches", () => {
  const text = section([
    pull(1, "diyanko", "Handoff: left nothing.", true),
    pull(2, "someone", "Handoff: run this."),
    pull(3, "diyanko", "Handoff: from a fork.", false, "fork/ibis"),
    pull(4, "ibis-release[bot]", ""),
  ]);
  assert.match(text, /#4 fix: change 4 \(ready, `branch-4`, updated 2026-11-28\)\. Release pull request\.\n- #1 .*\(draft, .*Left nothing\./);
  assert.doesNotMatch(text, /#2|#3/);
});

test("only the generated section of the tracking issue changes", () => {
  const body = `## Plan\nkeep\n\n## In flight\n${START}\nold\n${END}\n\n## Waiting\nkeep too`;
  assert.equal(replace(body, `${START}\nnew\n${END}`), `## Plan\nkeep\n\n## In flight\n${START}\nnew\n${END}\n\n## Waiting\nkeep too`);
  assert.throws(() => replace("no markers", "x"), /needs/);
});
