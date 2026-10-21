import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { SPEC_FILE, compare, derive, table } from "./gallery-geometry.mjs";

const spec = JSON.parse(readFileSync(SPEC_FILE, "utf8"));
const design = readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "docs", "design.md"), "utf8");
const LOOKS = ["macos", "windows", "linux"];

/** The leading number of each platform cell in a docs/design.md table row. */
function designRow(token) {
  const line = design.split("\n").find((row) => row.startsWith(`| \`${token}\``));
  assert.ok(line, `docs/design.md has a ${token} row`);
  return line.split("|").slice(2, 5).map((cell) => Number(cell.trim().match(/^\d+/)?.[0]));
}

test("the geometry spec derives the corner chain and text x that docs/design.md states", () => {
  assert.deepEqual(LOOKS.map((look) => derive(spec, look).rWin), designRow("--r-win"));
  assert.deepEqual(LOOKS.map((look) => derive(spec, look).rRow), designRow("--r-row"));
  assert.deepEqual(LOOKS.map((look) => derive(spec, look).rTile), designRow("--r-tile"));
  const { macos, windows } = { macos: derive(spec, "macos").textX, windows: derive(spec, "windows").textX };
  assert.match(design, new RegExp(`text x .*${macos} px \\(${windows} px on Windows\\)`));
  assert.equal(derive(spec, "linux").textX, macos);
});

test("the geometry spec's row heights, footer and search band are the ones docs/design.md states", () => {
  const prose = design.replace(/\s+/g, " ");
  for (const [density, height] of Object.entries(spec.rowHeight)) {
    assert.match(prose, new RegExp(`${density} ${height} px`, "i"));
  }
  assert.match(prose, new RegExp(`\\| Footer \\| ${spec.footer} px \\|`));
  assert.match(prose, new RegExp(`search band is ${spec.platforms.macos.searchBand} px \\(${spec.platforms.windows.searchBand} px on Windows\\)`));
  assert.equal(spec.platforms.linux.searchBand, spec.platforms.macos.searchBand);
  assert.match(design, /design-geometry\.json/, "docs/design.md names the spec file the check reads");
});

const onSpec = { rowHeight: 58, footer: 40, searchBand: 48, glyphCentre: 40, tileCentre: 40, textX: 60, titleX: 60, rWin: 8, rRow: 4, rField: 4, rTile: 2 };

test("a frame on spec passes every check, within the tolerance", () => {
  const rows = compare(spec, "windows", "normal", "comfortable", { ...onSpec, textX: 60.4, titleX: 60 });
  assert.equal(rows.length, 10);
  assert.ok(rows.every((row) => row.pass), table(rows));
});

test("drift, a missing element and Larger text scaling are reported", () => {
  const rows = compare(spec, "windows", "normal", "comfortable", { ...onSpec, footer: 52, rTile: Number.NaN });
  assert.deepEqual(rows.filter((row) => !row.pass).map((row) => row.name), ["footer height", "tile radius"]);
  assert.match(table(rows), /tile radius +windows +normal +2 +— +MISMATCH/);
  const large = compare(spec, "macos", "large", "compact", { rowHeight: 46 });
  assert.deepEqual(large.map((row) => [row.name, row.pass]), [["row height (compact)", true]]);
});
