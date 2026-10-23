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
// It also checks the layout of states with and without chosen references
// (#746): no region of the picker overlaps another, no chip is cut by the
// tray's edge at rest, and the results list keeps its own scroll area.
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { PLATFORM_NAMES, Unavailable, VIEWPORT, openGallery } from "./gallery-contact-sheet.mjs";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const SPEC_FILE = path.join(repo, "docs", "design-geometry.json");
// The gallery state that shows each density with rows on screen.
const DENSITY_STATES = { compact: "compact", comfortable: "many-results", spacious: "spacious" };
const LARGER_TEXT = 1.15;
// States whose regions must stay separate: chosen references, a preview, none.
const LAYOUT_STATES = ["tray-preview", "tray-20", "tray", "compact", "many-results"];

/** The per-platform values docs/design.md derives from its tokens. */
export function derive(spec, platform, scale = 1) {
  const look = spec.platforms[platform];
  const rRow = Math.max(look.rMin, look.rWin - look.inset * scale);
  return {
    searchBand: look.searchBand * scale,
    textX: (look.inset + spec.rowPad + spec.tile + spec.rowGap) * scale,
    rWin: look.rWin,
    rRow,
    rTile: Math.max(look.rMin, rRow - spec.rowPad * scale),
  };
}

/** One row per check: what the spec says, what the frame measured, and whether they agree. */
export function compare(spec, platform, text, density, measured) {
  const scale = text === "large" ? LARGER_TEXT : 1;
  const expected = derive(spec, platform, scale);
  const rows = [];
  const check = (name, want, got) => rows.push({ name, platform, text, density, expected: want, measured: got,
    pass: Number.isFinite(got) && Math.abs(got - want) <= spec.tolerance });
  // Larger text scales rem sizes and insets; window corners and radius floors stay fixed.
  check(`row height (${density})`, spec.rowHeight[density] * scale, measured.rowHeight);
  if (density !== "comfortable") return rows;
  const look = spec.platforms[platform];
  check("footer height", spec.footer * scale + look.footerDivider, measured.footer);
  check("search band height", expected.searchBand, measured.searchBand);
  check("header divider", spec.headerDivider, measured.headerDivider);
  for (const name of ["fieldHeight", "pinHeight"]) check(name, look.headerControl * scale, measured[name]);
  check("popup height", look.popup * scale, measured.popupHeight);
  check("footer action height", spec.footerAction * scale, measured.footerAction);
  check("footer divider", look.footerDivider, measured.footerDivider);
  for (const name of ["pinTopGap", "pinBottomGap", "pinEndGap", "footerTopGap", "footerBottomGap"]) check(name, look.inset * scale, measured[name]);
  check("search glyph centre x = tile centre x", measured.tileCentre, measured.glyphCentre);
  check("typed text x = row title x", measured.titleX, measured.textX);
  check("typed text x", expected.textX, measured.textX);
  check("window radius", expected.rWin, measured.rWin);
  check("row radius", expected.rRow, measured.rRow);
  check("search field radius", expected.rRow, measured.rField);
  check("tile radius", expected.rTile, measured.rTile);
  return rows;
}

const overlaps = (a, b) => Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 0.5
  && Math.min(a.right, b.right) - Math.max(a.left, b.left) > 0.5;
/** Boxes cut by the edge of the area they scroll in: partly visible, neither whole nor hidden. */
const cut = (clip, boxes) => boxes.filter((box) => {
  const visible = Math.min(box.bottom, clip.bottom) - Math.max(box.top, clip.top);
  return visible > 0.5 && visible < box.bottom - box.top - 0.5;
}).length;

/** One row per layout rule (#746): no picker region overlaps another, no chip
 * is cut by the tray's edge, and the results list scrolls in its own area. */
export function layout(platform, text, state, { regions, tray, list }) {
  const rows = [];
  const check = (name, pass, got) => rows.push({ name: `${name} (${state})`, platform, text, density: state, expected: 0, measured: got, pass });
  const pairs = regions.flatMap((box, i) => regions.slice(i + 1).filter((other) => overlaps(box, other)).map((other) => `${box.name}/${other.name}`));
  check(pairs.length ? `no regions overlap: ${pairs.join(", ")}` : "no regions overlap", pairs.length === 0, pairs.length);
  if (tray) check("no chip cut by the tray edge", cut(tray.clip, tray.chips) === 0, cut(tray.clip, tray.chips));
  check("the list scrolls in its own area", Boolean(list?.ownScroll), list?.ownScroll ? 0 : 1);
  const area = regions.find(box => box.name === "results-area");
  if (area) {
    const inset = 8 * (text === "large" ? LARGER_TEXT : 1);
    const before = regions.filter(box => box !== area && box.bottom <= area.top).map(box => box.bottom);
    const after = regions.filter(box => box !== area && box.top >= area.bottom).map(box => box.top);
    for (const [edge, gap] of [["top", area.top - Math.max(...before)], ["bottom", Math.min(...after) - area.bottom]]) {
      rows.push({ name: `list ${edge} gap (${state})`, platform, text, density: state, expected: inset, measured: gap, pass: Number.isFinite(gap) && Math.abs(gap - inset) <= .5 });
    }
  }
  return rows;
}

/** Runs in the page: every visible region of the picker, the tray's chips and the list's overflow. */
function measureLayout() {
  const box = (element) => {
    const { top, bottom, left, right } = element.getBoundingClientRect();
    return { top, bottom, left, right };
  };
  const name = (element) => element.getAttribute("aria-label") ?? (element.className || element.localName);
  // The search band, footer and each band between them (the picker content is display: contents without a selection).
  const regions = [...document.querySelectorAll("main > *, main > .picker-content > *")]
    .filter((element) => !element.classList.contains("picker-content") && !element.classList.contains("visually-hidden"))
    .map((element) => ({ name: name(element), ...box(element) }))
    .filter((region) => region.bottom - region.top > 0.5 && region.right - region.left > 0.5);
  const tray = document.querySelector('[aria-label="Selected references"]');
  const edges = tray && getComputedStyle(tray);
  const list = document.querySelector(".results-area");
  return {
    regions,
    // What the tray shows: its padding box, inside the borders.
    tray: tray ? { clip: { top: box(tray).top + parseFloat(edges.borderTopWidth), bottom: box(tray).bottom - parseFloat(edges.borderBottomWidth) },
      chips: [...tray.children].map(box) } : null,
    list: list ? { ownScroll: /auto|scroll/.test(getComputedStyle(list).overflowY) } : null,
  };
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
  const header = document.querySelector(".search-bar"), footer = document.querySelector("footer");
  const pin = box(".search-bar .pin-button"), action = box("footer button[aria-haspopup]");
  const headerBox = header.getBoundingClientRect(), footerBox = footer.getBoundingClientRect();
  const headerDivider = px(header, "borderBottomWidth"), footerDivider = px(footer, "borderTopWidth");
  // The chain's name once #576 lands, the fixed token before it.
  const rWin = parseFloat(root.getPropertyValue("--r-win") || root.getPropertyValue("--window-radius"));
  return {
    rowHeight: row?.getBoundingClientRect().height ?? NaN,
    footer: box("footer")?.height ?? NaN,
    searchBand: headerBox.height - headerDivider,
    headerDivider, footerDivider, fieldHeight: inputBox?.height ?? NaN, pinHeight: pin?.height ?? NaN,
    popupHeight: box(".toolbar-menu-button")?.height ?? NaN, footerAction: action?.height ?? NaN,
    pinTopGap: pin ? pin.top - headerBox.top : NaN,
    pinBottomGap: pin ? headerBox.bottom - headerDivider - pin.bottom : NaN,
    pinEndGap: pin ? (document.documentElement.dir === "rtl" ? pin.left - headerBox.left : headerBox.right - pin.right) : NaN,
    footerTopGap: action ? action.top - footerBox.top - footerDivider : NaN,
    footerBottomGap: action ? footerBox.bottom - action.bottom : NaN,
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
    const jobs = Object.keys(spec.platforms).flatMap((platform) => ["normal", "large"].flatMap((text) => [
      ...Object.entries(DENSITY_STATES).map(([density, state]) => ({ platform, text, density, state })),
      ...LAYOUT_STATES.map((state) => ({ platform, text, state, layout: true }))]));
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
          rows.push(...(job.layout ? layout(job.platform, job.text, job.state, await page.evaluate(measureLayout))
            : compare(spec, job.platform, job.text, job.density, await page.evaluate(measure))));
          if (job.layout) {
            await page.evaluate(() => { const list = document.querySelector(".results-area"); list.scrollTop = list.scrollHeight / 2; });
            rows.push(...layout(job.platform, job.text, job.state, await page.evaluate(measureLayout)).map(row => ({ ...row, name: `${row.name}; scrolled` })));
          }
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
  const order = [...Object.keys(DENSITY_STATES), ...LAYOUT_STATES];
  const rank = (row) => Object.keys(spec.platforms).indexOf(row.platform) * 100 + (row.text === "large" ? 10 : 0)
    + order.indexOf(row.density);
  rows.sort((a, b) => rank(a) - rank(b));
  console.log(table(rows));
  const mismatches = rows.filter((row) => !row.pass).length;
  console.log(`\n${rows.length - mismatches} of ${rows.length} geometry and layout checks pass` +
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
