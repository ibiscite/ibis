// Runs the reader spike (#1252) in Chromium at deviceScaleFactor 1, light and
// dark, and records what it proves: the page renders, the text layer is in the
// accessibility tree and selectable, and no request leaves the bundle.
// VITE_READER_SPIKE=1 pnpm -C apps/desktop exec vite build --config vite.canvas.config.ts --outDir DIST
// IBIS_PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs node apps/desktop/canvas/reader-spike/shoot.mjs DIST OUT_DIR
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE);
const [dist, out] = process.argv.slice(2);
if (!dist || !out) throw new Error("Usage: shoot.mjs DIST OUT_DIR");
await mkdir(out, { recursive: true });
const browser = await chromium.launch({ headless: true });
const report = { browser: `chromium ${browser.version()}`, runs: [] };
for (const theme of ["light", "dark"]) {
  const page = await browser.newPage({ viewport: { width: 1200, height: 760 }, deviceScaleFactor: 1 });
  const requests = [], errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => { if (message.type() === "error") errors.push(message.text()); });
  await page.addInitScript(() => Object.defineProperty(navigator, "platform", { value: "MacIntel" }));
  await page.route("**/*", async (route) => {
    const url = route.request().url();
    requests.push(url);
    const name = new URL(url).pathname.slice(1);
    if (!name) return route.fulfill({ contentType: "text/html", body: '<!doctype html><meta charset="utf-8"><link rel="stylesheet" href="ibis-canvas.css"><div id="host" style="height:100vh"></div><script src="ibis-canvas.js"></script>' });
    if (name !== "ibis-canvas.js" && name !== "ibis-canvas.css") return route.abort();
    await route.fulfill({ body: await readFile(path.join(dist, name)), contentType: name.endsWith(".js") ? "text/javascript" : "text/css" });
  });
  await page.goto("http://canvas.invalid/");
  await page.emulateMedia({ colorScheme: theme });
  await page.evaluate((theme) => window.IbisCanvas.mount(document.getElementById("host"), { scenario: "reader-spike", theme }), theme);
  await page.waitForFunction(() => document.documentElement.dataset.readerSpike, null, { timeout: 15000 });
  const state = await page.evaluate(() => document.documentElement.dataset.readerSpike);
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: `${out}/reader-spike-${theme}.png` });
  const run = { theme, state, errors, requests: requests.map((url) => new URL(url).pathname) };
  if (theme === "light") {
    // The text layer, tinted, to show it sits on the drawn glyphs.
    await page.evaluate(() => document.documentElement.setAttribute("data-show-text-layer", ""));
    await page.screenshot({ path: `${out}/reader-spike-text-layer.png`, clip: { x: 0, y: 40, width: 460, height: 640 } });
    await page.evaluate(() => document.documentElement.removeAttribute("data-show-text-layer"));
    run.ariaSnapshot = await page.locator(".reader-spike-document").ariaSnapshot();
    // Select one sentence through the text layer, as a person dragging would.
    run.selected = await page.evaluate(() => {
      const span = [...document.querySelectorAll(".textLayer span")].find((node) => node.textContent?.startsWith("A quotation worth citing"));
      if (!span) return null;
      const range = document.createRange();
      range.selectNodeContents(span);
      getSelection()?.removeAllRanges();
      getSelection()?.addRange(range);
      return getSelection()?.toString() ?? null;
    });
    run.find = await page.getByText("imaginary mason").count();
    run.spans = await page.locator(".textLayer span").count();
    run.indicator = await page.locator(".reader-spike-indicator").textContent();
  }
  report.runs.push(run);
  await page.close();
}
await browser.close();
await writeFile(`${out}/reader-spike.json`, `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify(report, null, 2));
