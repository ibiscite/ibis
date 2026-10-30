// Real built picker + invented IPC. Browser layout evidence, not native input/engine proof.
// Serve `pnpm build` with `pnpm exec vite preview`; reuse an installed Playwright.
import assert from "node:assert/strict";
const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE || "playwright");
const base = new URL(process.argv[2] || "http://127.0.0.1:4173");
assert.ok(["127.0.0.1", "localhost"].includes(base.hostname), "Use a loopback fixture server");
const browser = await chromium.launch({ headless: true });
let profiles = 0;
try {
  for (const width of [720, 360]) for (const larger of [false, true]) for (const colorScheme of ["light", "dark"]) {
    const small = width === 360, limit = small ? 320 : 800;
    const page = await browser.newPage({ viewport: { width, height: small ? 320 : 109 }, colorScheme });
    const errors = [], fits = [];
    page.setDefaultTimeout(5000);
    page.on("pageerror", (error) => errors.push(error.message));
    await page.exposeFunction("fixtureFit", async (height) => {
      fits.push(height);
      await page.setViewportSize({ width, height: Math.min(limit, Math.max(page.viewportSize().height, height ?? 480)) });
    });
    await page.addInitScript(({ larger }) => {
      Object.defineProperty(navigator, "platform", { value: "MacIntel" });
      let callback = 0;
      const preferences = { citation_style: null, density: "comfortable", larger_text: larger, show_paste_preview: true, search_mark: true, paste: false };
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
        transformCallback: () => ++callback, unregisterCallback: () => {},
        invoke: async (command, args) => {
          if (command === "fit_picker") return window.fixtureFit(args.height);
          if (command === "plugin:event|listen") return ++callback;
          if (command === "plugin:event|unlisten") return;
          if (command === "plugin:window|is_visible") return true;
          if (command === "native_appearance") return "opaque";
          if (command === "picker_preferences") return preferences;
          if (command === "set_picker_preferences") return { preferences: Object.assign(preferences, args.preferences), copy_format: "pandoc" };
          if (command === "copy_format") return "pandoc";
          if (command === "picker_destination") return { app: null, app_id: null, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null };
          if (command === "shortcut_status") return { shortcut: "CommandOrControl+Shift+Y", registered: true, error: null };
          if (command === "library_status") return { state: "loaded", revision: "1", has_snapshot: true, zotero: false, name: "invented.bib", snapshot_name: "invented.bib", references: 14, warnings: 0, error: null, problem: null, note: null };
          if (command === "search" && args.query.text === "refuse") throw Error("synthetic search refusal");
          if (command === "search") return { revision: "1", results: args.query.text ? Array.from({ length: 14 }, (_, i) => ({ revision: "1", id: { library: "invented.bib", key: `synthetic${i}` }, cite: `synthetic-cite-${i}`, source: "invented.bib", key: `synthetic${i}`, title: `Invented reference ${i}`, authors: "Aster", first_author: "Aster", author_count: 1, year: 2026, container: null, item_type: "article-journal", partial: false, matched: ["title"] })) : [] };
          if (command === "preview_file_citation") return { ticket: "synthetic-ticket", content_hash: "synthetic-hash", text: "[@synthetic0]", html: null, warnings: [] };
          if (command === "login_item") return { name: "Ibis", enabled: false, problem: null };
          if (command === "update_checks") return null;
          if (command === "update_available") return false;
          throw Error(`Unexpected synthetic IPC: ${command}`);
        },
      };
    }, { larger });
    await page.goto(base.href);
    await page.getByText("Type to find a reference", { exact: true }).waitFor();
    await page.waitForFunction((larger) => document.documentElement.dataset.textSize === (larger ? "larger" : "normal"), larger);
    await page.evaluate(() => document.fonts.ready);
    const scan = () => page.evaluate(() => {
      const main = document.querySelector("main"), bounds = main.getBoundingClientRect();
      const issues = [];
      if (document.documentElement.scrollHeight > innerHeight + 1 || document.documentElement.scrollTop || document.body.scrollTop || main.scrollTop) issues.push("window scrolls");
      const scrollers = [...main.querySelectorAll("*")].filter((node) => /auto|scroll/.test(getComputedStyle(node).overflowY) && node.clientHeight > 0 && node.scrollHeight > node.clientHeight + 1);
      if (scrollers.length > 1) issues.push(`multiple scrollbars: ${scrollers.map((node) => `${node.className}:${node.clientHeight}/${node.scrollHeight}`).join(", ")}`);
      for (const button of main.querySelectorAll("footer button")) {
        if (!button.getClientRects().length) continue;
        const rect = button.getBoundingClientRect();
        if (rect.bottom > Math.min(innerHeight, bounds.bottom) + 1 || rect.top < Math.max(0, bounds.top) || rect.left < 0 || rect.right > innerWidth + 1) issues.push("footer action clipped");
      }
      if (bounds.top !== 0 || bounds.bottom > innerHeight + 1) issues.push("main exceeds frame");
      return issues;
    });
    assert.deepEqual(await scan(), [], `${width}/${larger}/${colorScheme}: compact`);
    if (!small) assert.ok(page.viewportSize().height > 109, "clipped initial viewport must grow to content");
    const frame = page.viewportSize(), before = fits.length;
    await page.getByRole("button", { name: /^More/ }).click();
    await page.getByRole("menu").waitFor();
    await page.keyboard.press("End");
    await page.getByRole("menuitem", { name: /^Quit/ }).waitFor({ state: "visible" });
    await page.keyboard.press("Home");
    await page.keyboard.press("Escape");
    assert.deepEqual(page.viewportSize(), frame, "More/Escape preserve frame");
    assert.equal(fits.length, before, "menus do not request a different fit");
    const search = page.getByLabel("Search references", { exact: true });
    await search.fill("invented");
    await page.getByRole("option").last().waitFor();
    assert.deepEqual(await scan(), [], `${width}/${larger}/${colorScheme}: results`);
    for (const control of [page.getByLabel("Page", { exact: true }), page.getByRole("radiogroup", { name: "Citation form" }), page.getByRole("region", { name: "Preview" })]) {
      await control.scrollIntoViewIfNeeded();
      const reachable = await control.evaluate((node) => {
        const box = node.getBoundingClientRect(), port = node.closest(".picker-content").getBoundingClientRect();
        return box.top >= port.top - 1 && box.bottom <= port.bottom + 1 && box.left >= port.left - 1 && box.right <= port.right + 1;
      });
      assert.equal(reachable, true, "Page/Form/preview reachable in the bounded middle region");
      assert.deepEqual(await scan(), [], "revealing shelf controls keeps search/footer fixed and one scroller");
    }
    await search.press("Shift+Enter");
    await page.getByRole("region", { name: "Preview" }).waitFor();
    assert.deepEqual(await scan(), [], `${width}/${larger}/${colorScheme}: shelf`);
    await search.fill("refuse");
    await page.getByRole("alert").filter({ hasText: "Search" }).first().waitFor();
    assert.deepEqual(await scan(), [], `${width}/${larger}/${colorScheme}: failed search retains footer and one scroller`);
    // Consumer oracle must reject a genuinely clipped frame, not only read CSS.
    await page.evaluate(() => { document.querySelector("main").style.height = "1px"; });
    assert.ok((await scan()).includes("footer action clipped"), "same geometry oracle detects clipped footer");
    assert.deepEqual(errors, [], "no page errors");
    await page.close(); profiles += 1;
  }
  console.log(`Picker layout: ${profiles} browser profiles passed (normal/small, normal/Larger, light/dark).`);
} finally { await browser.close(); }
