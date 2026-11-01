// Renders every canvas scenario from the real bundle, in light and dark, for the design canvas.
// pnpm -C apps/desktop build:canvas
// IBIS_PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs node apps/desktop/gallery/canvas.mjs apps/desktop/dist-canvas OUT_DIR
import { readFile } from "node:fs/promises";
import path from "node:path";
const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE);
const dist = process.argv[2], out = process.argv[3];
const browser = await chromium.launch({ headless: true });
const size = (scenario) => scenario === "settings-look" ? { width: 920, height: 1320 }
  : scenario === "tokens" ? { width: 920, height: 1000 }
  : scenario === "settings-shortcuts" ? { width: 920, height: 900 }
  : scenario.startsWith("settings-") || scenario === "states" ? { width: 920, height: 560 } : { width: 720, height: 480 };
const load = async (viewport) => {
  const page = await browser.newPage({ viewport, deviceScaleFactor: 2 });
  await page.route("**/*", async (route) => {
    const name = new URL(route.request().url()).pathname.slice(1);
    if (!name) return route.fulfill({ contentType: "text/html", body: '<!doctype html><meta charset="utf-8"><link rel="stylesheet" href="ibis-canvas.css"><div id="host" style="height:100vh"></div><script src="ibis-canvas.js"></script>' });
    await route.fulfill({ body: await readFile(path.join(dist, name)), contentType: name.endsWith(".js") ? "text/javascript" : "text/css" });
  });
  await page.goto("http://canvas.invalid/");
  return page;
};
const probe = await load(size("results"));
const scenarios = await probe.evaluate(() => [...window.IbisCanvas.scenarios]);
await probe.close();
const failures = [];
for (const scenario of scenarios) for (const theme of ["light", "dark"]) {
  const page = await load(size(scenario));
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.emulateMedia({ colorScheme: theme });
  await page.evaluate(({ scenario, theme }) => { window.IbisCanvas.mount(document.getElementById("host"), { scenario, theme }); }, { scenario, theme });
  await page.waitForTimeout(700);
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: `${out}/${scenario}-${theme}.png` });
  if (errors.length) failures.push(`${scenario}/${theme}: ${errors[0]}`);
  await page.close();
}
await browser.close();
if (failures.length) throw new Error(failures.join("\n"));
console.log(`Canvas: ${scenarios.length * 2} pictures.`);
