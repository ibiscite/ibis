// Built app, invented IPC and an installed Playwright; no server or real libraries.
// pnpm -C apps/desktop build
// IBIS_PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs node apps/desktop/src/features/picker/library-setup.browser.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
const assertRem = (actual, expected, message) => {
  assert.ok(Math.abs(actual - expected) <= 0.01, `${message}: expected ${expected} rem, got ${actual}`);
};
const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE || "playwright");
const dist = fileURLToPath(new URL("../../../dist/", import.meta.url));
const browser = await chromium.launch({ headless: true });
let profiles = 0;
try {
  for (const [width, height] of [[480, 360], [720, 480], [840, 600]]) {
    for (const text of ["normal", "larger", "200%"]) for (const theme of ["light", "dark"]) {
      const label = `${width}x${height}/${text}/${theme}`;
      const page = await browser.newPage({ viewport: { width, height }, colorScheme: theme });
      page.setDefaultTimeout(5000);
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.route("**/*", async (route) => {
        const url = new URL(route.request().url());
        assert.equal(url.hostname, "ibis.fixture", "fixture never uses the network");
        const name = url.pathname === "/" ? "index.html" : url.pathname.slice(1);
        assert.ok(!name.includes(".."));
        const contentType = name.endsWith(".js") ? "text/javascript" : name.endsWith(".css") ? "text/css" : "text/html";
        await route.fulfill({ body: await readFile(path.join(dist, name)), contentType });
      });
      await page.addInitScript(({ text }) => {
        Object.defineProperty(navigator, "platform", { value: "MacIntel" });
        if (text === "200%") document.addEventListener("DOMContentLoaded", () => { document.documentElement.style.fontSize = "200%"; });
        const preferences = { citation_style: null, density: "comfortable", larger_text: text === "larger", show_paste_preview: true, search_icon: "ibis", paste: false, menu_bar: true };
        const none = { state: "none", revision: "1", has_snapshot: false, zotero: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, problem: null, note: null };
        let library = none, callback = 0, discoveries = 0, presentation = 0;
        const callbacks = new Map(), listeners = new Map();
        const emit = (event, payload) => {
          for (const [id, name] of listeners) if (event === name) callbacks.get(id)?.({ event, id, payload });
        };
        window.fixtureStatus = (patch) => { library = { ...none, ...patch }; emit("library:changed", library); };
        window.fixtureReopen = () => { emit("picker:hidden", null); emit("picker:shown", ++presentation); window.dispatchEvent(new Event("focus")); };
        window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: (_event, id) => listeners.delete(id) };
        window.__TAURI_INTERNALS__ = {
          metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
          transformCallback: (handler) => { const id = ++callback; callbacks.set(id, handler); return id; },
          unregisterCallback: (id) => callbacks.delete(id),
          invoke: async (command, args) => {
            if (command === "plugin:event|listen") { listeners.set(args.handler, args.event); return args.handler; }
            if (command === "plugin:event|unlisten" || command === "fit_picker" || command === "plugin:window|hide") return;
            if (command === "hide_picker") {
              if (args.presentation !== null && args.presentation !== presentation) return false;
              presentation += 1;
              emit("picker:hidden", null);
              return true;
            }
            if (command === "plugin:window|is_visible") return true;
            if (command === "picker_presentation") return presentation;
            if (command === "native_appearance") return "opaque";
            if (command === "picker_preferences") return preferences;
            if (command === "copy_format") return "pandoc";
            if (command === "picker_destination") return { app: null, app_id: null, app_identity: null, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null, paste: null, paste_question: false, can_choose_paste: false };
            if (command === "shortcut_status") return { shortcut: "CommandOrControl+Shift+Y", registered: true, error: null };
            if (command === "library_status") return library;
            if (command === "choose_library" || command === "use_zotero") return null;
            if (command === "find_sources") { discoveries += 1; return { zotero: "absent", list: discoveries, files: [{ name: "example.bib", folder: "Documents" }], cut_short: false }; }
            if (command === "use_sample_library") { window.fixtureStatus({ state: "loaded", name: "Sample references", snapshot_name: "Sample references", has_snapshot: true, references: 3, sample: true }); return; }
            if (command === "search" || command === "recent_references") return { revision: "1", results: [{ revision: "1", id: { library: "invented.bib", key: "lantern" }, cite: "synthetic-lantern", source: "invented.bib", key: "lantern", title: "Invented lantern gardens reference", authors: "Aster", first_author: "Aster", author_count: 1, year: 2026, container: null, item_type: "article-journal", partial: false, matched: ["title"] }] };
            if (command === "preview_file_citation") return { ticket: "synthetic-ticket", content_hash: "synthetic-hash", text: "[@lantern]", html: null, warnings: [] };
            if (command === "login_item") return { name: "Ibis", enabled: false, problem: null };
            if (command === "update_checks") return null;
            if (command === "update_available") return false;
            throw Error(`Unexpected synthetic IPC: ${command}`);
          },
        };
      }, { text });
      await page.goto("http://ibis.fixture/");
      const zotero = page.getByRole("option", { name: /^Zotero/ });
      await zotero.waitFor();
      await page.evaluate(() => document.fonts.ready);
      await page.waitForFunction(() => document.activeElement?.getAttribute("data-key") === "zotero");
      assert.equal(await page.locator(".search-bar").isVisible(), false, `${label}: welcome fills window`);
      assert.equal(await page.getByRole("option").count(), 4);
      const welcomeWidth = await page.locator(".welcome-mark").evaluate((node) => node.getBoundingClientRect().width / parseFloat(getComputedStyle(document.documentElement).fontSize));
      assertRem(welcomeWidth, 3.5, `${label}: 3.5 rem welcome mark`);
      const scan = async () => page.evaluate(() => {
        const main = document.querySelector("main"), bounds = main.getBoundingClientRect(), issues = [];
        if (Math.abs(bounds.height - innerHeight) > 1) issues.push("frame does not fill window");
        if (document.documentElement.scrollHeight > innerHeight + 1 || main.scrollTop) issues.push("window scrolls");
        const owners = [...main.querySelectorAll("*")].filter((node) => /auto|scroll/.test(getComputedStyle(node).overflowY) && node.clientHeight && node.scrollHeight > node.clientHeight + 1);
        if (owners.length > 1) issues.push(`multiple scrollers: ${owners.map((node) => node.className).join(",")}`);
        if (main.scrollWidth > innerWidth + 1) issues.push("horizontal overflow");
        for (const node of main.querySelectorAll("footer button")) {
          if (!node.getClientRects().length) continue;
          const box = node.getBoundingClientRect();
          if (box.top < bounds.top || box.bottom > Math.min(innerHeight, bounds.bottom) + 1 || box.left < 0 || box.right > innerWidth + 1) issues.push("footer clipped");
        }
        for (const node of main.querySelectorAll(".choice-row .result-title, .choice-help, .choice-line")) {
          if (node.scrollWidth > node.clientWidth + 1 || node.scrollHeight > node.clientHeight + 1) issues.push("choice text clipped");
        }
        const group = main.querySelector(".choice-screen, .empty-opening"), middle = main.querySelector(".picker-content");
        if (group && group.getBoundingClientRect().height < middle.clientHeight - 16) {
          const box = group.getBoundingClientRect(), port = middle.getBoundingClientRect();
          if (Math.abs((box.top - port.top) - (port.bottom - box.bottom)) > 2) issues.push("group not centred");
        }
        return issues;
      });
      assert.deepEqual(await scan(), [], `${label}: first run`);
      for (const [key, id] of [["ArrowRight", "file"], ["ArrowDown", "samples"], ["ArrowLeft", "find"], ["ArrowUp", "zotero"]]) {
        await page.keyboard.press(key);
        assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("data-key")), id, `${label}: ${key}`);
      }
      await page.getByRole("option", { name: /^Find them/ }).press("Enter");
      const found = page.getByRole("option", { name: /^Use example.bib/ });
      await found.waitFor();
      await page.keyboard.press("ArrowRight");
      assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("data-key")), "samples", `${label}: samples reachable after discovery`);
      await page.keyboard.press("Escape");
      await page.evaluate(() => window.fixtureReopen());
      // The native show handler must not focus a hidden Search control.
      assert.equal(await page.evaluate(() => !!document.activeElement?.closest(".choice-list")), true, `${label}: reopen focus`);
      await page.getByRole("option", { name: /^Try sample references/ }).press("Enter");
      await page.getByText("Invented lantern gardens reference", { exact: true }).waitFor();
      const mark = page.locator(".search-mark");
      const artwork = await mark.evaluate((node) => {
        const side = node.viewBox.baseVal.width, eye = node.querySelector("mask circle");
        return { size: node.getBoundingClientRect().width / parseFloat(getComputedStyle(document.documentElement).fontSize), eye: Number(eye.getAttribute("r")) * 2 * 24 / side,
          stops: [...node.querySelectorAll("stop")].map((stop) => getComputedStyle(stop).stopColor), transform: getComputedStyle(node).transform };
      });
      assertRem(artwork.size, 1.5, `${label}: 24 px mark`);
      assert.ok(artwork.eye >= 2, `${label}: open eye`);
      assert.deepEqual(artwork.stops, theme === "light" ? ["rgb(207, 70, 40)", "rgb(181, 51, 26)"] : ["rgb(255, 127, 94)", "rgb(242, 85, 47)"], `${label}: brand gradient`);
      assert.equal(artwork.transform, "none");
      await page.evaluate(() => { document.documentElement.dir = "rtl"; });
      assert.equal(await mark.evaluate((node) => getComputedStyle(node).transform), "none", `${label}: RTL keeps logo`);
      await page.evaluate(() => { document.documentElement.dir = "ltr"; window.fixtureStatus({ state: "error", name: "missing.bib", problem: "missing", error: "Invented missing file" }); });
      await page.getByRole("option", { name: /^Find missing.bib/ }).waitFor();
      assert.deepEqual(await scan(), [], `${label}: missing-file recovery`);
      await page.getByRole("option", { name: /^Find missing.bib/ }).press("Enter");
      // Recovery includes the source action added with the library footer:
      // cancelling Find keeps Find, Use another library and Open source.
      assert.deepEqual(await page.getByRole("option").evaluateAll((rows) => rows.map((row) => row.getAttribute("data-key"))),
        ["find", "another", "source"], `${label}: cancellation keeps all recovery choices`);
      assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("data-key")), "find", `${label}: cancellation keeps Find focused`);
      await page.evaluate(() => window.fixtureStatus({ state: "loaded", name: "empty.bib", snapshot_name: "empty.bib", has_snapshot: true }));
      await page.getByRole("heading", { name: "No references in empty.bib" }).waitFor();
      assert.deepEqual(await scan(), [], `${label}: empty library`);
      await page.evaluate(() => window.fixtureStatus({ state: "loading", name: "slow.bib" }));
      await page.getByRole("progressbar", { name: "Opening slow.bib…" }).waitFor();
      assert.deepEqual(await scan(), [], `${label}: opening`);
      await page.emulateMedia({ reducedMotion: "reduce" });
      assert.equal(await page.locator(".opening-mark").evaluate((node) => getComputedStyle(node).animationName), "none");
      assert.equal(await page.locator(".opening-progress .update-track > div").evaluate((node) => getComputedStyle(node).animationName), "none");
      // The same outcome oracle must reject a deliberately clipped frame.
      await page.locator("main").evaluate((node) => { node.style.height = "1px"; });
      assert.ok((await scan()).length > 0, `${label}: detects clipping`);
      assert.deepEqual(errors, [], `${label}: page errors`);
      await page.close(); profiles += 1;
    }
  }
  console.log(`Library setup: ${profiles} browser profiles passed (minimum/Medium/Large, normal/Larger/200%, light/dark, recovery, opening, reopen, samples, brand and Reduce Motion).`);
} finally { await browser.close(); }
