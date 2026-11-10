// One contact sheet: every surface at Medium in light mode, side by side in one
// picture, so an edge that doesn't line up shows at a glance (docs/design.md,
// "One contact sheet per design pull request"). Invented data only.
// IBIS_PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs pnpm -C apps/desktop contact
import { mkdir, readFile } from "node:fs/promises";
import path from "node:path";
const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE);
const dist = process.argv[2], out = process.argv[3];
await mkdir(path.dirname(out), { recursive: true });
const surfaces = [
  ["results", "Picker"], ["format-menu", "Format"], ["more", "Actions (More)"],
  ["cited-since-opening", "Cited Since Opening"], ["writing-projects", "Writing Projects"], ["shortcuts", "Keyboard Shortcuts"],
  ["settings-general", "Settings › General"], ["settings-look", "Settings › Look"], ["settings-citations", "Settings › Citations"],
  ["settings-shortcuts", "Settings › Shortcuts"], ["settings-about", "About"],
  ["first-run", "First run"], ["empty", "Empty library"], ["missing", "Library missing"], ["zotero-closed", "Zotero closed"],
  ["unreadable", "Library unreadable"], ["loading", "Opening"], ["error", "Error"],
];
// Medium, the default picker window; Settings at its own default window.
const size = (scenario) => scenario.startsWith("settings-") ? { width: 920, height: 560 } : { width: 720, height: 480 };
const browser = await chromium.launch({ headless: true });
const shot = async (scenario) => {
  const page = await browser.newPage({ viewport: size(scenario), deviceScaleFactor: 1 });
  // macOS is the reference design, so pictures show Mac keys whichever system captures them.
  await page.addInitScript(() => Object.defineProperty(navigator, "platform", { value: "MacIntel" }));
  await page.route("**/*", async (route) => {
    const name = new URL(route.request().url()).pathname.slice(1);
    if (!name) return route.fulfill({ contentType: "text/html", body: '<!doctype html><meta charset="utf-8"><link rel="stylesheet" href="ibis-canvas.css"><div id="host" style="height:100vh"></div><script src="ibis-canvas.js"></script>' });
    await route.fulfill({ body: await readFile(path.join(dist, name)), contentType: name.endsWith(".js") ? "text/javascript" : "text/css" });
  });
  await page.goto("http://canvas.invalid/");
  await page.emulateMedia({ colorScheme: "light" });
  await page.evaluate((scenario) => { window.IbisCanvas.mount(document.getElementById("host"), { scenario, theme: "light" }); }, scenario);
  await page.waitForTimeout(700);
  await page.evaluate(() => document.fonts.ready);
  const picture = (await page.screenshot()).toString("base64");
  await page.close();
  return picture;
};
const tiles = [];
for (const [scenario, label] of surfaces) tiles.push(`<figure><img src="data:image/png;base64,${await shot(scenario)}" width="${size(scenario).width}" height="${size(scenario).height}"><figcaption>${label}</figcaption></figure>`);
const sheet = await browser.newPage({ viewport: { width: 3000, height: 800 }, deviceScaleFactor: 1 });
await sheet.setContent(`<!doctype html><style>
  body { margin: 0; padding: 24px; background: #8e8e93; font: 600 18px -apple-system, system-ui, sans-serif; color: #fff; }
  main { display: flex; flex-wrap: wrap; align-items: flex-start; gap: 24px; }
  figure { margin: 0; } img { display: block; border-radius: 10px; box-shadow: 0 1px 4px rgb(0 0 0 / 30%); }
  figcaption { margin-top: 8px; }
</style><main>${tiles.join("")}</main>`);
await sheet.screenshot({ path: out, fullPage: true });
await browser.close();
console.log(`Contact sheet: ${surfaces.length} surfaces in ${out}`);
