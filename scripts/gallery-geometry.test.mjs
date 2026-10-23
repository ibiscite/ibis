import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { SPEC_FILE, compare, derive, layout, table } from "./gallery-geometry.mjs";

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
  assert.equal(windows, macos);
  assert.match(design, new RegExp(`text x .*${macos} px on every OS`));
  assert.equal(derive(spec, "linux").textX, macos);
});

test("the geometry spec's row heights, footer and search band are the ones docs/design.md states", () => {
  const prose = design.replace(/\s+/g, " ");
  for (const [density, height] of Object.entries(spec.rowHeight)) {
    assert.match(prose, new RegExp(`${density} ${height} px`, "i"));
  }
  assert.match(prose, new RegExp(`\\| Footer \\| ${spec.footer} px \\|`));
  for (const [look, name] of [["macos", "macOS"], ["windows", "Windows"], ["linux", "GNOME"]]) {
    assert.match(prose, new RegExp(`${spec.platforms[look].searchBand} px ${name}`));
  }
  assert.deepEqual(LOOKS.map(look => spec.platforms[look].inset), [8, 8, 8]);
  assert.match(design, /design-geometry\.json/, "docs/design.md names the spec file the check reads");
});

const onSpec = { rowHeight: 58, footer: 41, searchBand: 48, glyphCentre: 36, tileCentre: 36, textX: 64, titleX: 64, rWin: 8, rRow: 4, rField: 4, rTile: 4,
  headerDivider: 1, fieldHeight: 32, pinHeight: 32, popupHeight: 32, footerAction: 24, footerDivider: 1,
  pinTopGap: 8, pinBottomGap: 8, pinEndGap: 8, footerTopGap: 8, footerBottomGap: 8 };

test("a frame on spec passes every check, within the tolerance", () => {
  const rows = compare(spec, "windows", "normal", "comfortable", { ...onSpec, textX: 64.4 });
  assert.equal(rows.length, 21);
  assert.ok(rows.every((row) => row.pass), table(rows));
});

test("Larger text scales nested insets but keeps window corners and radius floors fixed", () => {
  const scaled = derive(spec, "macos", 1.15);
  assert.equal(scaled.rWin, 28);
  assert.ok(Math.abs(scaled.rRow - 18.8) < 1e-9);
  assert.equal(scaled.rTile, 6);
  assert.ok(Math.abs(scaled.textX - 73.6) < 1e-9);
});

test("a Larger text frame with scaled insets passes without hiding radius drift", () => {
  const fixture = { ...spec, platforms: { ...spec.platforms, windows: { ...spec.platforms.windows, inset: 4, rWin: 8, rMin: 2 } } };
  const measured = { ...onSpec, rowHeight: 66.7, footer: 47, searchBand: 55.2,
    glyphCentre: 46, tileCentre: 46, textX: 69, titleX: 69, rRow: 3.4, rField: 3.4, rTile: 2,
    fieldHeight: 36.8, pinHeight: 36.8, popupHeight: 36.8, footerAction: 27.6,
    pinTopGap: 4.6, pinBottomGap: 4.6, pinEndGap: 4.6, footerTopGap: 4.6, footerBottomGap: 4.6 };
  const rows = compare(fixture, "windows", "large", "comfortable", measured);
  assert.ok(rows.every((row) => row.pass), table(rows));
  const drifted = compare(fixture, "windows", "large", "comfortable", { ...measured, rRow: 4.1 });
  assert.deepEqual(drifted.filter((row) => !row.pass).map((row) => row.name), ["row radius"]);
});

test("drift, a missing element and Larger text scaling are reported", () => {
  const rows = compare(spec, "windows", "normal", "comfortable", { ...onSpec, footer: 52, rTile: Number.NaN });
  assert.deepEqual(rows.filter((row) => !row.pass).map((row) => row.name), ["footer height", "tile radius"]);
  assert.match(table(rows), /tile radius +windows +normal +4 +— +MISMATCH/);
  const large = compare(spec, "macos", "large", "compact", { rowHeight: 46 });
  assert.deepEqual(large.map((row) => [row.name, row.pass]), [["row height (compact)", true]]);
});

test("header/footer fills, outside dividers and each eight-pixel gap are measured", () => {
  for (const name of ["searchBand", "headerDivider", "fieldHeight", "pinHeight", "popupHeight", "footerAction", "footerDivider", "pinTopGap", "pinBottomGap", "pinEndGap", "footerTopGap", "footerBottomGap"]) {
    const failed = compare(spec, "windows", "normal", "comfortable", { ...onSpec, [name]: onSpec[name] - 1 }).filter(row => !row.pass);
    assert.equal(failed.length, 1, `${name} drift must be visible`);
  }
});

const region = (name, top, bottom) => ({ name, top, bottom, left: 0, right: 720 });
const chips = (top, count) => Array.from({ length: count }, (_, i) => ({ top: top + i * 34, bottom: top + i * 34 + 28 }));

test("the list's clipping viewport keeps eight pixels from adjacent bands", () => {
  for (const [text, scale] of [["normal", 1], ["large", 1.15]]) {
    const bands = [region("search-bar", 0, 53), region("results-area", 53 + 8 * scale, 400 - 8 * scale), region("shelf", 400, 440)];
    const frame = { regions: bands, tray: null, list: { ownScroll: true } };
    assert.ok(layout("windows", text, "many-results", frame).every(row => row.pass));
    for (const edge of ["top", "bottom"]) {
      const drifted = { ...frame, regions: bands.map(box => box.name === "results-area" ? { ...box, [edge]: edge === "top" ? 53 : 400 } : box) };
      assert.deepEqual(layout("windows", text, "many-results", drifted).filter(row => !row.pass).map(row => row.name),
        [`list ${edge} gap (many-results)`]);
    }
  }
});

test("separate regions and a tray of whole chips pass the layout checks", () => {
  const rows = layout("macos", "normal", "tray-preview", {
    regions: [region("search-bar", 0, 56), region("Selected references", 56, 131), region("Preview", 131, 227), region("results-area", 235, 432), region("footer", 440, 480)],
    tray: { clip: { top: 56, bottom: 130 }, chips: chips(62, 50) },
    list: { ownScroll: true },
  });
  assert.equal(rows.length, 5);
  assert.ok(rows.every((row) => row.pass), table(rows));
  // Side by side is not an overlap.
  const beside = layout("linux", "normal", "pinned", { regions: [{ ...region("a", 0, 40), right: 300 }, { ...region("b", 0, 40), left: 300 }], tray: null, list: { ownScroll: true } });
  assert.ok(beside.every((row) => row.pass), table(beside));
});

test("overlapping regions, a half chip and a list without its own scroll fail (#746)", () => {
  const rows = layout("windows", "normal", "tray-preview", {
    regions: [region("Selected references", 56, 152), region("Preview", 150, 246), region("results-area", 240, 440), region("footer", 448, 488)],
    tray: { clip: { top: 56, bottom: 151 }, chips: chips(62, 50) },
    list: { ownScroll: false },
  });
  assert.deepEqual(rows.filter((row) => !row.pass).map((row) => row.name), [
    "no regions overlap: Selected references/Preview, Preview/results-area (tray-preview)",
    "no chip cut by the tray edge (tray-preview)", "the list scrolls in its own area (tray-preview)", "list top gap (tray-preview)"]);
  assert.equal(rows[0].measured, 2);
  // Without a tray only regions and the list are checked, and a missing list fails.
  assert.deepEqual(layout("linux", "large", "many-results", { regions: [], tray: null, list: null }).map((row) => row.pass), [true, false]);
});
