// Every relative link in the repository's Markdown reaches a file, and every
// #anchor into Markdown reaches a heading there, so splitting a document can
// never leave a reader at a dead end.
import { test } from "node:test";
import assert from "node:assert/strict";
import { brokenLinks, documents } from "./doc-links.mjs";

test("every relative link in the repository's Markdown reaches its file and heading", () => {
  assert.deepEqual(brokenLinks(documents()), []);
});

test("a moved file, a renamed heading and a link outside the repository are each reported", () => {
  const files = {
    "docs/a.md": "# A\n\n## Same\n\n## Same\n\nSee [b](b.md#two-words), [gone](gone.md), [top](#a), [second](#same-1), [old](#old) and [up](../../x.md).\n\n```\n[not a link](nowhere.md)\n```\n",
    "docs/b.md": "# B\n\n## Two **words**!\n",
  };
  assert.deepEqual(brokenLinks(["docs/a.md"], (path) => files[path], (path) => path in files), [
    "docs/a.md: gone.md (no such file)",
    "docs/a.md: #old (no such heading)",
    "docs/a.md: ../../x.md (no such file)",
  ]);
});
