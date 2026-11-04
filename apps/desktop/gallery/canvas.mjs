// Renders every canvas scenario from the real bundle, in light and dark, for the design canvas.
// IBIS_PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs pnpm -C apps/desktop canvas
import { mkdir, readFile } from "node:fs/promises";
import path from "node:path";
const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE);
const dist = process.argv[2], out = process.argv[3];
await mkdir(out, { recursive: true });
const browser = await chromium.launch({ headless: true });
const size = (scenario) => scenario === "notice" ? { width: 560, height: 100 }
  : boards(scenario) ? { width: 920, height: 560 } : { width: 720, height: 480 };
// Boards and Settings pages are captured whole: the window grows until nothing
// scrolls, so no picture is cut off by a fixed height. The picker keeps its one
// steady size [C10], as people see it.
const boards = (scenario) => scenario.startsWith("settings-") || ["states", "tokens", "parts"].includes(scenario);
const overflow = (page) => page.evaluate(() => Math.max(document.documentElement.scrollHeight - innerHeight,
  ...[...document.querySelectorAll("*")].filter((node) => /auto|scroll/.test(getComputedStyle(node).overflowY))
    .map((node) => node.scrollHeight - node.clientHeight)));
const load = async (viewport) => {
  const page = await browser.newPage({ viewport, deviceScaleFactor: 2 });
  // macOS is the reference design (docs/design.md), so pictures show Mac keys
  // and metrics whichever system captures them.
  await page.addInitScript(() => Object.defineProperty(navigator, "platform", { value: "MacIntel" }));
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
// Contrast modes, for the screens whose rules change there: forced colours and
// Increase Contrast, each in light and dark, as <scenario>-<theme>-forced.png and -contrast.png.
const contrastScenarios = new Set(["settings-general", "settings-shortcuts", "settings-look", "shelf", "reference-details", "file-drag"]);
const modes = [{ suffix: "", media: {} }, { suffix: "-forced", media: { forcedColors: "active" } }, { suffix: "-contrast", media: { contrast: "more" } }];
const failures = [];
let pictures = 0;
for (const scenario of scenarios) for (const mode of modes) for (const theme of ["light", "dark"]) {
  if (mode.suffix && !contrastScenarios.has(scenario)) continue;
  const page = await load(size(scenario));
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.emulateMedia({ colorScheme: theme, forcedColors: "none", contrast: "no-preference", ...mode.media });
  await page.evaluate(({ scenario, theme }) => { window.IbisCanvas.mount(document.getElementById("host"), { scenario, theme }); }, { scenario, theme });
  await page.waitForTimeout(700);
  await page.evaluate(() => document.fonts.ready);
  for (let round = 0; boards(scenario) && round < 4; round++) {
    const more = await overflow(page);
    if (more <= 0) break;
    const viewport = page.viewportSize();
    await page.setViewportSize({ width: viewport.width, height: viewport.height + Math.ceil(more) });
    await page.waitForTimeout(200);
  }
  await page.screenshot({ path: `${out}/${scenario}-${theme}${mode.suffix}.png` });
  // The layout rules (canvas/geometry.ts) hold in every mode and theme.
  for (const broken of await page.evaluate(() => window.IbisCanvas.geometry())) errors.push(`layout ${broken}`);
  pictures++;
  if (errors.length) failures.push(`${scenario}/${theme}${mode.suffix}: ${errors[0]}`);
  await page.close();
}
await browser.close();
if (failures.length) throw new Error(failures.join("\n"));
console.log(`Canvas: ${pictures} pictures.`);
