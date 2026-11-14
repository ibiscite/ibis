// @vitest-environment node
import { JSDOM, VirtualConsole } from "jsdom";
import { MessageChannel } from "node:worker_threads";
import { build } from "vite";
import { beforeAll, expect, test, vi } from "vitest";
import config from "../vite.canvas.config";

let script;
let stylesheet;
beforeAll(async () => {
  const built = await build({ ...config, configFile: false, logLevel: "silent", build: { ...config.build, write: false } });
  if (Array.isArray(built)) expect(built).toHaveLength(1);
  const result = Array.isArray(built) ? built[0] : built;
  if (!("output" in result)) throw new Error("Expected one canvas bundle");
  expect(result.output.map((file) => file.fileName).sort()).toEqual(["ibis-canvas.css", "ibis-canvas.js"]);
  const js = result.output.find((file) => file.type === "chunk");
  const css = result.output.find((file) => file.type === "asset");
  if (js?.type !== "chunk" || css?.type !== "asset") throw new Error("Missing canvas assets");
  script = js.code;
  stylesheet = String(css.source);
}, 30_000);

// jsdom cannot measure layout. Reuse an installed browser for this regression:
// IBIS_PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs pnpm exec vitest run canvas/canvas.test.mjs
test.skipIf(!process.env.IBIS_PLAYWRIGHT_MODULE)("only resettable settings reserve space and their controls never shift when reset appears", async () => {
  const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE);
  const browser = await chromium.launch({ headless: true });
  try {
    for (const width of [920, 800]) {
      const page = await browser.newPage({ viewport: { width, height: 900 } });
      await page.addInitScript(() => Object.defineProperty(navigator, "platform", { value: "MacIntel" }));
      await page.route("**/*", (route) => route.fulfill({ contentType: "text/html", body: '<!doctype html><meta charset="utf-8"><div id="host" style="height:100vh"></div>' }));
      await page.goto("http://canvas.invalid/");
      await page.addStyleTag({ content: stylesheet });
      await page.addScriptTag({ content: script });
      await page.evaluate(() => window.IbisCanvas.mount(document.getElementById("host"), { scenario: "settings-general" }));
      const login = page.locator('.ui-row').filter({ has: page.locator('#login') });
      // Measure the drawn switch, rather than its visually hidden input.
      const control = login.locator('.ui-switch');
      await control.waitFor({ state: "visible" });
      await page.waitForFunction(() => !document.querySelector('#login').closest('.ui-row').querySelector('.ui-switch').hasAttribute('data-disabled'));
      const original = await control.boundingBox();
      expect(original.width).toBeGreaterThan(0);
      expect(await login.locator('.ui-row-reset').count()).toBe(1);
      expect(await login.getByRole("button", { name: /^Use Default/ }).count()).toBe(0);
      await control.click();
      await login.getByRole("button", { name: /^Use Default/ }).waitFor();
      expect((await control.boundingBox()).x).toBe(original.x);
      await login.getByRole("button", { name: /^Use Default/ }).click();
      await page.waitForFunction(() => !document.querySelector('#login').closest('.ui-row').querySelector('[data-setting-reset]'));
      expect((await control.boundingBox()).x).toBe(original.x);
      await page.getByRole("button", { name: "Details…" }).click();
      const backup = page.getByRole("button", { name: "Back Up…" });
      expect(await backup.locator('..').locator('.ui-row-reset').count()).toBe(0);
      await page.evaluate(() => window.IbisCanvas.mount(document.getElementById("host"), { scenario: "settings-shortcuts" }));
      const recorder = page.getByRole("button", { name: /^Open Ibis:/ });
      await recorder.waitFor();
      const right = await recorder.evaluate((node) => node.getBoundingClientRect().right);
      const keys = await page.locator('#key-quit kbd').evaluate((node) => node.getBoundingClientRect().right);
      expect(right).toBe(keys);
      const box = await recorder.boundingBox();
      await recorder.click();
      // Both short bindings fit the button's minimum width.
      await recorder.press("Alt+Shift+Y");
      await page.getByRole("button", { name: "Use Default for Open Ibis" }).waitFor();
      const changed = await recorder.boundingBox();
      expect(changed.x).toBe(box.x);
      expect(changed.x + changed.width).toBe(box.x + box.width);
      await page.close();
    }
  } finally { await browser.close(); }
}, 30_000);

test("the built scenario list renders, switches and reopens without Tauri or network", async () => {
  const errors = [];
  const console = new VirtualConsole();
  console.on("jsdomError", (error) => errors.push(error));
  console.on("error", (...args) => errors.push(args));
  const dom = new JSDOM('<div data-ibis-canvas="first-run" data-theme="dark"></div>', {
    url: "file:///canvas-board.html?scenario=results&theme=light",
    runScripts: "outside-only", pretendToBeVisual: true, virtualConsole: console,
  });
  const { window } = dom;
  const ports = [];
  window.MessageChannel = class extends MessageChannel {
    constructor() { super(); ports.push(this.port1, this.port2); }
  };
  window.TextEncoder = TextEncoder;
  // JSDOM omits scrolling; production Settings scrolls its current page.
  window.HTMLElement.prototype.scrollTo = function ({ top = 0, left = 0 } = {}) {
    this.scrollTop = top; this.scrollLeft = left;
  };
  // Any accidental runtime transport is a failure, including on interaction.
  window.fetch = () => { throw new Error("Canvas attempted fetch"); };
  window.XMLHttpRequest = class { constructor() { throw new Error("Canvas attempted XHR"); } };
  window.WebSocket = class { constructor() { throw new Error("Canvas attempted WebSocket"); } };
  window.eval(script);
  const canvas = window.IbisCanvas;
  const host = window.document.querySelector("[data-ibis-canvas]");
  const shown = async (scenario, theme) => {
    await vi.waitFor(() => {
      expect(window.document.documentElement.dataset.appearance).toBe(theme);
      expect(window.document.querySelector("[src], [href]")).toBeNull();
      if (scenario.startsWith("settings-")) {
        expect(host.querySelector('[aria-current="page"]')?.textContent?.toLowerCase()).toContain({ look: "appearance" }[scenario.slice(9)] ?? scenario.slice(9));
        expect(host.querySelector(scenario === "settings-shortcuts" ? ".ui-shortcut-table" : ".ui-row")).not.toBeNull();
      } else if (scenario === "notice") {
        expect(host.querySelector('.copy-receipt[role="status"]')?.textContent).toContain("Copied");
        expect(host.querySelector("code")?.textContent).toBe("[Aster, 2026]");
        expect(window.document.documentElement.classList.contains("notice-window")).toBe(true);
        expect(host.querySelector("button")).toBeNull();
      } else if (scenario === "first-run") expect(host.textContent).toContain("Where do your references live?");
      else if (scenario === "states") {
        for (const state of ["hovered", "pressed", "focus-visible"]) expect(host.querySelector(`[data-${state}]`)).not.toBeNull();
      } else if (scenario === "empty" || scenario === "file-drag") {
        expect(host.textContent).toContain("No references in invented.bib");
        if (scenario === "file-drag") expect(host.querySelector(".picker-content[data-file-drag]")).not.toBeNull();
      }
      else if (scenario === "error") expect(host.textContent).toContain("Can’t open invented.bib");
      else if (scenario === "loading") expect(host.textContent).toContain("Opening invented.bib");
      else if (scenario.startsWith("unknown-app")) expect(host.textContent).toContain("could not tell which app");
      else if (scenario === "tokens") for (const token of ["--accent", "--space-8", "--r-large", "--r-small"]) expect(host.textContent).toContain(token);
      else if (scenario === "shortcuts") expect(window.document.querySelector('[role="dialog"]')).not.toBeNull();
      else if (scenario === "parts") for (const part of ["Button: primary", "Settings rows", "Marks and icons"]) expect(host.textContent).toContain(part);
      else if (scenario === "empty-search") expect(host.textContent).toContain("Type to find");
      else if (scenario === "no-matches") expect(host.textContent).toContain("No matching references");
      else if (scenario === "missing") expect(host.textContent).toContain("Can’t find invented.bib");
      else if (scenario === "zotero-closed") expect(host.textContent).toContain("Open Zotero to read your library");
      else if (scenario === "unreadable") expect(host.textContent).toContain("Can’t read invented.bib");
      else if (scenario === "long-titles") expect(host.textContent).toContain("An invented reference whose title");
      else if (scenario === "commands") {
        // ">" lists the picker's commands, not the library; the footer runs the highlighted one.
        expect(host.querySelector('[role="option"]')?.textContent).toContain("Change Format");
        expect(host.textContent).not.toContain("Invented reference 0");
        expect(host.querySelector("footer")?.textContent).toContain("Run Change Format");
      }
      else {
        expect(host.textContent).toContain("Invented reference 0");
        if (scenario === "more") expect(window.document.querySelector('.ui-palette [role="option"]')).not.toBeNull();
        if (scenario === "format-menu") expect(window.document.querySelector('#format-palette-pandoc, .ui-palette [aria-current="true"]')).not.toBeNull();
        if (["more", "format-menu"].includes(scenario)) {
          const dialog = window.document.querySelector('[role="dialog"]');
          expect(dialog?.getAttribute("aria-labelledby")).toBeTruthy();
          expect(dialog?.querySelector(".ui-view-back")?.hasAttribute("title")).toBe(false);
          expect(dialog?.querySelector("h2")).toBeNull();
        }
        if (["library-selector", "library-menu"].includes(scenario)) {
          expect(host.querySelector(".library-status-trigger")?.textContent).toContain("Zotero · 190 references");
          if (scenario === "library-menu") expect(window.document.querySelector("#library-menu")?.textContent).toContain("Field Notes");
        }
        if (scenario === "reference-details") expect(window.document.querySelector('.ui-popover [role="dialog"]')?.textContent).toContain("Page");
        if (scenario === "show-as-menu") expect([...window.document.querySelectorAll('[role="menuitemradio"]')].some((item) => item.textContent.includes("[-@synthetic0]"))).toBe(true);
        if (scenario === "reader") expect(host.textContent).toContain("Hide PDF");
      }
    });
  };
  try {
    await shown("first-run", "dark");
    expect(host.querySelectorAll('[role="option"]')).toHaveLength(4);
    host.querySelector('[role="option"][data-key="file"]')?.click();
    await vi.waitFor(() => expect(host.textContent).toContain("The file chooser did not open. Try again."));
    expect(canvas.scenarios).toEqual(expect.arrayContaining(["results", "first-run", "shelf", "missing", "states", "tokens", "settings-about"]));
    for (const theme of ["light", "dark"]) for (const scenario of canvas.scenarios) {
      const unmount = canvas.mount(host, { scenario, theme });
      await shown(scenario, theme);
      // Refuse an unknown scenario before disturbing the visible board.
      expect(() => canvas.mount(host, { scenario: "unknown" })).toThrow("Unknown canvas scenario");
      expect(host.childElementCount).toBeGreaterThan(0);
      unmount();
      expect(host.childElementCount).toBe(0);
    }
    host.dataset.ibisCanvas = "";
    delete host.dataset.theme;
    const fromUrl = canvas.mount(host);
    await shown("results", "light");
    fromUrl();
    // Retired cleanup and delayed menu setup cannot consume the reopened board.
    const retired = canvas.mount(host, { scenario: "more" });
    const close = canvas.mount(host, { scenario: "first-run", theme: "light" });
    retired();
    await shown("first-run", "light");
    expect(window.document.querySelector('[role="menu"]')).toBeNull();
    close();
    expect(errors).toEqual([]);
  } finally { window.close(); for (const port of ports) port.close(); }
}, 30_000);

test("canvas output has only relative resource URLs and no network transport", () => {
  // Framework namespace identifiers and diagnostic links are inert strings,
  // not resources. All other absolute/protocol-relative URLs are forbidden.
  const inert = new Set([
    "http://www.w3.org/2000/svg", "http://www.w3.org/1998/Math/MathML",
    "http://www.w3.org/1999/xlink", "http://www.w3.org/XML/1998/namespace",
    "https://react.dev/errors/",
    // The command line's words in the shared catalogue; the app never opens it.
    "https://github.com/ibiscite/ibis/issues",
  ]);
  const urls = (script + stylesheet).match(/(?:[a-z][a-z\d+.-]*:\/\/|["'\x60]\/\/)[^\s"'\x60<>\\)]+/gi) ?? [];
  expect(urls.filter((url) => !inert.has(url))).toEqual([]);
  expect(script.match(/\b(?:fetch|XMLHttpRequest|WebSocket|EventSource|sendBeacon)\s*\(/g) ?? []).toEqual([]);
  for (const match of stylesheet.matchAll(/(?:url\(|@import\s+)["']?([^\s)"';]+)/g)) {
    expect(match[1]).toMatch(/^(?:\.{1,2}\/|data:image\/svg\+xml,|%23)/);
  }
});
