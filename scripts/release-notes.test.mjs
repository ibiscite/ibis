import assert from "node:assert/strict";
import { test } from "node:test";
import { changes, releaseNotes } from "./release-notes.mjs";

const generated = `:robot: I have created a release *beep* *boop*
---


## 0.1.1 (2026-10-27)


### Features

* choose Zotero group libraries in the picker ([#422](https://github.com/ibiscite/ibis/issues/422)) ([5e3239e](https://github.com/ibiscite/ibis/commit/5e3239e4fbd76d5cd4a8d83e3cfc0764ce435adf))
* add a synthetic picker states gallery ([#221](https://github.com/ibiscite/ibis/issues/221)) ([191db80](https://github.com/ibiscite/ibis/commit/191db804a45abd4113468006f96e6764dba6b46c)), closes [#220](https://github.com/ibiscite/ibis/issues/220)


### Bug Fixes

* **desktop:** explain libraries with no references ([1a2b3c4](https://github.com/ibiscite/ibis/commit/1a2b3c4d))

---
This PR was generated with [Release Please](https://github.com/googleapis/release-please).
`;

test("drops the greeting, hashes and long links and keeps the groups", () => {
  assert.equal(
    changes(generated),
    [
      "**Features**",
      "",
      "- Choose Zotero group libraries in the picker (#422)",
      "- Add a synthetic picker states gallery (#221)",
      "",
      "**Bug Fixes**",
      "",
      "- Desktop: explain libraries with no references",
    ].join("\n"),
  );
});

test("adds the title line, download table and verification for the tag", () => {
  const notes = releaseNotes("v0.1.1", generated);
  assert.match(notes, /^Ibis 0\.1\.1: /);
  assert.match(notes, /\[Ibis-Windows-setup\.exe\]\(https:\/\/github\.com\/ibiscite\/ibis\/releases\/download\/v0\.1\.1\/Ibis-Windows-setup\.exe\)/);
  assert.match(notes, /\*\*Preview build\. Not signed\.\*\*/);
  assert.match(notes, /\*\*Windows installation:\*\*/);
  assert.match(notes, /--source-ref refs\/tags\/v0\.1\.1 /);
  assert.doesNotMatch(notes, /beep|Release Please|[0-9a-f]{7}\]/);
});

test("leaves notes that already have a download table unchanged", () => {
  const once = releaseNotes("v0.1.1", generated);
  assert.equal(releaseNotes("v0.1.1", once), once);
});

test("macOS first launch uses Privacy & Security rather than the obsolete context-menu override", () => {
  const notes = releaseNotes("v0.2.0", generated);
  assert.match(notes, /On macOS,.*System Settings → Privacy & Security and choose Open Anyway\./);
  assert.doesNotMatch(notes, /right-click Ibis in Applications and choose Open/);
});

test("still cleans notes that only mention a download heading inside an item", () => {
  const notes = releaseNotes("v0.1.1", generated.replace("in the picker (", "in the picker, see ## Download ("));
  assert.match(notes, /^## Download$/m);
  assert.doesNotMatch(notes, /beep/);
});

test("says where to look when the change list is empty", () => {
  assert.match(releaseNotes("v0.2.0", ":robot: I have created a release *beep* *boop*\n---\n"), /See the commits since the previous release\./);
});
