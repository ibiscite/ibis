// Real built picker + invented IPC. Browser layout evidence, not native input/engine proof.
// Serve `pnpm build` with `pnpm exec vite preview`; reuse an installed Playwright.
import assert from "node:assert/strict";
import { isWholeNotice, noticeText, waitForLayout } from "./picker-layout-checks.mjs";
const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE || "playwright");
const base = new URL(process.argv[2] || "http://127.0.0.1:4173");
assert.ok(["127.0.0.1", "localhost"].includes(base.hostname), "Use a loopback fixture server");
const browser = await chromium.launch({ headless: true });
let profiles = 0;
try {
  const cases = [720, 360].flatMap((width) => [false, true].flatMap((larger) => ["light", "dark"].map((colorScheme) => ({ width, larger, colorScheme, scale: 1 }))));
  cases.push(...[false, true].map((larger) => ({ width: 720, larger, colorScheme: "light", scale: 1, startupNotice: true })));
  cases.push(...[false, true].map((larger) => ({ width: 720, larger, colorScheme: "light", scale: 1, largeNotice: true })));
  cases.push({ width: 360, larger: false, colorScheme: "light", scale: 3 });
  for (const { width, larger, colorScheme, scale, startupNotice = false, largeNotice = false } of cases) {
    const small = width === 360, limit = largeNotice ? 480 : small ? 320 : 800;
    const page = await browser.newPage({ viewport: { width, height: small ? 320 : 109 }, colorScheme });
    const caseLabel = `${width}/${larger ? "larger" : "normal"}/${colorScheme}/scale-${scale}/${startupNotice ? "startup-notice" : largeNotice ? "large-notice" : "default"}`;
    const waitForCheck = (check, predicate, arg) => waitForLayout(page, caseLabel, check, predicate, arg);
    const errors = [], fits = [];
    page.setDefaultTimeout(5000);
    page.on("pageerror", (error) => errors.push(error.message));
    await page.exposeFunction("fixtureFit", async (height) => {
      fits.push(height);
      await page.setViewportSize({ width, height: Math.min(limit, Math.max(page.viewportSize().height, height ?? 480)) });
    });
    await page.addInitScript(({ larger, scale, startupNotice, largeNotice }) => {
      if (scale !== 1) document.addEventListener("DOMContentLoaded", () => { document.documentElement.style.fontSize = `${scale * 100}%`; });
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
          if (command === "library_status") return { state: "loaded", revision: "1", has_snapshot: true, zotero: false, name: "invented.bib", snapshot_name: "invented.bib", references: 14, warnings: 0, error: null, problem: null, note: null, settings_notice: startupNotice ? "Synthetic startup settings notice." : null };
          if (command === "search" && args.query.text === "refuse") throw Error("synthetic search refusal");
          if (command === "search") return { revision: "1", results: args.query.text ? Array.from({ length: 14 }, (_, i) => ({ revision: "1", id: { library: "invented.bib", key: `synthetic${i}` }, cite: `synthetic-cite-${i}`, source: "invented.bib", key: `synthetic${i}`, title: `Invented reference ${i}`, authors: "Aster", first_author: "Aster", author_count: 1, year: 2026, container: null, item_type: "article-journal", partial: false, matched: ["title"] })) : [] };
          if (command === "preview_file_citation") return { ticket: "synthetic-ticket", content_hash: "synthetic-hash", text: "[@synthetic0]", html: null, warnings: [] };
          if (command === "copy_file_citation") return { text: "[@synthetic0]", warnings: largeNotice ? Array.from({ length: 16 }, (_, i) => `Invented copy warning ${i + 1}: check the reference metadata.`) : [] };
          if (command === "login_item") return { name: "Ibis", enabled: false, problem: null };
          if (command === "update_checks") return null;
          if (command === "update_available") return false;
          throw Error(`Unexpected synthetic IPC: ${command}`);
        },
      };
    }, { larger, scale, startupNotice, largeNotice });
    await page.goto(base.href);
    await page.getByText("Type to find a reference", { exact: true }).waitFor();
    await waitForCheck("text-size-applied", (larger) => document.documentElement.dataset.textSize === (larger ? "larger" : "normal"), larger);
    await page.evaluate(() => document.fonts.ready);
    if (scale === 3) {
      await page.mouse.move(width - 5, 20);
      await page.mouse.wheel(0, 2000);
      await waitForCheck("extreme-wheel-scroll", () => document.querySelector("main").scrollTop > 0);
      assert.equal(await page.getByRole("button", { name: /^More/ }).evaluate((node) => { const rect = node.getBoundingClientRect(); return rect.top >= 0 && rect.bottom <= innerHeight + 1; }), true, "real wheel input reaches extreme-text footer");
      for (const control of [page.getByLabel("Search references", { exact: true }), page.getByRole("button", { name: /^More/ })]) {
        await control.scrollIntoViewIfNeeded();
        assert.equal(await control.evaluate((node) => { const rect = node.getBoundingClientRect(); return rect.top >= 0 && rect.bottom <= innerHeight + 1; }), true, "extreme text can reveal each fixed-edge control");
      }
      const owners = await page.evaluate(() => [document.querySelector("main"), ...document.querySelectorAll("main *")].filter((node) => /auto|scroll/.test(getComputedStyle(node).overflowY) && node.clientHeight > 0 && node.scrollHeight > node.clientHeight + 1).length);
      assert.equal(owners, 1, "extreme text has one full-page scroll owner");
      const offset = await page.locator("main").evaluate((node) => node.scrollTop);
      const frame = page.viewportSize(), before = fits.length;
      await page.getByRole("button", { name: /^More/ }).click();
      await page.getByRole("menu").waitFor();
      await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      assert.equal(await page.locator("main").evaluate((node) => node.scrollTop), offset, "opening More preserves the scrolled footer during measurement");
      assert.deepEqual(page.viewportSize(), frame, "extreme-text More preserves frame");
      assert.equal(fits.length, before, "extreme-text More does not request a different fit");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
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
      const search = main.querySelector(".search-bar").getBoundingClientRect();
      if (search.top < 0 || search.bottom > innerHeight + 1) issues.push("search clipped");
      if (bounds.top !== 0 || bounds.bottom > innerHeight + 1) issues.push("main exceeds frame");
      return issues;
    });
    assert.deepEqual(await scan(), [], `${width}/${larger}/${colorScheme}: compact`);
    if (!small) assert.ok(page.viewportSize().height > 109, "clipped initial viewport must grow to content");
    const wholeNotice = async (check, text) => {
      // Both the locator and geometry oracle must select the exact same notice.
      const notice = page.getByRole("alert").and(page.getByText(text, { exact: true })).first();
      await waitForCheck(check, isWholeNotice, text);
      assert.equal(await notice.isVisible(), true, `${caseLabel}: ${check} visible`);
      assert.equal(await notice.evaluate((node) => /auto|scroll/.test(getComputedStyle(node.closest(".notices")).overflowY)), false, "notices use the middle scroll owner");
      assert.deepEqual(await scan(), [], "whole compact notice keeps search/footer and one scroller");
    };
    if (startupNotice) await wholeNotice("startup-notice-whole", noticeText.startup);
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
    if (!small) {
      await search.fill("refuse");
      await wholeNotice("failed-search-notice-whole", noticeText.failedSearch);
      assert.equal(await page.locator("main").evaluate((node) => node.hasAttribute("data-compact")), true, "failed search stays compact");
      await search.fill("");
      await page.getByText("Type to find a reference", { exact: true }).waitFor();
    }
    await search.fill("invented");
    await page.getByRole("option").last().waitFor();
    assert.deepEqual(await scan(), [], `${width}/${larger}/${colorScheme}: results`);
    if (largeNotice) {
      await page.getByRole("region", { name: "Preview" }).getByText("[@synthetic0]", { exact: true }).waitFor();
      await search.press("Enter");
      await page.getByRole("list", { name: "Copy warnings" }).waitFor();
      // Copy consumes the old selection; choose two references while its warnings remain.
      await search.press("Shift+Enter");
      await search.press("ArrowDown");
      await search.press("Shift+Enter");
      await waitForCheck("large-notice-two-selected", () => document.querySelectorAll('.shelf-chips > .chip').length === 2);
      assert.deepEqual(page.viewportSize(), { width: 720, height: 480 }, "large-notice fixture uses the reported frame");
      assert.ok(await page.locator(".notices").evaluate((node) => node.getBoundingClientRect().height >= 300), "warnings consume at least 300px");
      assert.ok(await page.locator(".picker-content").evaluate((node) => node.clientHeight > 16 * parseFloat(getComputedStyle(document.documentElement).fontSize)), "middle exceeds the old 16rem fallback");
      await search.evaluate((node) => { node.focus({ preventScroll: true }); node.setSelectionRange(2, 5); });
      const selection = () => search.evaluate((node) => ({ focused: document.activeElement === node, range: [node.selectionStart, node.selectionEnd],
        references: [...document.querySelectorAll('.shelf-chips > .chip')].map((chip) => chip.title) }));
      const chosen = await selection();
      const owners = () => page.evaluate(() => [document.querySelector("main"), ...document.querySelectorAll("main *")]
        .filter((node) => /auto|scroll/.test(getComputedStyle(node).overflowY) && node.clientHeight > 0 && node.scrollHeight > node.clientHeight + 1)
        .map((node) => node.className));
      assert.deepEqual(await scan(), [], `720x480/${larger}: large notice with selection`);
      assert.deepEqual(await owners(), ["picker-content scroll-area"], "large notices and results share the middle scroller");
      for (const height of [88, 480, 800, 480]) {
        await page.setViewportSize({ width, height });
        await waitForCheck("large-notice-page-scroll", (extreme) => document.querySelector("main").hasAttribute("data-scroll-page") === extreme, height === 88);
        await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert.deepEqual(await selection(), chosen, "layout changes preserve focus, query range and selected references");
        if (height !== 88) {
          assert.deepEqual(await scan(), [], "growing the viewport restores the bounded middle");
          assert.deepEqual(await owners(), ["picker-content scroll-area"], "notice fallback survives viewport growth");
        }
      }
      for (const control of [page.getByRole("list", { name: "Copy warnings" }).locator("li").first(), page.getByRole("list", { name: "Copy warnings" }).locator("li").last(),
        page.getByLabel("Page", { exact: true }), page.getByRole("radiogroup", { name: "Citation form" }), page.getByRole("region", { name: "Preview" })]) {
        await control.scrollIntoViewIfNeeded();
        assert.equal(await control.evaluate((node) => {
          const box = node.getBoundingClientRect(), port = node.closest(".picker-content").getBoundingClientRect();
          return box.top >= port.top - 1 && box.bottom <= port.bottom + 1 && box.left >= port.left - 1 && box.right <= port.right + 1;
        }), true, "both ends of the notice and Page/Form/preview remain reachable");
        assert.deepEqual(await scan(), [], "revealing notice and shelf keeps search/footer whole and one scroller");
      }
      assert.deepEqual(await selection(), chosen, "scrolling the static layout preserves focus and selection");
      await page.getByRole("button", { name: "Dismiss copy warnings" }).click();
      await page.getByRole("list", { name: "Copy warnings" }).waitFor({ state: "detached" });
      assert.deepEqual(await owners(), ["results-area scroll-area"], "empty notices restore the results scroller");
      assert.deepEqual((await selection()).references, chosen.references, "dismissing warnings keeps selected references");
      assert.deepEqual(await scan(), [], "dismissed notice leaves one scroller");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    if (small) {
      await page.getByRole("region", { name: "Preview" }).getByText("[@synthetic0]", { exact: true }).waitFor();
      await waitForCheck("results-announced", () => document.querySelector('[aria-label="Search results"]').textContent.trim().length > 0);
      for (const height of [88, 120]) {
        await page.setViewportSize({ width, height });
        await waitForCheck("small-viewport-page-scroll", () => document.querySelector("main").hasAttribute("data-scroll-page"));
        await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        await page.setViewportSize({ width, height: 320 });
        await waitForCheck("restored-viewport-middle-scroll", () => !document.querySelector("main").hasAttribute("data-scroll-page"));
        assert.deepEqual(await scan(), [], `${height}px to 320px recovers bounded middle and whole footer without rerender`);
      }
    }
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
    await page.getByRole("alert").and(page.getByText(noticeText.failedSearch, { exact: true })).first().waitFor();
    assert.deepEqual(await scan(), [], `${width}/${larger}/${colorScheme}: failed search retains footer and one scroller`);
    // Consumer oracle must reject a genuinely clipped frame, not only read CSS.
    await page.evaluate(() => { document.querySelector("main").style.height = "1px"; });
    assert.ok((await scan()).includes("footer action clipped"), "same geometry oracle detects clipped footer");
    assert.deepEqual(errors, [], "no page errors");
    await page.close(); profiles += 1;
  }
  console.log(`Picker layout: ${profiles} browser profiles passed (normal/small, normal/Larger, light/dark, compact/large notices + extreme-text More).`);
} finally { await browser.close(); }
