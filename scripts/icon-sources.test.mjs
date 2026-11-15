import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import { test } from "node:test";

test("vendored icon names and upstream glyphs match SOURCES.json", async () => {
  const directory = new URL("../brand/icons/", import.meta.url);
  const { glyphs, sha256 } = JSON.parse(await readFile(new URL("SOURCES.json", directory), "utf8"));
  assert.deepEqual((await readdir(directory)).sort(),
    ["LICENSE-phosphor", "SOURCES.json", ...Object.keys(glyphs).map(name => `${name}.svg`)].sort());
  assert.deepEqual(Object.keys(sha256).sort(), [...new Set(Object.values(glyphs))].sort());
  for (const [name, upstream] of Object.entries(glyphs)) {
    const source = await readFile(new URL(`${name}.svg`, directory));
    assert.equal(createHash("sha256").update(source).digest("hex"), sha256[upstream],
      `${name}.svg must be the unchanged Phosphor Regular ${upstream} glyph`);
  }
});
