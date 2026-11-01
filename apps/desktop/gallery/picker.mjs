// Screenshots every picker state in light and dark from the built app, with invented data.
// pnpm -C apps/desktop build
// IBIS_PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs node apps/desktop/gallery/picker.mjs apps/desktop/dist OUT_DIR
import { readFile } from "node:fs/promises";
import path from "node:path";
const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE);
const dist = process.argv[2], out = process.argv[3];
const browser = await chromium.launch({ headless: true });
for (const theme of ["light", "dark"]) {
const width = 720, height = 480, text = "normal";
const page = await browser.newPage({ viewport: { width, height }, colorScheme: theme, deviceScaleFactor: 2 });
page.setDefaultTimeout(5000);
await page.route("**/*", async (route) => {
  const url = new URL(route.request().url());
  const name = url.pathname === "/" ? "index.html" : url.pathname.slice(1);
  const contentType = name.endsWith(".js") ? "text/javascript" : name.endsWith(".css") ? "text/css" : "text/html";
  await route.fulfill({ body: await readFile(path.join(dist, name)), contentType });
});
await page.addInitScript(() => {
  window.__frames = [];
  const tick = () => {
    const main = document.querySelector("main");
    if (main) {
      const shelf = document.querySelector(".shelf-citation");
      const f = [document.querySelectorAll('[role="option"]').length, (document.querySelector('[role="option"]')?.textContent ?? "").slice(0, 28), (shelf?.textContent ?? "").slice(0, 24), document.querySelector("main footer")?.textContent?.slice(0, 40) ?? "", [...(shelf?.children ?? [])].map((n) => n.className || n.tagName).join("+").slice(0, 40)];
      const key = JSON.stringify(f);
      if (key !== window.__last) { window.__last = key; window.__frames.push([window.__phase, Math.round(performance.now()), ...f]); }
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  window.__shifts = []; window.__mut = {};
  const label = (n) => { if (!n || !n.tagName) return "?"; return n.tagName.toLowerCase() + (n.className && typeof n.className === "string" ? "." + n.className.split(/\s+/).slice(0, 2).join(".") : ""); };
  new PerformanceObserver((list) => { for (const e of list.getEntries()) window.__shifts.push({ v: +e.value.toFixed(4), phase: window.__phase, who: (e.sources || []).map((s) => `${label(s.node)} ${Math.round(s.previousRect.top)},${Math.round(s.previousRect.height)}>${Math.round(s.currentRect.top)},${Math.round(s.currentRect.height)}`).slice(0, 4) }); }).observe({ type: "layout-shift", buffered: true });
});
      await page.addInitScript(({ text }) => {
        Object.defineProperty(navigator, "platform", { value: "MacIntel" });
        if (text === "200%") document.addEventListener("DOMContentLoaded", () => { document.documentElement.style.fontSize = "200%"; });
        const preferences = { citation_style: null, density: "comfortable", larger_text: text === "larger", show_paste_preview: true, search_mark: true, paste: false };
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
            const rows = (q) => Array.from({ length: q ? Math.max(2, 9 - q.length) : 8 }, (_, i) => ({ revision: "1", id: { library: "invented.bib", key: `k${i}${q}` }, cite: `synthetic-${i}`, source: "invented.bib", key: `k${i}${q}`, title: `Invented reference ${q} number ${i} about lantern gardens`, authors: "Aster et al.", first_author: "Aster", author_count: 3, year: 2020 + i, container: "Journal of Invented Things", item_type: "article-journal", partial: false, matched: ["title"] }));
            if (command === "search" || command === "recent_references") { await new Promise((resolve) => setTimeout(resolve, 40)); return { revision: "1", results: rows(args?.query?.text ?? "") }; }
            if (command === "preview_file_citation") { await new Promise((resolve) => setTimeout(resolve, 25)); return { ticket: "synthetic-ticket", content_hash: "synthetic-hash", text: "[@k0]", html: null, warnings: [] }; }
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
page.on("pageerror", (e) => console.log("PAGEERROR", e.message));
await page.getByRole("option", { name: /^Zotero/ }).waitFor();
await page.getByRole("option", { name: /^Try sample references/ }).press("Enter");
await page.waitForTimeout(500);


const shot = (name) => page.screenshot({ path: `${out}/${theme}-${name}.png` });
const search = page.getByRole("combobox", { name: "Search references" });
await search.waitFor(); await page.waitForTimeout(300);
await shot("01-empty");
await search.fill("qui"); await page.waitForTimeout(250); await shot("02-results");
await search.press("ArrowDown"); await search.press("Shift+Enter"); await search.press("ArrowDown"); await search.press("Shift+Enter"); await page.waitForTimeout(300); await shot("03-two-added");
await page.locator(".shelf-count").click().catch(() => {}); await page.waitForTimeout(250); await shot("04-selected-references");
await page.keyboard.press("Escape"); await page.waitForTimeout(150);
await page.locator(".shelf-segment-button").first().click().catch(() => {}); await page.waitForTimeout(250); await shot("05-reference-menu");
await page.keyboard.press("Escape"); await page.waitForTimeout(150);
await search.focus();
await page.getByRole("button", { name: /^More/ }).click(); await page.waitForTimeout(250); await shot("06-more");
await page.keyboard.press("Escape"); await page.waitForTimeout(150);
await page.locator(".toolbar-menu-control button, .toolbar-menu-control").first().click().catch(() => {}); await page.waitForTimeout(250); await shot("07-format");
await page.keyboard.press("Escape"); await page.waitForTimeout(150);
await page.getByRole("button", { name: "Library" }).click().catch(() => {}); await page.waitForTimeout(250); await shot("08-library");
await page.keyboard.press("Escape"); await page.waitForTimeout(150);
await search.focus(); await page.keyboard.press("Meta+/"); await page.waitForTimeout(300); await shot("09-keys");
await page.keyboard.press("Escape"); await page.waitForTimeout(100);
await page.close();
}
await browser.close();
