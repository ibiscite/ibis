// Real built picker + invented IPC. Browser layout evidence, not native input/engine proof.
// Serve `pnpm build` with `pnpm exec vite preview`; reuse an installed Playwright.
import assert from "node:assert/strict";
import { dismissMenu, isAnchoredDetails, isWholeNotice, isWholeShelf, noticeText, waitForLayout } from "./picker-layout-checks.mjs";
const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE || "playwright");
const base = new URL(process.argv[2] || "http://127.0.0.1:4173");
assert.ok(["127.0.0.1", "localhost"].includes(base.hostname), "Use a loopback fixture server");
const browser = await chromium.launch({ headless: true });
let profiles = 0;
try {
  const cases = [720, 480].flatMap((width) => [false, true].flatMap((larger) => ["light", "dark"].map((colorScheme) => ({ width, larger, colorScheme, scale: 1 }))));
  cases.push(...[720, 480].flatMap((width) => [false, true].map((larger) => ({ width, larger, colorScheme: "light", scale: 1, startupNotice: true }))));
  cases.push(...[false, true].map((larger) => ({ width: 720, larger, colorScheme: "light", scale: 1, largeNotice: true })));
  cases.push(...[false, true].flatMap((larger) => ["progress", "settings"].map((pointerNotice) => ({ width: 720, larger, colorScheme: "light", scale: 1, pointerNotice }))));
  cases.push(...[720, 480].flatMap((width) => [false, true].flatMap((larger) =>
    ["first-run", "empty-library", "loading", "recovery"].map((viewState) => ({ width, larger, colorScheme: "light", scale: 1, viewState })))));
  cases.push(...["loaded", "first-run", "recovery"].map((viewState) => ({ width: 480, larger: false, colorScheme: "light", scale: 2, viewState })));
  cases.push({ width: 480, larger: false, colorScheme: "light", scale: 2, shortcutNotice: true });
  cases.push(...[720, 480].flatMap((width) => [false, true].flatMap((larger) =>
    ["expanded", "failed", "warnings", "undo"].map((shelfDetails) => ({ width, larger, colorScheme: "light", scale: 1, shelfDetails })))));
  cases.push(...["expanded", "failed", "warnings", "undo"].map((shelfDetails) => ({ width: 480, larger: false, colorScheme: "light", scale: 2, shelfDetails })));
  for (const { width, larger, colorScheme, scale, shelfDetails = null, startupNotice = false, largeNotice = false, pointerNotice = null, viewState = "loaded", shortcutNotice = false } of cases) {
    const small = width === 480;
    const height = small ? 360 : largeNotice || pointerNotice ? 480 : Math.round(480 * (larger ? 1.15 : 1));
    const page = await browser.newPage({ viewport: { width, height }, colorScheme });
    const caseLabel = `${width}/${larger ? "larger" : "normal"}/${colorScheme}/scale-${scale}/${shelfDetails ? `shelf-${shelfDetails}` : shortcutNotice ? "shortcut-notice" : startupNotice ? "startup-notice" : largeNotice ? "large-notice" : pointerNotice ? `pointer-${pointerNotice}` : viewState}`;
    const waitForCheck = (check, predicate, arg) => waitForLayout(page, caseLabel, check, predicate, arg);
    const errors = [], resizes = [];
    // Exit must finish even if a modal temporarily hides the old menu from ARIA.
    const moreMenu = page.getByRole("menu", { name: "More", exact: true, includeHidden: true });
    const formatMenu = page.getByRole("menu", { name: /^Copy as/, includeHidden: true });
    const keysDialog = page.getByRole("dialog", { name: "Keyboard shortcuts", exact: true });
    page.setDefaultTimeout(5000);
    page.on("pageerror", (error) => errors.push(error.message));
    await page.exposeFunction("fixtureSizing", (command) => { resizes.push(command); });
    await page.addInitScript(({ larger, scale, startupNotice, largeNotice, pointerNotice, viewState, shortcutNotice, shelfDetails, formatFailure, entryWarning }) => {
      if (scale !== 1) document.addEventListener("DOMContentLoaded", () => { document.documentElement.style.fontSize = `${scale * 100}%`; });
      Object.defineProperty(navigator, "platform", { value: "MacIntel" });
      let callback = 0;
      const callbacks = new Map(), listeners = new Map();
      let undoTicket = null, refuseUndo = true;
      window.fixtureWindowReset = () => {
        undoTicket = 7;
        const event = "picker:window-reset";
        const ids = [...listeners].filter(([, name]) => name === event).map(([id]) => id);
        if (!ids.length) throw Error("Synthetic reset listener unavailable");
        for (const id of ids) callbacks.get(id)?.({ event, id, payload: { revision: 7, undo: 7 } });
      };
      const preferences = { citation_style: shelfDetails === "warnings" ? "ieee" : null, density: "comfortable", larger_text: larger, show_paste_preview: true, search_mark: true, paste: false };
      const library = { state: "loaded", revision: "1", has_snapshot: true, zotero: false, name: "invented.bib", snapshot_name: "invented.bib", references: 14, warnings: 0, error: null, problem: null, note: null, settings_notice: startupNotice ? "Synthetic startup settings notice." : null };
      if (viewState === "first-run") Object.assign(library, { state: "none", has_snapshot: false, name: null, snapshot_name: null, references: 0 });
      if (viewState === "empty-library") library.references = 0;
      if (viewState === "loading") Object.assign(library, { state: "loading", has_snapshot: false, references: 0 });
      if (viewState === "recovery") Object.assign(library, { state: "error", has_snapshot: false, references: 0, problem: "missing", error: "The invented library file is unavailable" });
      window.fixtureNotice = (shown) => {
        const event = pointerNotice === "progress" ? "update:progress" : "library:changed";
        const ids = [...listeners].filter(([, name]) => name === event).map(([id]) => id);
        if (!ids.length) throw Error("Synthetic notice listener unavailable");
        const payload = pointerNotice === "progress"
          ? { attempt: 1, revision: 1, received: 25, total: 100, phase: "downloading" }
          : { ...library, settings_notice: shown ? "Synthetic asynchronous settings notice." : null };
        for (const id of ids) callbacks.get(id)?.({ event, id, payload });
      };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: (_event, id) => listeners.delete(id) };
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
        transformCallback: (handler) => { const id = ++callback; callbacks.set(id, handler); return id; }, unregisterCallback: (id) => callbacks.delete(id),
        invoke: async (command, args) => {
          if (/fit_picker|set_size|set_position/.test(command)) {
            await window.fixtureSizing(command);
            throw Error("Content must never request native sizing");
          }
          if (command === "plugin:event|listen") { listeners.set(args.handler, args.event); return args.handler; }
          if (command === "plugin:event|unlisten") return;
          if (command === "plugin:window|is_visible") return true;
          if (command === "native_appearance") return "opaque";
          if (command === "picker_preferences") return preferences;
          if (command === "set_picker_preferences") return { preferences: Object.assign(preferences, args.preferences), copy_format: "pandoc" };
          if (command === "copy_format") return "pandoc";
          if (command === "picker_destination") return { app: null, app_id: null, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null };
          if (command === "shortcut_status") return { shortcut: "CommandOrControl+Shift+Y", registered: true, error: shortcutNotice ? "Synthetic shortcut refusal" : null };
          if (command === "library_status") return library;
          if (command === "search" && args.query.text === "refuse") throw Error("synthetic search refusal");
          if (command === "search") return { revision: "1", results: args.query.text && args.query.text !== "no-match" ? Array.from({ length: 14 }, (_, i) => ({ revision: "1", id: { library: "invented.bib", key: `synthetic${i}` }, cite: `synthetic-cite-${i}`, source: "invented.bib", key: `synthetic${i}`, title: shelfDetails === "failed" ? `Invented reference ${i}. ${"A synthetic long title. ".repeat(30)}`.trim() : `Invented reference ${i}`, authors: "Aster", first_author: "Aster", author_count: 1, year: 2026, container: null, item_type: "article-journal", partial: shelfDetails === "warnings" && i === 0, matched: ["title"] })) : [] };
          if (command === "preview_file_citation" && shelfDetails === "failed") throw { code: "failed", message: formatFailure };
          if (command === "preview_file_citation") return { ticket: "synthetic-ticket", content_hash: "synthetic-hash", text: shelfDetails === "warnings" ? "Aster, Invented reference, 2026." : "[@synthetic0]", html: null, warnings: shelfDetails === "warnings" ? [entryWarning] : [] };
          if (command === "copy_file_citation") return { text: "[@synthetic0]", warnings: largeNotice ? Array.from({ length: 16 }, (_, i) => `Invented copy warning ${i + 1}: check the reference metadata.`) : [] };
          if (command === "login_item") return { name: "Ibis", enabled: false, problem: null };
          if (command === "update_checks") return null;
          if (command === "update_available") return false;
          if (command === "set_pinned") return;
          if (command === "read_window_position_undo") return undoTicket;
          if (command === "dismiss_window_position_undo") { if (args.revision === undoTicket) undoTicket = null; return; }
          if (command === "undo_window_position") {
            if (args.revision !== undoTicket) return false;
            if (refuseUndo) { refuseUndo = false; return false; }
            undoTicket = null;
            return true;
          }
          throw Error(`Unexpected synthetic IPC: ${command}`);
        },
      };
    }, { larger, scale, startupNotice, largeNotice, pointerNotice, viewState, shortcutNotice, shelfDetails, formatFailure: noticeText.formatting, entryWarning: noticeText.entryWarning });
    await page.goto(base.href);
    await page.locator("main").waitFor();
    await waitForCheck("text-size-applied", (larger) => document.documentElement.dataset.textSize === (larger ? "larger" : "normal"), larger);
    await page.evaluate(() => document.fonts.ready);
    const scan = async () => {
      const issues = await page.evaluate(() => {
      const main = document.querySelector("main"), bounds = main.getBoundingClientRect();
      const issues = [];
      if (document.documentElement.scrollHeight > innerHeight + 1 || document.documentElement.scrollTop || document.body.scrollTop || main.scrollTop) issues.push("window scrolls");
      const active = document.querySelector(".details-menu-body") ?? main;
      const scrollers = [...active.querySelectorAll("*")].filter((node) => /auto|scroll/.test(getComputedStyle(node).overflowY) && node.clientHeight > 0 && node.scrollHeight > node.clientHeight + 1);
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
      if (!await page.evaluate(isWholeShelf)) issues.push("shelf clipped or scrolling");
      return issues;
    };
    const reachable = async (control) => {
      await control.scrollIntoViewIfNeeded();
      return control.evaluate((node) => {
        const band = node.closest(".shelf");
        if (band) {
          const rect = node.getBoundingClientRect(), frame = band.getBoundingClientRect();
          return rect.top >= frame.top - 1 && rect.bottom <= frame.bottom + 1 && rect.left >= frame.left - 1 && rect.right <= frame.right + 1;
        }
        const owner = [node.closest(".details-menu-scroll"), node.closest(".results-area"), node.closest(".picker-content")]
          .find((part) => part && /auto|scroll/.test(getComputedStyle(part).overflowY));
        const middle = (node.closest(".details-menu-scroll") ?? node.closest(".picker-content")).getBoundingClientRect();
        const bounds = owner.getBoundingClientRect();
        const top = Math.max(middle.top, bounds.top), bottom = Math.min(middle.bottom, bounds.bottom);
        const rect = node.getBoundingClientRect();
        if (rect.left < middle.left - 1 || rect.right > middle.right + 1 || bottom <= top) return false;
        if (rect.height <= bottom - top) return rect.top >= top - 1 && rect.bottom <= bottom + 1;
        // A choice taller than the viewport is still readable in full only
        // if scrolling can actually expose both ends. Test those positions.
        owner.scrollTop += rect.top - top;
        const start = node.getBoundingClientRect();
        const startVisible = start.top >= top - 1 && start.top < bottom;
        owner.scrollTop += start.bottom - bottom;
        const end = node.getBoundingClientRect();
        return startVisible && end.bottom <= bottom + 1 && end.bottom > top;
      });
    };
    if (viewState !== "loaded") {
      if (viewState === "loading") await page.locator(".empty-loading").waitFor();
      else {
        await page.getByRole("option").last().waitFor();
        for (const choice of await page.getByRole("option").all()) assert.equal(await reachable(choice), true, `${caseLabel}: every setup/recovery choice reachable`);
      }
      assert.deepEqual(await scan(), [], `${caseLabel}: whole search/footer and one scroller`);
      await page.getByRole("button", { name: "More", exact: true }).click();
      await moreMenu.waitFor();
      await dismissMenu(moreMenu);
      assert.deepEqual(page.viewportSize(), { width, height }, "choice/loading menus keep the opening frame");
      assert.deepEqual(resizes, [], "no native size requests from content");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    await page.getByText("Type to find a reference", { exact: true }).waitFor();
    assert.deepEqual(await scan(), [], `${width}/${larger}/${colorScheme}: steady empty frame`);
    assert.equal(await reachable(page.locator(".empty-description")), true, "empty hint is reachable without clipping");
    if (!small) assert.equal(await page.locator(".picker-content").evaluate((node) => node.scrollHeight <= node.clientHeight + 1), true, "default empty frame needs no scrollbar");
    const wholeNotice = async (check, text) => {
      // Both the locator and geometry oracle must select the exact same notice.
      const notice = page.getByRole("alert").and(page.getByText(text, { exact: true })).first();
      await notice.scrollIntoViewIfNeeded();
      await waitForCheck(check, isWholeNotice, text);
      assert.equal(await notice.isVisible(), true, `${caseLabel}: ${check} visible`);
      assert.equal(await notice.evaluate((node) => /auto|scroll/.test(getComputedStyle(node.closest(".notices")).overflowY)), false, "notices use the middle scroll owner");
      assert.deepEqual(await scan(), [], "whole notice keeps search/footer and one scroller");
    };
    if (startupNotice) await wholeNotice("startup-notice-whole", noticeText.startup);
    const frame = page.viewportSize(), before = resizes.length;
    await page.getByRole("button", { name: "More", exact: true }).click();
    await moreMenu.waitFor();
    await moreMenu.press("End");
    await moreMenu.getByRole("menuitem", { name: /^Quit/ }).waitFor({ state: "visible" });
    await moreMenu.press("Home");
    await dismissMenu(moreMenu);
    assert.deepEqual(page.viewportSize(), frame, "More/Escape preserve frame");
    assert.equal(resizes.length, before, "menus never request native sizing");
    const search = page.getByLabel("Search references", { exact: true });
    await search.press("Meta+/");
    await keysDialog.waitFor();
    assert.deepEqual(page.viewportSize(), frame, "the keys sheet keeps the opening frame");
    await keysDialog.press("Escape");
    await keysDialog.waitFor({ state: "detached" });
    await page.getByRole("button", { name: /^Copy as/ }).click();
    await formatMenu.waitFor();
    await dismissMenu(formatMenu, page.getByRole("searchbox", { name: "Filter formats", exact: true }));
    assert.deepEqual(page.viewportSize(), frame, "the format pop-up keeps the opening frame");
    if (!small) {
      await search.fill("refuse");
      await wholeNotice("failed-search-notice-whole", noticeText.failedSearch);
      await search.fill("");
      await page.getByText("Type to find a reference", { exact: true }).waitFor();
    }
    await page.getByRole("button", { name: "Keep Window Open" }).click();
    await search.fill("no-match");
    await page.getByText("No matching references", { exact: true }).waitFor();
    assert.deepEqual(page.viewportSize(), frame, "no matches and the pin keep the opening frame");
    await search.fill("invented");
    await page.getByRole("option").last().waitFor();
    assert.deepEqual(await scan(), [], `${width}/${larger}/${colorScheme}: results`);
    assert.deepEqual(page.viewportSize(), frame, "typing results keeps the opening frame");
    const anchoredMenu = async (anchor) => {
      const rect = await anchor.evaluate((node) => {
        const rect = node.getBoundingClientRect();
        return { top: rect.top, bottom: rect.bottom, left: rect.left, right: rect.right };
      });
      await waitForCheck("details-menu-anchored-and-contained", isAnchoredDetails, rect);
    };
    const tabLoop = async (dialog, anotherControl = true) => {
      const close = dialog.getByRole("button", { name: "Close", exact: true });
      await close.press("Shift+Tab");
      assert.equal(await close.evaluate((node) => node.closest('[role="dialog"]').contains(document.activeElement)), true, "Shift+Tab stays in the menu");
      if (anotherControl) assert.equal(await close.evaluate((node) => node !== document.activeElement), true, "Shift+Tab wraps from Close to the last control");
      await page.keyboard.press("Tab");
      assert.equal(await close.evaluate((node) => document.activeElement === node), true, "Tab wraps back to Close");
    };
    const openEditor = async (segment = false) => {
      const chip = page.locator("main > .shelf .shelf-chips > .chip").first();
      const edit = chip.getByRole("button", { name: /^Edit/ });
      const fromChip = segment && await edit.isVisible();
      const anchor = fromChip ? chip : page.locator("main > .shelf .shelf-output");
      if (fromChip) await edit.click();
      else await page.getByRole("button", { name: "Edit citation", exact: true }).click();
      const dialog = page.getByRole("dialog", { name: /^Citation details/ });
      await dialog.waitFor();
      await page.getByLabel("Page", { exact: true }).waitFor();
      await anchoredMenu(anchor);
      await tabLoop(dialog);
      await page.getByLabel("Page", { exact: true }).focus();
      return dialog;
    };
    const closeEditor = async (dialog) => {
      await dialog.getByRole("button", { name: "Close", exact: true }).click();
      await dialog.waitFor({ state: "detached" });
    };
    if (shelfDetails === "undo") {
      const band = await page.locator("main > .shelf").boundingBox();
      await page.evaluate(() => window.fixtureWindowReset());
      const undo = page.getByRole("button", { name: "Undo", exact: true });
      await undo.waitFor();
      assert.equal(await search.evaluate((node) => document.activeElement === node), true, "Reset's Undo notice leaves typing focus in Search");
      await undo.click();
      await page.getByRole("alert").and(page.getByText("Could not undo. Try again.", { exact: true })).waitFor();
      assert.equal(await undo.isEnabled(), true, "a refused Undo stays visible and retryable");
      await undo.click();
      await undo.waitFor({ state: "detached" });
      assert.equal(await search.evaluate((node) => document.activeElement === node), true, "successful retry returns focus to Search");
      assert.deepEqual(await page.locator("main > .shelf").boundingBox(), band, "Undo feedback does not move the shelf");
      assert.deepEqual(await scan(), [], "Undo feedback keeps fixed bands and one scroller");
      assert.deepEqual(page.viewportSize(), frame, "Undo feedback keeps the fixture's frame");
      assert.deepEqual(resizes, [], "no content-driven native sizing");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    if (shelfDetails) {
      let beforeShelf = await page.locator("main > .shelf").boundingBox();
      let dialog;
      if (shelfDetails === "expanded") {
        for (let i = 0; i < 9; i++) { if (i) await search.press("ArrowDown"); await search.press("Shift+Enter"); }
        beforeShelf = await page.locator("main > .shelf").boundingBox();
        const count = page.getByRole("button", { name: "Show all 9 selected references", exact: true, includeHidden: true });
        await count.click();
        dialog = page.getByRole("dialog", { name: "Selected references", exact: true });
        await dialog.waitFor();
        await anchoredMenu(count);
        const controls = dialog.getByRole("button", { name: /^Remove .* from the selection$/ });
        assert.equal(await controls.count(), 9, "every chosen reference is removable in the overlay");
        for (const control of await controls.all()) assert.equal(await reachable(control), true, "every remove target is reachable");
      } else if (shelfDetails === "warnings") {
        await search.press("Shift+Enter");
        const warnings = page.getByRole("button", { name: /2 warnings.*Reference entry.*Closest matches/, includeHidden: true });
        await warnings.waitFor();
        beforeShelf = await page.locator("main > .shelf").boundingBox();
        assert.equal(await reachable(warnings), true, "the warnings control stays reachable from the shelf");
        await warnings.press("Space");
        dialog = page.getByRole("dialog", { name: "Citation warnings", exact: true });
        await dialog.waitFor();
        await anchoredMenu(warnings);
        for (const meaning of [dialog.getByRole("heading", { name: "Reference entry", exact: true }), dialog.getByRole("heading", { name: "Closest matches", exact: true }),
          dialog.getByText(noticeText.entryWarning, { exact: true }), dialog.getByText(/IEEE numbers depend on your whole document/), dialog.getByText(/only some search words matched/)]) {
          assert.equal(await reachable(meaning), true, "every warning, reference-entry explanation and closest-match provenance is reachable");
        }
      } else {
        const refusal = page.locator(".notices").getByRole("alert").and(page.getByText(noticeText.formatting, { exact: true }));
        await refusal.waitFor();
        assert.equal(await reachable(refusal), true, "the full refusal is readable before opening Details");
        const shelfReason = page.locator("main > .shelf .shelf-output p");
        await shelfReason.waitFor();
        assert.equal(await shelfReason.textContent(), noticeText.formatting, "the reason appears beside the failed citation before Details opens");
        assert.equal(await reachable(shelfReason), true, "the refusal line stays visible in the fixed shelf");
        await page.getByRole("button", { name: "Details", exact: true }).click();
        dialog = page.getByRole("dialog", { name: /^Citation details/ });
        await dialog.waitFor();
        await anchoredMenu(page.locator("main > .shelf .shelf-output"));
        assert.equal(await reachable(dialog.locator(".shelf-detail-title")), true, "both ends of a long reference title are reachable");
        assert.equal(await reachable(dialog.getByText(noticeText.formatting, { exact: true })), true, "both ends of the full refusal are reachable");
      }
      await tabLoop(dialog, shelfDetails !== "warnings");
      assert.deepEqual(await scan(), [], "details leave whole fixed bands and one active scroller");
      const shelf = await page.locator("main > .shelf").boundingBox();
      assert.deepEqual(shelf, beforeShelf, "opening details does not grow or move the shelf");
      await closeEditor(dialog);
      assert.deepEqual(await scan(), [], "closing details restores the one browsing scroller");
      assert.deepEqual(page.viewportSize(), frame, "shelf details never change the window frame");
      assert.deepEqual(resizes, [], "no native size requests from content");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    if (pointerNotice) {
      await waitForCheck("pointer-steady-frame", (height) => innerHeight === height, height);
      await page.getByRole("region", { name: "Preview" }).waitFor();
      await page.locator(".results-area").evaluate((node) => { node.scrollTop = 380; });
      await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const row = page.locator("#result-8");
      await row.getByText("Invented reference 8", { exact: true }).click();
      await search.evaluate((node) => { node.focus({ preventScroll: true }); node.setSelectionRange(2, 5); });
      await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      assert.equal(await page.locator(".results-area").evaluate((node) => node.scrollTop), 380, "pointer browsing retains offset 380");
      let top = await row.evaluate((node) => node.getBoundingClientRect().top);
      const anchored = (top) => {
        const row = document.querySelector("#result-8"), rect = row.getBoundingClientRect();
        const port = row.closest(".picker-content").getBoundingClientRect();
        const search = document.querySelector("#search");
        return row.getAttribute("aria-selected") === "true" && Math.abs(rect.top - top) <= 1 && rect.top >= port.top - 1 && rect.bottom <= port.bottom + 1 &&
          document.activeElement === search && search.selectionStart === 2 && search.selectionEnd === 5;
      };
      const owners = () => page.evaluate(() => [document.querySelector("main"), document.querySelector(".picker-content"), document.querySelector(".results-area")]
        .flatMap((node, index) => /auto|scroll/.test(getComputedStyle(node).overflowY) && node.clientHeight > 0 && node.scrollHeight > node.clientHeight + 1 ? [index] : []));
      await page.evaluate(() => window.fixtureNotice(true));
      await waitForCheck("pointer-notice-appeared", () => document.querySelector(".notices").childElementCount > 0);
      await waitForCheck("pointer-notice-appear-anchor", anchored, top);
      assert.deepEqual(await owners(), [1], "notice uses only the middle scroller");
      assert.deepEqual(await scan(), [], "pointer notice keeps search/footer whole and one scroller");
      // Progress has no clear event. Settings notices exercise the real appear /
      // clear path, including browsing to a newer place before they clear.
      if (pointerNotice === "settings") {
        await page.locator(".picker-content").evaluate((node) => { node.scrollTop += 60; });
        top -= 60;
        await waitForCheck("pointer-notice-browsed", anchored, top);
        await page.evaluate(() => window.fixtureNotice(false));
        await waitForCheck("pointer-notice-cleared", () => document.querySelector(".notices").childElementCount === 0);
        await waitForCheck("pointer-notice-clear-anchor", anchored, top);
        assert.deepEqual(await owners(), [2], "clearing restores only the results scroller");
        assert.deepEqual(await scan(), [], "cleared notice keeps search/footer whole and one scroller");
      }
      for (let i = 0; i < 5; i++) await search.press("ArrowDown");
      await waitForCheck("pointer-keyboard-takeover-reveals", () => {
        const row = document.querySelector("#result-13"), rect = row.getBoundingClientRect();
        const area = row.closest(".results-area"), middle = area.parentElement;
        const owner = /auto|scroll/.test(getComputedStyle(area).overflowY) ? area : middle;
        const port = owner.getBoundingClientRect();
        return row.getAttribute("aria-selected") === "true" && rect.top >= port.top - 1 && rect.bottom <= port.bottom + 1;
      });
      assert.deepEqual(await scan(), [], "keyboard reveal keeps search/footer whole and one scroller");
      assert.deepEqual(resizes, [], "no native size requests from content");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
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
      assert.equal(await page.evaluate(isWholeShelf), true, "the shelf stays outside the overflowing notice region");
      await search.evaluate((node) => { node.focus({ preventScroll: true }); node.setSelectionRange(2, 5); });
      const selection = () => search.evaluate((node) => ({ focused: document.activeElement === node, range: [node.selectionStart, node.selectionEnd],
        references: [...document.querySelectorAll('.shelf-chips > .chip')].map((chip) => chip.title) }));
      const chosen = await selection();
      const owners = () => page.evaluate(() => [document.querySelector("main"), ...document.querySelectorAll("main *")]
        .filter((node) => /auto|scroll/.test(getComputedStyle(node).overflowY) && node.clientHeight > 0 && node.scrollHeight > node.clientHeight + 1)
        .map((node) => node.className));
      assert.deepEqual(await scan(), [], `720x480/${larger}: large notice with selection`);
      assert.deepEqual(await owners(), ["picker-content scroll-area"], "large notices and results share the middle scroller");
      for (const height of [360, 480, 800, 480]) {
        await page.setViewportSize({ width, height });
        await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert.deepEqual(await selection(), chosen, "layout changes preserve focus, query range and selected references");
        {
          assert.deepEqual(await scan(), [], "growing the viewport restores the bounded middle");
          assert.deepEqual(await owners(), ["picker-content scroll-area"], "notice fallback survives viewport growth");
        }
      }
      for (const control of [page.getByRole("list", { name: "Copy warnings" }).locator("li").first(), page.getByRole("list", { name: "Copy warnings" }).locator("li").last(), page.getByRole("region", { name: "Preview" })]) {
        assert.equal(await reachable(control), true, "notice ends and the fixed preview remain reachable");
        assert.deepEqual(await scan(), [], "revealing a notice keeps all three bands whole");
      }
      const editor = await openEditor();
      for (const control of [page.getByLabel("Page", { exact: true }), page.getByRole("radiogroup", { name: "Citation form" })]) {
        assert.equal(await reachable(control), true, "Page/Form remain reachable in the shared overlay");
      }
      await closeEditor(editor);
      await search.evaluate((node) => node.focus({ preventScroll: true }));
      assert.deepEqual(await selection(), chosen, "details preserve query range and selected references");
      await page.getByRole("button", { name: "Dismiss copy warnings" }).click();
      await page.getByRole("list", { name: "Copy warnings" }).waitFor({ state: "detached" });
      assert.deepEqual(await owners(), ["results-area scroll-area"], "empty notices restore the results scroller");
      assert.deepEqual((await selection()).references, chosen.references, "dismissing warnings keeps selected references");
      assert.deepEqual(await scan(), [], "dismissed notice leaves one scroller");
      assert.deepEqual(resizes, [], "no native size requests from content");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    if (small) {
      await page.getByRole("region", { name: "Preview" }).getByText("[@synthetic0]", { exact: true }).waitFor();
      await waitForCheck("results-announced", () => document.querySelector('[aria-label="Search results"]').textContent.trim().length > 0);
      for (const nextHeight of [480, 360]) {
        await page.setViewportSize({ width, height: nextHeight });
        await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert.deepEqual(await scan(), [], "dragging preserves the bounded middle and whole search/footer");
      }
    }
    assert.equal(await reachable(page.getByRole("region", { name: "Preview" })), true, "preview is whole in its fixed band");
    let editor = await openEditor();
    for (const control of [page.getByLabel("Page", { exact: true }), page.getByRole("radiogroup", { name: "Citation form" })]) {
      assert.equal(await reachable(control), true, "Page/Form remain reachable in the shared overlay");
      assert.deepEqual(await scan(), [], "details keep the fixed bands whole and one active scroller");
    }
    await closeEditor(editor);
    await search.press("Shift+Enter");
    await page.getByRole("region", { name: "Preview" }).waitFor();
    await page.setViewportSize({ width: 720, height: Math.max(height, 480) });
    editor = await openEditor(true);
    await page.getByLabel("Page", { exact: true }).fill("12");
    await page.setViewportSize({ width: 480, height: 360 });
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.deepEqual(await scan(), [], "resizing while editing keeps fixed bands and one active scroller");
    const foldedAnchor = page.locator("main > .shelf .shelf-output > button[aria-label^='Show all']");
    // A visible segment folds to the count; an editor opened from the whole
    // citation stays on that citation. Both anchors stay inside the window.
    const anchor = await foldedAnchor.getAttribute("aria-expanded") === "true" ? foldedAnchor : page.locator("main > .shelf .shelf-output");
    await anchoredMenu(anchor);
    const heading = editor.getByRole("heading");
    assert.equal(await heading.evaluate((node) => {
      const title = node.getBoundingClientRect();
      return title.top >= 0 && title.bottom <= innerHeight && title.left >= 0 && title.right <= innerWidth;
    }), true, "the chosen reference stays readable in its editor at minimum size");
    if (scale === 1) assert.equal(await page.locator('[role="option"][aria-selected="true"] .result-title').evaluate((node) => {
      const title = node.getBoundingClientRect(), port = node.closest(".picker-content").getBoundingClientRect();
      return title.top >= port.top - 1 && title.bottom <= port.bottom + 1;
    }), true, "the selected result also stays within the middle while editing");
    assert.equal(await reachable(page.getByLabel("Page", { exact: true })), true, "the editor remains reachable");
    assert.equal(await page.getByLabel("Page", { exact: true }).evaluate((node) => document.activeElement === node), true, "resizing preserves Page focus");
    assert.equal(await page.getByLabel("Page", { exact: true }).inputValue(), "12", "folding preserves the person's edit");
    await closeEditor(editor);
    await page.setViewportSize({ width, height });
    assert.deepEqual(await scan(), [], `${width}/${larger}/${colorScheme}: shelf`);
    await search.fill("refuse");
    await page.getByRole("alert").and(page.getByText(noticeText.failedSearch, { exact: true })).first().waitFor();
    assert.deepEqual(await scan(), [], `${width}/${larger}/${colorScheme}: failed search retains footer and one scroller`);
    // Consumer oracle must reject a genuinely clipped frame, not only read CSS.
    await page.evaluate(() => { document.querySelector("main").style.height = "1px"; });
    assert.ok((await scan()).includes("footer action clipped"), "same geometry oracle detects clipped footer");
    assert.deepEqual(resizes, [], "no native size requests from content");
    assert.deepEqual(errors, [], "no page errors");
    await page.close(); profiles += 1;
  }
  console.log(`Picker layout: ${profiles} browser profiles passed (default/minimum, normal/Larger/200%, light/dark, setup/loading/recovery, notices and editing).`);
} finally { await browser.close(); }
