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
        expect(host.querySelector('[aria-current="page"]')?.textContent?.toLowerCase()).toContain(scenario.slice(9));
        expect(host.querySelector(".ui-settings-row")).not.toBeNull();
      } else if (scenario === "first-run") expect(host.textContent).toContain("Where do your references live?");
      else {
        expect(host.textContent).toContain("Invented reference 0");
        if (scenario === "more") expect(window.document.querySelector('[role="menu"][aria-label="More"]')).not.toBeNull();
        if (scenario === "format-menu") expect(window.document.querySelector('[role="menuitemradio"]')).not.toBeNull();
      }
    });
  };
  try {
    await shown("first-run", "dark");
    expect(host.querySelectorAll('[role="option"]')).toHaveLength(4);
    host.querySelector('[role="option"][data-key="file"]')?.click();
    await vi.waitFor(() => expect(host.textContent).toContain("The file chooser did not open. Try again."));
    expect(canvas.scenarios).toEqual(["results", "first-run", "more", "format-menu", "settings-general", "settings-look", "settings-citations", "settings-shortcuts", "settings-about"]);
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
  ]);
  const urls = (script + stylesheet).match(/(?:[a-z][a-z\d+.-]*:\/\/|["'\x60]\/\/)[^\s"'\x60<>\\)]+/gi) ?? [];
  expect(urls.filter((url) => !inert.has(url))).toEqual([]);
  expect(script.match(/\b(?:fetch|XMLHttpRequest|WebSocket|EventSource|sendBeacon)\s*\(/g) ?? []).toEqual([]);
  for (const match of stylesheet.matchAll(/(?:url\(|@import\s+)["']?([^\s)"';]+)/g)) {
    expect(match[1]).toMatch(/^(?:\.{1,2}\/|data:image\/svg\+xml,)/);
  }
});
