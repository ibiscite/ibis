// Run with an existing Playwright via IBIS_PLAYWRIGHT_MODULE; no installs,
// listener, Tauri backend or real library. Output is kept in memory.
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { build } from "vite";
import react from "@vitejs/plugin-react";

const built = await build({
  configFile: false, root: fileURLToPath(new URL("../..", import.meta.url)), plugins: [react()], logLevel: "error",
  build: { write: false, minify: false, lib: { entry: fileURLToPath(new URL("./readable-overlays.fixture.tsx", import.meta.url)), name: "Readability", formats: ["iife"] } },
});
const output = (Array.isArray(built) ? built : [built]).flatMap((result) => result.output);
const script = output.filter((file) => file.type === "chunk").map((file) => file.code).join("\n");
const css = output.filter((file) => file.type === "asset" && file.fileName.endsWith(".css")).map((file) => file.source).join("\n");
assert.ok(script && css, "bundle includes the shared components and real CSS");
if (process.argv.includes("--build-only")) {
  console.log("Readability fixture bundled; browser scenarios not run.");
  process.exit(0);
}
const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE || "playwright");
const browser = await chromium.launch({ headless: true, ...(process.env.IBIS_CHROMIUM_PATH ? { executablePath: process.env.IBIS_CHROMIUM_PATH } : {}) });
let profiles = 0;
try {
  for (const colorScheme of ["light", "dark"]) for (const preference of ["normal", "contrast", "transparency"]) for (const scale of [1, 1.15, 2]) {
    const page = await browser.newPage({ viewport: { width: 480, height: 360 }, colorScheme, contrast: preference === "contrast" ? "more" : "no-preference", reducedMotion: "reduce" });
    page.setDefaultTimeout(5000);
    // Playwright's generic media option does not expose reduced transparency.
    const session = await page.context().newCDPSession(page);
    await session.send("Emulation.setEmulatedMedia", { features: [
      { name: "prefers-color-scheme", value: colorScheme }, { name: "prefers-contrast", value: preference === "contrast" ? "more" : "no-preference" },
      { name: "prefers-reduced-transparency", value: preference === "transparency" ? "reduce" : "no-preference" }, { name: "prefers-reduced-motion", value: "reduce" },
    ] });
    await page.setContent(`<html data-platform="macos" data-system="macos" data-native-appearance="liquid-glass" style="font-size:${scale * 100}%"><head><style>${css}</style><style>
      header { display:flex; flex-wrap:wrap; align-items:center; gap:8px; padding:8px; }
      .fixture-body { height:260px; overflow:auto; padding:8px; }
      .fixture-body ul { margin:0; padding:0; }
    </style></head><body><div id="fixture"></div></body></html>`);
    await page.addScriptTag({ content: script });
    await page.getByRole("button", { name: "Keys", exact: true }).waitFor();
    // Long data and descriptions grow vertically, inside their existing pads.
    assert.deepEqual(await page.locator('.ui-option, .ui-settings-row').evaluateAll((rows) => rows.flatMap((row) => {
      const box = row.getBoundingClientRect(), style = getComputedStyle(row), issues = [];
      for (const child of row.children) {
        const text = child.getBoundingClientRect();
        if (text.left < box.left + parseFloat(style.paddingLeft) - 1 || text.right > box.right - parseFloat(style.paddingRight) + 1 || text.top < box.top + parseFloat(style.paddingTop) - 1 || text.bottom > box.bottom - parseFloat(style.paddingBottom) + 1) issues.push("content crosses row padding");
      }
      if (row.scrollWidth > row.clientWidth + 1) issues.push("row widens");
      return issues;
    })), [], `${colorScheme}/${preference}/${scale} wrapped rows`);
    for (const name of ["More", "Reference", "Format", "Keys"]) {
      await page.getByRole("button", { name: name === "Format" ? /^Format/ : name, exact: name !== "Format" }).click();
      const overlay = page.locator(".ui-overlay");
      await overlay.waitFor();
      const frame = await overlay.evaluate((node) => {
        const box = node.getBoundingClientRect(), style = getComputedStyle(node);
        const colour = style.backgroundColor.match(/[\d.]+/g).map(Number);
        return { left: box.left, right: box.right, top: box.top, bottom: box.bottom, alpha: colour[3] ?? 1, blur: style.backdropFilter };
      });
      assert.ok(frame.left >= 7 && frame.right <= 473 && frame.top >= 7 && frame.bottom <= 353, `${name} stays inside minimum window`);
      assert.ok(Math.abs(frame.alpha - (preference === "normal" ? name === "Keys" ? .96 : .92 : 1)) < .001, `${name} uses its overlay role`);
      assert.equal(frame.blur, preference === "normal" ? "blur(30px) saturate(1.8)" : "none");
      assert.deepEqual(await overlay.locator(".menu-item").evaluateAll((rows) => rows.flatMap((row) => {
        const box = row.getBoundingClientRect(), style = getComputedStyle(row);
        return [...row.children].flatMap((child) => {
          const text = child.getBoundingClientRect();
          return text.left < box.left + parseFloat(style.paddingLeft) - 1 || text.right > box.right - parseFloat(style.paddingRight) + 1 || text.top < box.top + parseFloat(style.paddingTop) - 1 || text.bottom > box.bottom - parseFloat(style.paddingBottom) + 1 ? ["menu text crosses row padding"] : [];
        });
      })), [], `${name} keeps padding around wrapped labels and trailing shortcuts`);
      if (name === "Format") {
        const list = page.getByRole("menu", { name: "Format" });
        const scan = () => list.evaluate((node) => {
          const box = node.getBoundingClientRect();
          const rows = [...node.querySelectorAll('[role^="menuitem"]')].map((row) => row.getBoundingClientRect());
          return { top: node.hasAttribute("data-more-above"), bottom: node.hasAttribute("data-more-below"), whole: rows.some((row) => Math.abs(row.bottom - box.bottom) <= 1), mask: getComputedStyle(node).maskImage };
        });
        await list.evaluate((node) => { node.scrollTop = 0; node.dispatchEvent(new Event("scroll")); });
        let edges = await scan();
        assert.equal(edges.top, false, "first row at rest stays crisp");
        assert.equal(edges.whole, true, "short format menu ends on a whole row");
        await list.evaluate((node) => { node.scrollTop = (node.scrollHeight - node.clientHeight) / 2; node.dispatchEvent(new Event("scroll")); });
        edges = await scan();
        assert.ok(edges.top && edges.bottom, "middle has content beyond both edges");
        if (preference !== "normal") assert.equal(edges.mask, "none");
        await list.evaluate((node) => { node.scrollTop = node.scrollHeight; node.dispatchEvent(new Event("scroll")); });
        assert.equal((await scan()).bottom, false, "last row at true end stays crisp");
      }
      await page.keyboard.press("Escape");
      await overlay.waitFor({ state: "detached" });
    }
    // At the ordinary window/text size, two content-sized columns are centred.
    if (scale === 1) {
      await page.setViewportSize({ width: 720, height: 480 });
      await page.getByRole("button", { name: "Keys", exact: true }).click();
      const layout = await page.locator(".keys-dialog").evaluate((node) => {
        const box = node.getBoundingClientRect(), groups = [...node.querySelectorAll("tbody")].map((group) => group.getBoundingClientRect());
        return { columns: groups[0].left !== groups[1].left, centred: Math.abs((box.top + box.bottom) / 2 - innerHeight / 2) < 1, width: box.width };
      });
      assert.ok(layout.columns && layout.centred && layout.width <= 600, "keys sheet is two centred columns, at most 600px wide");
    }
    await page.close(); profiles++;
  }
  console.log(`${profiles} readable-overlay profiles passed.`);
} finally { await browser.close(); }
