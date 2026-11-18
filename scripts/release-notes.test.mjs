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
  assert.match(notes, /\*\*Command line:\*\* \[ibis-macOS-Apple-silicon\.tar\.gz\]\(https:\/\/github\.com\/ibiscite\/ibis\/releases\/download\/v0\.1\.1\/ibis-macOS-Apple-silicon\.tar\.gz\)/);
  assert.match(notes, /\[ibis-Windows\.zip\]/);
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

test("0.2 upgrade notes explain the new default and preserve explicitly stored shortcuts", () => {
  for (const tag of ["v0.2.0", "v0.2.1"]) {
    const notes = releaseNotes(tag, generated);
    assert.match(notes, /default.*Command\+Shift\+Y/);
    assert.match(notes, /explicitly stored shortcut.*Control\+Command\+R/);
    assert.match(notes, /keep that old chord.*`"shortcut": "Control\+Command\+R"`/);
    assert.match(notes, /remove.*`shortcut`.*restart/);
  }
  for (const tag of ["v0.1.1", "v0.3.0"]) assert.doesNotMatch(releaseNotes(tag, generated), /macOS shortcut/);
});

test("0.3 notes say format choices from an earlier version start fresh", () => {
  // Release notes explain settings resets before 1.0.
  for (const tag of ["v0.3.0", "v0.3.1"]) {
    assert.match(releaseNotes(tag, generated), /\*\*Good to know:\*\* Format choices made in an earlier version start fresh; choose them again\./);
  }
  for (const tag of ["v0.2.1", "v0.4.0"]) assert.doesNotMatch(releaseNotes(tag, generated), /start fresh/);
});

test("lists each change once, even when two commits or PR references repeat it", () => {
  const repeated = `### Bug Fixes

* keep the picker open ([#12](https://github.com/ibiscite/ibis/issues/12)) ([1a2b3c4](https://github.com/ibiscite/ibis/commit/1a2b3c4d))
* keep the picker open ([#12](https://github.com/ibiscite/ibis/issues/12)) ([5e6f7a8](https://github.com/ibiscite/ibis/commit/5e6f7a8b))
* name the key ([9b0c1d2](https://github.com/ibiscite/ibis/commit/9b0c1d2e))
* name the key ([9b0c1d2](https://github.com/ibiscite/ibis/commit/9b0c1d2e))
`;
  assert.equal(changes(repeated), ["**Bug Fixes**", "", "- Keep the picker open (#12)", "- Name the key"].join("\n"));
});

test("adds the contract changes after the styles, and nothing when there are none", () => {
  const contract = "**Contract changes**\n\nBreaking:\n\n- `command writings removed`\n";
  const notes = releaseNotes("v0.3.11", generated, "ibiscite/ibis", "**Citation styles**\n\n- None\n", contract);
  assert.match(notes, /\*\*Citation styles\*\*\n\n- None\n\n\*\*Contract changes\*\*\n\nBreaking:\n\n- `command writings removed`\n\n<details>/);
  assert.doesNotMatch(releaseNotes("v0.3.11", generated, "ibiscite/ibis", "", " \n"), /Contract changes/);
});

test("an oversized contract report cannot push release notes over GitHub's limit", () => {
  const summary = "**Contract changes**\n\nCompared with ` v0.3.10 `. 1 added, 0 removed, 0 changed.";
  const contract = `${summary}\n\n**Added**\n\n- \`${"x".repeat(125_000)}\`\n`;
  const styles = "**Citation styles**\n\n- None\n";
  const notes = releaseNotes("v0.3.11", generated, undefined, styles, contract);
  assert.ok(notes.length <= 125_000);
  assert.ok(notes.includes(summary));
  assert.ok(notes.includes(styles.trim()));
  assert.match(notes, /scripts\/contract-compat\.mjs --base refs\/tags\/<baseline>/);
  assert.match(notes, /<summary>Verify a download<\/summary>/);

  const styleLength = 125_000 - releaseNotes("v0.3.11", generated).length - 10;
  const longStyles = "x".repeat(styleLength);
  const almostFull = releaseNotes("v0.3.11", generated, undefined, longStyles);
  assert.ok(almostFull.length <= 125_000);
  assert.equal(releaseNotes("v0.3.11", generated, undefined, longStyles, contract), almostFull);
});
