// Build the canvas first. Uses an existing Playwright, no server or real data.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE || "playwright");
const script = await readFile(new URL("../dist-canvas/ibis-canvas.js", import.meta.url), "utf8");
const css = await readFile(new URL("../dist-canvas/ibis-canvas.css", import.meta.url), "utf8");
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 920, height: 560 }, reducedMotion: "reduce" });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  async function open(scenario, theme = "light") {
    await page.evaluate(() => window.stopCanvas?.());
    await page.setContent(`<html data-platform="macos" data-system="macos"><head><style>${css}</style></head><body><div id="probe"></div></body></html>`);
    await page.addScriptTag({ content: script });
    await page.evaluate(({ scenario, theme }) => { window.stopCanvas = window.IbisCanvas.mount(document.getElementById("probe"), { scenario, theme }); }, { scenario, theme });
    await page.locator("main").waitFor();
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  }
  for (const theme of ["light", "dark"]) {
    await open("settings-about", theme);
    await page.getByRole("button", { name: "Version 0.2.1" }).waitFor();
    const scrolling = await page.locator(".settings-page").evaluate((node) => ({ height: node.clientHeight, content: node.scrollHeight }));
    assert.ok(scrolling.content <= scrolling.height, `About never scrolls at 920 × 560 (${theme}): ${JSON.stringify(scrolling)}`);
    assert.equal(await page.getByRole("button", { name: "Ibis", exact: true }).locator("svg").count(), 2, "About uses the light/dark app icon");
  }
  await open("stale");
  await page.locator(".source-action").waitFor();
  await page.locator(".footer-action").waitFor();
  const footerGeometry = await page.locator(".source-action, .footer-action").evaluateAll((buttons) => buttons.map((button) => {
    const style = getComputedStyle(button);
    return [button.getBoundingClientRect().height, style.borderRadius, style.paddingInlineStart, style.paddingInlineEnd];
  }));
  assert.deepEqual(footerGeometry[0], footerGeometry[1], "source and copy have identical footer geometry");
  await open("file-drag");
  await page.locator(".picker-content[data-file-drag]").waitFor();
  assert.equal(await page.locator(".picker-content").evaluate((node) => getComputedStyle(node).outlineStyle), "solid", "the accent ring covers the content area");
  await page.dispatchEvent(".picker-content", "dragleave");
  await page.locator(".picker-content:not([data-file-drag])").waitFor();
  await open("settings-look");
  const crops = await page.locator(".ui-picker-picture[data-header]").evaluateAll((pictures) => pictures.map((picture) => {
    const frame = picture.getBoundingClientRect(), host = picture.querySelector(".ui-picker-picture-content > div");
    const header = host.shadowRoot.querySelector(".search-bar"), query = host.shadowRoot.querySelector("#search");
    const bar = header.getBoundingClientRect(), field = query.getBoundingClientRect(), style = getComputedStyle(header);
    return { width: frame.width, height: frame.height, headerHeight: bar.height, leading: field.left - bar.left, inset: parseFloat(style.paddingLeft) * .54, icon: header.dataset.icon };
  }));
  assert.equal(crops.length, 3);
  assert.equal(new Set(crops.map((crop) => `${crop.width}/${crop.height}/${crop.headerHeight}`)).size, 1, "all icon pictures take an identical header crop");
  assert.ok(Math.abs(crops[2].leading - crops[2].inset) < 1, "None starts at the leading inset without reserved icon space");
  assert.ok(crops.every((crop) => crop.headerHeight <= crop.height + 1), "each crop contains the whole header");
  await open("settings-shortcuts");
  const table = page.getByRole("table", { name: "Keyboard Shortcuts" });
  assert.equal(await table.getByRole("columnheader").count(), 2);
  const rowHeights = await table.locator('tbody tr:has(th[scope="row"])').evaluateAll((rows) => rows.map((row) => row.getBoundingClientRect().height));
  assert.ok(rowHeights.every((height) => Math.abs(height - 32) < 1), `compact shortcut rows: ${rowHeights}`);
  await page.getByRole("searchbox", { name: "Filter shortcuts" }).fill("quit");
  assert.equal(await table.locator('th[scope="row"]').allTextContents().then((labels) => labels.join()), "Quit Ibis");
  await page.getByRole("searchbox", { name: "Filter shortcuts" }).fill("unmatched");
  await page.getByText("No matching actions", { exact: true }).waitFor();
  await page.setViewportSize({ width: 720, height: 560 });
  await open("shortcuts");
  const sheet = page.getByRole("dialog", { name: "Keyboard Shortcuts" });
  await sheet.waitFor();
  assert.equal(await sheet.evaluate((node) => document.activeElement === node), true, "opening focuses the sheet itself");
  const left = sheet.locator(".keys-groups > table").first();
  assert.deepEqual(await left.locator('th[scope="rowgroup"]').allTextContents(), ["While searching", "Your library", "From any app"]);
  assert.deepEqual(await sheet.locator(".keys-groups > table").last().locator('th[scope="rowgroup"]').allTextContents(), ["Anywhere in Ibis"]);
  const close = sheet.getByRole("button", { name: "Close", exact: true });
  assert.equal(await close.getAttribute("data-focus-visible"), null, "opening has no close-button ring");
  await page.keyboard.press("Tab");
  assert.equal(await close.evaluate((node) => document.activeElement === node && node.hasAttribute("data-focus-visible")), true, "Tab reveals the close button's ring");
  const closeShape = await close.evaluate((node) => { const style = getComputedStyle(node); return { radius: parseFloat(style.borderRadius), size: node.clientWidth }; });
  assert.ok(/^(6|7|8)px$/.test(await page.locator(".keys-heading .ui-button").evaluate((node) => getComputedStyle(node).borderTopLeftRadius)), "the close button is the square icon button");
  for (const frame of [{ width: 720, height: 560 }, { width: 480, height: 360 }]) {
    await page.setViewportSize(frame);
    const body = sheet.locator(".keys-scroll");
    await body.evaluate((node) => { node.scrollTop = node.scrollHeight; });
    const geometry = await sheet.evaluate((node) => { const box = node.getBoundingClientRect(), scroll = node.querySelector(".keys-scroll"), end = scroll.lastElementChild.getBoundingClientRect(); return { top: box.top, bottom: box.bottom, width: node.scrollWidth - node.clientWidth, end: end.bottom, port: scroll.getBoundingClientRect().bottom }; });
    assert.ok(geometry.top >= 0 && geometry.bottom <= frame.height && geometry.width <= 1 && geometry.end <= geometry.port + 1, `whole sheet and reachable end: ${JSON.stringify(geometry)}`);
  }
  await page.keyboard.press("Escape");
  await sheet.waitFor({ state: "detached" });
  assert.deepEqual(errors, []);
  console.log("Design batch: About fits 920 × 560 in both themes; equal header crops, None inset, 32 pt shortcut rows, filter, sheet focus/Tab and minimum-size reachability pass.");
} finally { await browser.close(); }
