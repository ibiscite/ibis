#!/usr/bin/env node
// Measures the picker in the evidence gallery against the geometry in
// docs/design-geometry.json (the numbers in docs/design.md): row height per
// density, footer, search band, search glyph over the tile column, typed text
// at the row title's x, and the corner chain, for each look at normal and
// Larger text.
//
//   node scripts/gallery-geometry.mjs [--enforce] [--no-build] [--dist DIR]
//
// It prints one table. Until the geometry work (#576) lands it only reports:
// exit 0 whatever it measures. With --enforce a mismatch exits 1. Exit 2 means
// Playwright or Chromium is unavailable. Browser render: Chromium's fonts and
// no native window, so the window corner is the CSS token, not the frame.
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { PLATFORM_NAMES, Unavailable, VIEWPORT, openGallery } from "./gallery-contact-sheet.mjs";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const SPEC_FILE = path.join(repo, "docs", "design-geometry.json");
// The gallery state that shows each density with rows on screen.
const DENSITY_STATES = { compact: "compact", comfortable: "many-results", spacious: "spacious" };
const LARGER_TEXT = 1.15;

/** The per-platform values docs/design.md derives from its tokens. */
export function derive(spec, platform) {
  const look = spec.platforms[platform];
  const rRow = Math.max(look.rMin, look.rWin - look.inset);
  return {
    searchBand: look.searchBand,
    textX: look.inset + spec.rowPad + spec.tile + spec.rowGap,
    rWin: look.rWin,
    rRow,
    rTile: Math.max(look.rMin, rRow - spec.rowPad),
  };
}

/** One row per check: what the spec says, what the frame measured, and whether they agree. */
export function compare(spec, platform, text, density, measured) {
  const expected = derive(spec, platform);
  const scale = text === "large" ? LARGER_TEXT : 1;
  const rows = [];
  const check = (name, want, got) => rows.push({ name, platform, text, density, expected: want, measured: got,
    pass: Number.isFinite(got) && Math.abs(got - want) <= spec.tolerance });
  // Sizes are in rem, so Larger text scales them; radii are px and do not scale.
  check(`row height (${density})`, spec.rowHeight[density] * scale, measured.rowHeight);
  if (density !== "comfortable") return rows;
  check("footer height", spec.footer * scale, measured.footer);
  check("search band height", expected.searchBand * scale, measured.searchBand);
  check("search glyph centre x = tile centre x", measured.tileCentre, measured.glyphCentre);
  check("typed text x = row title x", measured.titleX, measured.textX);
  check("typed text x", expected.textX * scale, measured.textX);
  check("window radius", expected.rWin, measured.rWin);
  check("row radius", expected.rRow, measured.rRow);
  check("search field radius", expected.rRow, measured.rField);
  check("tile radius", expected.rTile, measured.rTile);
  return rows;
}

/** Runs in the page: boxes and radii of the first row, the search band and the footer. */
function measure() {
  const box = (selector) => document.querySelector(selector)?.getBoundingClientRect();
  const px = (element, property) => element ? parseFloat(getComputedStyle(element)[property]) : NaN;
  const row = document.querySelector('#results [role="option"]');
  const tile = row?.querySelector(".result-tile")?.getBoundingClientRect();
  const glyph = box(".search-bar > svg");
  const input = document.querySelector("#search");
  const inputBox = input?.getBoundingClientRect();
  const root = getComputedStyle(document.documentElement);
  // The chain's name once #576 lands, the fixed token before it.
  const rWin = parseFloat(root.getPropertyValue("--r-win") || root.getPropertyValue("--window-radius"));
  return {
    rowHeight: row?.getBoundingClientRect().height ?? NaN,
    footer: box("footer")?.height ?? NaN,
    searchBand: box(".search-bar")?.height ?? NaN,
    glyphCentre: glyph ? glyph.left + glyph.width / 2 : NaN,
    tileCentre: tile ? tile.left + tile.width / 2 : NaN,
    textX: inputBox ? inputBox.left + px(input, "borderLeftWidth") + px(input, "paddingLeft") : NaN,
    titleX: row?.querySelector("strong")?.getBoundingClientRect().left ?? NaN,
    rWin,
    rRow: px(row, "borderTopLeftRadius"),
    rField: px(input, "borderTopLeftRadius"),
    rTile: px(row?.querySelector(".result-tile"), "borderTopLeftRadius"),
  };
}

const format = (value) => Number.isFinite(value) ? String(Math.round(value * 10) / 10) : "—";

export function table(rows) {
  const header = ["Check", "Look", "Text", "Spec", "Measured", ""];
  const lines = rows.map((row) => [row.name, row.platform, row.text, format(row.expected), format(row.measured), row.pass ? "ok" : "MISMATCH"]);
  const widths = header.map((_, i) => Math.max(header[i].length, ...lines.map((line) => line[i].length)));
  return [header, ...lines].map((line) => line.map((cell, i) => cell.padEnd(widths[i])).join("  ").trimEnd()).join("\n");
}

function parse(argv) {
  const options = { enforce: false, build: true, dist: path.join(repo, "apps", "desktop", "dist") };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--enforce") options.enforce = true;
    else if (argv[i] === "--no-build") options.build = false;
    else if (argv[i] === "--dist" && argv[i + 1] !== undefined) options.dist = path.resolve(argv[++i]);
    else throw new Error(`unknown option ${argv[i]}; see the comment at the top of this script`);
  }
  return options;
}

async function main() {
  const options = parse(process.argv.slice(2));
  const spec = JSON.parse(readFileSync(SPEC_FILE, "utf8"));
  const { browser, server } = await openGallery(options);
  const base = `http://127.0.0.1:${server.address().port}`;
  const rows = [];
  const failures = [];
  try {
    const context = await browser.newContext({ viewport: VIEWPORT, colorScheme: "light", reducedMotion: "reduce" });
    const jobs = Object.keys(spec.platforms).flatMap((platform) => ["normal", "large"].flatMap((text) =>
      Object.entries(DENSITY_STATES).map(([density, state]) => ({ platform, text, density, state }))));
    let next = 0;
    const worker = async () => {
      while (next < jobs.length) {
        const job = jobs[next++];
        const page = await context.newPage();
        try {
          await page.addInitScript((platform) => {
            Object.defineProperty(navigator, "platform", { configurable: true, get: () => platform });
          }, PLATFORM_NAMES[job.platform]);
          await page.mouse.move(-1, -1);
          const query = new URLSearchParams({ state: job.state, platform: job.platform, theme: "light", text: job.text });
          await page.goto(`${base}/gallery.html?${query}`);
          await page.waitForFunction((id) => document.documentElement.dataset.galleryRendered === id, job.state, { timeout: 60_000 });
          rows.push(...compare(spec, job.platform, job.text, job.density, await page.evaluate(measure)));
        } catch (error) {
          failures.push(`${job.platform}/${job.text}/${job.state}: ${error.message.split("\n")[0]}`);
        } finally {
          await page.close();
        }
      }
    };
    await Promise.all(Array.from({ length: 4 }, worker));
  } finally {
    server.close();
    await browser.close();
  }
  const rank = (row) => Object.keys(spec.platforms).indexOf(row.platform) * 100 + (row.text === "large" ? 10 : 0)
    + Object.keys(DENSITY_STATES).indexOf(row.density);
  rows.sort((a, b) => rank(a) - rank(b));
  console.log(table(rows));
  const mismatches = rows.filter((row) => !row.pass).length;
  console.log(`\n${rows.length - mismatches} of ${rows.length} geometry checks match docs/design-geometry.json` +
    (options.enforce ? "." : " (report only; --enforce fails on a mismatch)."));
  if (failures.length > 0) {
    console.error(`Not measured:\n  ${failures.join("\n  ")}`);
    process.exitCode = 1;
  } else if (options.enforce && mismatches > 0) process.exitCode = 1;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = error instanceof Unavailable ? 2 : 1;
  });
}
