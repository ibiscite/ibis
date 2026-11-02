// Real built picker + invented IPC. Browser layout evidence, not native input/engine proof.
// Serve `pnpm build` with `pnpm exec vite preview`; reuse an installed Playwright.
import assert from "node:assert/strict";
import { dismissMenu, hasMiddleViewport, isAnchoredDetails, isWholeNotice, isWholeRadioTarget, isWholeShelf, noticeText, refusalGeometry, refusalPresentation, shelfControl, waitForLayout } from "./picker-layout-checks.mjs";
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
    ["single", "expanded", "failed", "warnings", "undo", "copied", "rtl", "forms", "recents", "receipt"].map((shelfDetails) => ({ width, larger, colorScheme: "light", scale: 1, shelfDetails })))));
  cases.push(...["single", "expanded", "failed", "warnings", "undo", "copied", "rtl", "forms", "recents", "receipt"].map((shelfDetails) => ({ width: 480, larger: false, colorScheme: "light", scale: 2, shelfDetails })));
  for (const { width, larger, colorScheme, scale, shelfDetails = null, startupNotice = false, largeNotice = false, pointerNotice = null, viewState = "loaded", shortcutNotice = false } of cases) {
    const small = width === 480;
    const height = small ? 360 : largeNotice || pointerNotice ? 480 : Math.round(480 * (larger ? 1.15 : 1));
    const page = await browser.newPage({ viewport: { width, height }, colorScheme, reducedMotion: shelfDetails === "copied" && scale === 2 ? "reduce" : "no-preference" });
    const caseLabel = `${width}/${larger ? "larger" : "normal"}/${colorScheme}/scale-${scale}/${shelfDetails ? `shelf-${shelfDetails}` : shortcutNotice ? "shortcut-notice" : startupNotice ? "startup-notice" : largeNotice ? "large-notice" : pointerNotice ? `pointer-${pointerNotice}` : viewState}`;
    const waitForCheck = (check, predicate, arg) => waitForLayout(page, caseLabel, check, predicate, arg);
    const errors = [], resizes = [];
    // Exit must finish even if a modal temporarily hides the old menu from ARIA.
    const moreMenu = page.getByRole("menu", { name: "More", exact: true, includeHidden: true });
    const formatMenu = page.getByRole("menu", { name: /^Copy as/, includeHidden: true });
    const keysDialog = page.getByRole("dialog", { name: "Keyboard Shortcuts", exact: true });
    page.setDefaultTimeout(5000);
    page.on("pageerror", (error) => errors.push(error.message));
    await page.exposeFunction("fixtureSizing", (command) => { resizes.push(command); });
    await page.addInitScript(({ larger, scale, startupNotice, largeNotice, pointerNotice, viewState, shortcutNotice, shelfDetails, formatFailure, entryWarning }) => {
      if (scale !== 1) document.addEventListener("DOMContentLoaded", () => { document.documentElement.style.fontSize = `${scale * 100}%`; });
      Object.defineProperty(navigator, "platform", { value: "MacIntel" });
      let callback = 0;
      const callbacks = new Map(), listeners = new Map();
      let undoTicket = null, refuseUndo = true, activityTicket = 0, previewNumber = 0;
      const prepared = new Map();
      if (shelfDetails === "recents") localStorage.setItem("ibis-citation-history", JSON.stringify([{ app: "invented.writer", items: [{ library: "invented.bib", key: "synthetic0" }] }]));
      const row = (i, recent) => ({ revision: "1", id: { library: "invented.bib", key: `synthetic${i}` }, cite: `synthetic-cite-${i}`, source: "invented.bib", key: `synthetic${i}`, title: shelfDetails === "failed" ? `Invented reference ${i}. ${"A synthetic long title. ".repeat(30)}`.trim() : `Invented reference ${i}`, authors: "Aster", first_author: "Aster", author_count: 1, year: 2026, container: null, item_type: "article-journal", partial: shelfDetails === "warnings" && i === 0, matched: ["title"], ...(recent ? { recent } : {}) });
      if (shelfDetails === "rtl") document.addEventListener("DOMContentLoaded", () => { document.documentElement.dir = "rtl"; });
      window.fixtureWindowReset = () => {
        undoTicket = 7;
        const event = "picker:window-reset";
        const ids = [...listeners].filter(([, name]) => name === event).map(([id]) => id);
        if (!ids.length) throw Error("Synthetic reset listener unavailable");
        for (const id of ids) callbacks.get(id)?.({ event, id, payload: { revision: 7, undo: 7 } });
      };
      const preferences = { citation_style: shelfDetails === "warnings" ? "ieee" : shelfDetails === "forms" ? "apa" : null, density: "comfortable", larger_text: larger, show_paste_preview: true, search_icon: "ibis", paste: false, menu_bar: true };
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
          if (command === "picker_presentation") return 0;
          if (command === "native_appearance") return "opaque";
          if (command === "picker_preferences") return preferences;
          if (command === "set_picker_preferences") return { preferences: Object.assign(preferences, args.preferences), copy_format: "pandoc" };
          if (command === "copy_format") return "pandoc";
          if (command === "picker_destination") return { app: shelfDetails === "recents" ? "Invented Writer" : null, app_id: shelfDetails === "recents" ? "invented.writer" : null, app_identity: null, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null, paste: null, paste_question: false, can_choose_paste: false };
          if (command === "shortcut_status") return { shortcut: "CommandOrControl+Shift+Y", registered: true, error: shortcutNotice ? "Synthetic shortcut refusal" : null };
          if (command === "library_status") return library;
          if (command === "search" && args.query.text === "refuse") throw Error("synthetic search refusal");
          if (command === "recent_references") return { revision: "1", results: shelfDetails === "recents" && args.ids.length ? Array.from({ length: args.includeAdded ? 6 : 3 }, (_, i) => row(i, i < 3 ? "cited" : "added")) : [] };
          if (command === "search") return { revision: "1", results: args.query.text && args.query.text !== "no-match" ? Array.from({ length: 14 }, (_, i) => row(i)) : [],
            ...(args.query.interpret_locators && / p12$/.test(args.query.text) ? { interpretation: { search: "invented", locator: { label: "page", value: "12" }, typed: "p12" } } : {}) };
          if (command === "preview_file_citation" && args.intent.cancel) throw { code: "failed", message: "Synthetic preview replaced" };
          if (command === "preview_file_citation" && shelfDetails === "failed") throw { code: "failed", message: formatFailure };
          if (command === "preview_file_citation") {
            const ticket = `synthetic-ticket-${++previewNumber}`;
            const spans = [{ item: null, text: "[" }];
            args.items.forEach((item, index) => {
              if (index) spans.push({ item: null, text: "; " });
              const page = item.page_input || item.locator?.value;
              const key = shelfDetails === "rtl" ? `${"مثال-".repeat(48)}${index}` : ["single", "receipt"].includes(shelfDetails) ? `${"invented-long-key-".repeat(16)}${index}` : item.id.key;
              const text = args.style === "ieee" ? "Aster, Invented reference, 2026."
                : args.style === "apa" ? item.form === "year-only" ? `(2026${page ? `, p. ${page}` : ""})` : item.form === "narrative" ? `Aster (2026${page ? `, p. ${page}` : ""})` : `(Aster, 2026${page ? `, p. ${page}` : ""})`
                : `${item.form === "year-only" ? "-" : ""}@${key}${page ? `, p. ${page}` : ""}`;
              spans.push({ item: index, text });
            });
            spans.push({ item: null, text: "]" });
            if (args.style) { spans.shift(); spans.pop(); }
            const text = spans.map((span) => span.text).join("");
            prepared.set(ticket, { text, items: args.items, style: args.style });
            return { ticket, content_hash: "synthetic-hash", text, spans, html: null, warnings: shelfDetails === "warnings" ? [entryWarning] : [] };
          }
          if (command === "citation_details") {
            const saved = prepared.get(args.ticket), item = saved.items[args.item];
            return { current_form: saved.style === "ieee" ? null : item.form || "normal", forms: [
              { form: "normal", example: saved.style ? "(Aster, 2026)" : "[@synthetic0]", reason: saved.style === "ieee" ? "This style copies a reference entry." : null },
              { form: "narrative", example: saved.style ? "Aster (2026)" : "@synthetic0", reason: saved.items.length > 1 ? "Cite this reference alone for Name in the sentence." : saved.style === "ieee" ? "This style copies a reference entry." : null },
              { form: "year-only", example: saved.style ? "(2026)" : "[-@synthetic0]", reason: saved.style === "ieee" ? "This style cannot show Year only." : null },
            ] };
          }
          if (command === "copy_file_citation") {
            const saved = prepared.get(args.ticket);
            return { text: saved.text, summary: shelfDetails === "receipt" ? { text: saved.text.slice(0, 119) + "…", more: saved.items.length - 1 } : null,
              warnings: largeNotice ? Array.from({ length: 20 }, (_, i) => `Invented copy warning ${i + 1}: check the reference metadata.`) : [] };
          }
          if (command === "login_item") return { name: "Ibis", enabled: false, problem: null };
          if (command === "update_checks") return null;
          if (command === "update_available") return false;
          if (command === "update_picker_activity") return args.ticket === null ? ++activityTicket : args.ticket === activityTicket ? activityTicket : null;
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
      if (!await page.evaluate(hasMiddleViewport)) issues.push("middle viewport collapsed");
      if (!await page.evaluate(isWholeShelf)) issues.push("shelf clipped or scrolling");
      return issues;
    };
    const checkScan = async (check) => assert.deepEqual(await scan(), [], `${caseLabel}: ${check}`);
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
      // The delayed opening screen exposes its library name on the progress bar.
      if (viewState === "loading") await page.getByRole("progressbar", { name: "Opening invented.bib…", exact: true }).waitFor();
      else {
        await page.getByRole("option").last().waitFor();
        for (const choice of await page.getByRole("option").all()) assert.equal(await reachable(choice), true, `${caseLabel}: every setup/recovery choice reachable`);
      }
      await checkScan("whole search/footer and one scroller");
      await page.getByRole("button", { name: "More", exact: true }).click();
      await moreMenu.waitFor();
      await dismissMenu(moreMenu);
      assert.deepEqual(page.viewportSize(), { width, height }, "choice/loading menus keep the opening frame");
      assert.deepEqual(resizes, [], "no native size requests from content");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    if (shelfDetails === "recents") {
      await page.getByRole("heading", { name: "Cited here recently", exact: true }).waitFor();
      await page.getByRole("heading", { name: "Recently added", exact: true }).waitFor();
      const search = page.getByLabel("Search references", { exact: true });
      assert.equal(await page.getByRole("option").count(), 6, "recents use the ordinary combined result list");
      await search.press("ArrowDown"); await search.press("ArrowDown"); await search.press("ArrowDown");
      assert.equal(await page.getByRole("option", { selected: true }).getAttribute("id"), "result-3", "arrows cross the two headings without a second list");
      await search.press("Shift+Enter");
      await search.fill("invented p12");
      const token = page.getByRole("group", { name: "Using page 12", exact: true });
      await token.waitFor();
      assert.equal(await token.evaluate((node) => Boolean(node.closest(".search-field"))), true, "query Page token stays beside Search");
      for (const apply of await token.getByRole("button", { name: /^Apply locator to/ }).all()) await apply.click({ trial: true });
      await token.getByRole("button", { name: "Search as typed", exact: true }).click();
      await token.waitFor({ state: "detached" });
      await checkScan("recents and the query-page token keep the fixed bands whole");
      await page.getByRole("button", { name: "More", exact: true }).click();
      await moreMenu.getByRole("menuitem", { name: "Forget Recent Citations", exact: true }).click();
      await moreMenu.waitFor({ state: "detached" });
      assert.equal(await page.evaluate(() => localStorage.getItem("ibis-citation-history")), "[]", "recents are forgettable without Settings");
      assert.deepEqual(resizes, [], "recents never request native sizing");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    await page.getByText("Type to find a reference", { exact: true }).waitFor();
    await checkScan("steady empty frame");
    assert.equal(await reachable(page.locator(".empty-description")), true, "empty hint is reachable without clipping");
    if (!small) assert.equal(await page.locator(".picker-content").evaluate((node) => node.scrollHeight <= node.clientHeight + 1), true, "default empty frame needs no scrollbar");
    const wholeNotice = async (check, text) => {
      // Both the locator and geometry oracle must select the exact same notice.
      const notice = page.getByRole("alert").and(page.getByText(text, { exact: true })).first();
      await notice.scrollIntoViewIfNeeded();
      await waitForCheck(check, isWholeNotice, text);
      assert.equal(await notice.isVisible(), true, `${caseLabel}: ${check} visible`);
      assert.equal(await notice.evaluate((node) => /auto|scroll/.test(getComputedStyle(node.closest(".notices")).overflowY)), false, "notices use the middle scroll owner");
      await checkScan("whole notice keeps search/footer and one scroller");
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
    await checkScan("results");
    assert.deepEqual(page.viewportSize(), frame, "typing results keeps the opening frame");
    const anchoredMenu = async (anchor) => {
      const rect = await anchor.evaluate((node) => {
        const rect = node.getBoundingClientRect();
        return { top: rect.top, bottom: rect.bottom, left: rect.left, right: rect.right };
      });
      await waitForCheck("details-menu-anchored-and-contained", isAnchoredDetails, rect);
    };
    const openEditor = async () => {
      const open = async () => {
        const edit = shelfControl(page, /^Edit /).first();
        const count = shelfControl(page, /^Show all /);
        if (await edit.count()) { await edit.click(); return edit; }
        await count.click();
        const choices = page.getByRole("dialog", { name: "Selected references", exact: true });
        await choices.getByRole("button", { name: /^Edit / }).first().click();
        return count;
      };
      let anchor = await open();
      let dialog = page.getByRole("dialog", { name: /^Citation details/ });
      await dialog.waitFor();
      const pageField = dialog.getByRole("textbox", { name: "Page", exact: true });
      assert.equal(await pageField.evaluate((node) => document.activeElement === node), true, "opening a segment focuses Page");
      await anchoredMenu(anchor);
      await pageField.press("Tab");
      await dialog.waitFor({ state: "detached" });
      assert.equal(await page.evaluate(() => document.activeElement !== document.body && !document.activeElement.closest(".details-menu")), true, "Tab closes details and moves on");
      anchor = await open();
      dialog = page.getByRole("dialog", { name: /^Citation details/ });
      await dialog.waitFor();
      await anchoredMenu(anchor);
      return dialog;
    };
    const closeEditor = async (dialog) => {
      await dialog.press("Escape");
      await dialog.waitFor({ state: "detached" });
    };
    const checkFormPointers = async (dialog) => {
      for (const radio of await dialog.getByRole("radio").all()) {
        if (await radio.isDisabled()) continue;
        await radio.scrollIntoViewIfNeeded();
        assert.equal(await radio.evaluate(isWholeRadioTarget), true, "the enabled native radio fills its row and receives pointer hits");
        await radio.click({ trial: true });
      }
      const remove = dialog.getByRole("button", { name: /^Remove from Citation/ });
      if (await remove.isEnabled()) await remove.click({ trial: true });
    };
    if (shelfDetails === "forms") {
      const dialog = await openEditor();
      await dialog.locator(".citation-forms").getByText("(Aster, 2026)", { exact: true }).waitFor();
      await checkFormPointers(dialog);
      const year = dialog.getByRole("radio", { name: "Year only", exact: true });
      await year.check();
      await page.locator(".shelf .preview-text").filter({ hasText: /^\(2026\)$/ }).waitFor();
      await dialog.getByRole("textbox", { name: "Page", exact: true }).fill("9");
      await page.locator(".shelf .preview-text").filter({ hasText: /^\(2026, p\. 9\)$/ }).waitFor();
      await closeEditor(dialog);
      await search.press("ArrowDown"); await search.press("Shift+Enter");
      const mixed = await openEditor();
      const narrative = mixed.getByRole("radio", { name: "Name in the sentence", exact: true });
      assert.equal(await narrative.isDisabled(), true, "unsupported group form is disabled before editing");
      await mixed.getByText("Cite this reference alone for Name in the sentence.", { exact: true }).waitFor();
      await checkFormPointers(mixed);
      await closeEditor(mixed);
      await checkScan("current-style examples and true Year only keep one shelf line");
      assert.deepEqual(resizes, [], "forms never request native sizing");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    if (shelfDetails === "receipt") {
      for (let i = 0; i < 5; i++) { if (i) await search.press("ArrowDown"); await search.press("Shift+Enter"); }
      await search.press("Enter");
      const receipt = page.locator(".copy-notice .copy-receipt");
      await receipt.getByText("+4 more", { exact: true }).waitFor();
      assert.equal(await reachable(receipt), true, "the summary and remaining-reference count are reachable");
      assert.equal(await receipt.locator("code").evaluate((node) => { const excerpt = node.querySelector(".copy-receipt-excerpt").getBoundingClientRect(), more = node.querySelector(".copy-receipt-more").getBoundingClientRect(); return getComputedStyle(node).whiteSpace === "nowrap" && Math.abs(excerpt.top - more.top) < 1 && more.right <= node.getBoundingClientRect().right + 1; }), true, "the citation stays one line and keeps its remaining count in view");
      await checkScan("the receipt summary keeps the shelf one line");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    if (shelfDetails === "undo") {
      const band = await page.locator("main > .shelf").boundingBox();
      await search.press("Shift+Enter");
      await shelfControl(page, /^Edit /).first().press("Backspace");
      const removal = page.locator(".undo-notices > .undo-notice").first();
      await removal.getByText(/^Removed .* from citation\.$/).waitFor();
      await page.evaluate(() => window.fixtureWindowReset());
      const undo = page.locator(".undo-notices > .undo-notice").nth(1).getByRole("button", { name: /^Undo/ });
      await undo.waitFor();
      assert.equal(await search.evaluate((node) => document.activeElement === node), true, "Reset's Undo notice leaves typing focus in Search");
      await waitForCheck("independent-undo-actions-whole-and-uncovered", () => {
        const boxes = [...document.querySelectorAll(".undo-notices > .undo-notice")].map((node) => node.getBoundingClientRect());
        return boxes.length === 2 && boxes.every((box) => box.top >= 7 && box.bottom <= innerHeight - 7 && box.left >= 7 && box.right <= innerWidth - 7) && boxes[0].bottom <= boxes[1].top;
      });
      await undo.click();
      await page.getByRole("alert").and(page.getByText("Could not undo. Try again.", { exact: true })).waitFor();
      assert.equal(await undo.isEnabled(), true, "a refused Undo stays visible and retryable");
      await undo.click();
      await undo.waitFor({ state: "detached" });
      assert.equal(await search.evaluate((node) => document.activeElement === node), true, "successful retry returns focus to Search");
      await removal.getByRole("button", { name: /^Undo/ }).click();
      await removal.waitFor({ state: "detached" });
      assert.equal(await page.locator('[role="option"][data-citation-selected]').count(), 1, "undoing the window leaves the independent reference Undo intact");
      assert.deepEqual(await page.locator("main > .shelf").boundingBox(), band, "Undo feedback does not move the shelf");
      await checkScan("Undo feedback keeps fixed bands and one scroller");
      assert.deepEqual(page.viewportSize(), frame, "Undo feedback keeps the fixture's frame");
      assert.deepEqual(resizes, [], "no content-driven native sizing");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    if (shelfDetails === "copied") {
      const text = await page.locator("main > .shelf .preview-text").textContent();
      await search.press("Enter");
      await page.getByRole("status", { name: "Copied", exact: true }).waitFor();
      assert.equal(await page.locator("main > .shelf .preview-text").textContent(), text, "Copied confirms the exact receipt");
      assert.equal(await page.locator(".copy-notice p").count(), 0, "the shelf confirms each copy once");
      await waitForCheck("matching-copied-beaks", () => {
        const open = document.querySelector(".shelf-mark-open").getBoundingClientRect();
        const close = document.querySelector(".shelf-mark-close").getBoundingClientRect();
        const text = document.querySelector(".shelf .preview-text").getBoundingClientRect();
        return Math.abs(open.width - close.width) <= 1 && Math.abs(open.height - close.height) <= 1 &&
          Math.abs((text.left - open.right) - (close.left - text.right)) <= 1;
      });
      if (scale === 2) assert.equal(await page.locator(".shelf").evaluate((node) => getComputedStyle(node).animationName), "none", "Reduce Motion shows only the Copied end state");
      await checkScan("Copied keeps one whole shelf line");
      await search.fill("newer");
      await page.getByRole("status", { name: "Copied", exact: true }).waitFor({ state: "detached" });
      assert.deepEqual(resizes, [], "Copied never requests native sizing");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    if (shelfDetails === "single" || shelfDetails === "rtl") {
      await search.press("Shift+Enter");
      const band = await page.locator("main > .shelf").boundingBox();
      const segment = shelfControl(page, /^Edit /).first();
      await segment.hover();
      const remove = shelfControl(page, "Remove synthetic0 from the selection");
      await remove.waitFor({ state: "visible" });
      assert.equal(await remove.evaluate((node) => {
        const rect = node.getBoundingClientRect();
        const text = node.closest(".shelf-segment").querySelector(".preview-text").getBoundingClientRect();
        const size = rect.width * 2 / 3;
        const top = rect.top + (rect.height - size) / 2, left = rect.left + (rect.width - size) / 2;
        return Math.abs(rect.width - rect.height) <= 1 && rect.width >= 23 &&
          (top + size <= text.top + 1 || left + size <= text.left + 1 || top >= text.bottom - 1 || left >= text.right - 1);
      }), true, "the square remove button's glyph stays outside the long key's text at every text size/direction");
      await remove.click();
      assert.equal(await page.getByRole("dialog").count(), 0, "the remove badge never opens the menu");
      await page.getByRole("button", { name: /^Undo/ }).click();
      assert.equal(await page.locator('[role="option"][data-citation-selected]').count(), 1, "Undo restores the removed reference");
      const dialog = await openEditor();
      const fullCitation = dialog.getByRole("region", { name: "Full citation", exact: true });
      assert.equal(await fullCitation.textContent(), await segment.locator(".preview-text").textContent(), "the single-reference menu keeps the entire citation available without hover");
      await dialog.getByRole("textbox", { name: "Page", exact: true }).press("ArrowDown");
      await page.keyboard.press("End");
      assert.equal(await fullCitation.evaluate((node) => document.activeElement === node), true, "menu keys reach the complete citation without hover");
      assert.equal(await reachable(fullCitation), true, "the complete long/RTL citation is reachable in the menu");
      await dialog.getByRole("textbox", { name: "Page", exact: true }).fill("iv–vi");
      for (const control of await dialog.getByRole("radio").all()) assert.equal(await reachable(control), true, "all name forms stay whole and reachable");
      await checkFormPointers(dialog);
      assert.deepEqual(await page.locator("main > .shelf").boundingBox(), band, "the long citation and menu never grow the shelf");
      await closeEditor(dialog);
      const reopened = await openEditor();
      assert.equal(await reopened.getByRole("textbox", { name: "Page", exact: true }).inputValue(), "iv–vi", "close/reopen keeps the page");
      await closeEditor(reopened);
      await segment.focus();
      assert.equal(await remove.isVisible(), false, "keyboard focus never shows the hover badge");
      await segment.press("Backspace");
      await page.getByRole("button", { name: /^Undo/ }).waitFor();
      await checkScan("long/RTL text and removal keep the shelf whole");
      assert.deepEqual(resizes, [], "no native size requests from the shelf");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    if (shelfDetails) {
      let beforeShelf = await page.locator("main > .shelf").boundingBox();
      let dialog;
      if (shelfDetails === "expanded") {
        for (let i = 0; i < 9; i++) { if (i) await search.press("ArrowDown"); await search.press("Shift+Enter"); }
        beforeShelf = await page.locator("main > .shelf").boundingBox();
        const segments = page.locator("main > .shelf .shelf-segment-button");
        await segments.nth(8).waitFor({ state: "attached" });
        assert.equal(await segments.count(), 9, "the citation has one engine-owned segment per reference");
        await segments.first().focus(); await segments.first().press("ArrowRight");
        assert.equal(await segments.nth(1).evaluate((node) => document.activeElement === node), true, "arrow selection follows citation order");
        await segments.nth(1).press("Space");
        await page.getByRole("textbox", { name: "Page", exact: true }).waitFor();
        await closeEditor(page.getByRole("dialog", { name: /^Citation details/ }));
        const count = shelfControl(page, "9 references");
        await count.click();
        dialog = page.getByRole("dialog", { name: "Selected references", exact: true });
        await dialog.waitFor();
        await anchoredMenu(count);
        const controls = dialog.getByRole("button", { name: /^Edit / });
        assert.equal(await controls.count(), 9, "every chosen reference has its own editor in the overlay");
        for (const control of await controls.all()) {
          assert.equal(await reachable(control), true, "every reference menu is reachable");
          await control.click({ trial: true });
        }
      } else if (shelfDetails === "warnings") {
        await search.press("Shift+Enter");
        const warnings = shelfControl(page, /2 warnings.*Reference entry.*Closest matches/);
        await warnings.waitFor();
        await warnings.click({ trial: true });
        beforeShelf = await page.locator("main > .shelf").boundingBox();
        assert.equal(await reachable(warnings), true, "the warnings control stays reachable from the shelf");
        const folded = await page.locator("main > .shelf").getAttribute("data-folded") !== null;
        assert.equal(await warnings.getByText(folded ? "! 2" : "2 warnings", { exact: true }).isVisible(), true, "warning and closest-match choices have a visible cue before opening");
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
        const refusalState = await refusal.evaluate(refusalPresentation);
        const refusalDiagnostic = await refusal.evaluate(refusalGeometry);
        assert.deepEqual(refusalState, {
          startsInView: true, wrapsInFull: true, keyboardReachable: true,
        }, `${caseLabel}: a new refusal starts in view, wraps in full and is keyboard reachable before Details; geometry=${JSON.stringify(refusalDiagnostic)}`);
        assert.equal(await search.evaluate((node) => document.activeElement === node), true, "a refusal never takes focus from Search");
        assert.equal(await reachable(refusal), true, "the full refusal is readable before opening Details");
        const shelfReason = page.locator("main > .shelf .shelf-output p");
        await shelfReason.waitFor();
        assert.equal(await shelfReason.textContent(), noticeText.formatting, "the reason appears beside the failed citation before Details opens");
        assert.equal(await reachable(shelfReason), true, "the refusal line stays visible in the fixed shelf");
        await shelfControl(page, "Details").click();
        dialog = page.getByRole("dialog", { name: /^Citation details/ });
        await dialog.waitFor();
        await anchoredMenu(page.locator("main > .shelf .shelf-output"));
        assert.equal(await reachable(dialog.locator(".shelf-detail-title")), true, "both ends of a long reference title are reachable");
        assert.equal(await reachable(dialog.getByText(noticeText.formatting, { exact: true })), true, "both ends of the full refusal are reachable");
      }
      await checkScan("details leave whole fixed bands and one active scroller");
      const shelf = await page.locator("main > .shelf").boundingBox();
      assert.deepEqual(shelf, beforeShelf, "opening details does not grow or move the shelf");
      await closeEditor(dialog);
      await checkScan("closing details restores the one browsing scroller");
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
      await checkScan("pointer notice keeps search/footer whole and one scroller");
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
        await checkScan("cleared notice keeps search/footer whole and one scroller");
      }
      for (let i = 0; i < 5; i++) await search.press("ArrowDown");
      await waitForCheck("pointer-keyboard-takeover-reveals", () => {
        const row = document.querySelector("#result-13"), rect = row.getBoundingClientRect();
        const area = row.closest(".results-area"), middle = area.parentElement;
        const owner = /auto|scroll/.test(getComputedStyle(area).overflowY) ? area : middle;
        const port = owner.getBoundingClientRect();
        return row.getAttribute("aria-selected") === "true" && rect.top >= port.top - 1 && rect.bottom <= port.bottom + 1;
      });
      await checkScan("keyboard reveal keeps search/footer whole and one scroller");
      assert.deepEqual(resizes, [], "no native size requests from content");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    if (largeNotice) {
      await page.locator(".shelf .preview-text").filter({ hasText: /^\[@synthetic0\]$/ }).waitFor();
      await search.press("Enter");
      await page.getByRole("list", { name: "Copy warnings" }).waitFor();
      // Copy consumes the old selection; choose two references while its warnings remain.
      await search.press("Shift+Enter");
      await search.press("ArrowDown");
      await search.press("Shift+Enter");
      await waitForCheck("large-notice-two-selected", () => document.querySelector(".shelf-count")?.getAttribute("aria-label") === "2 references");
      assert.deepEqual(page.viewportSize(), { width: 720, height: 480 }, "large-notice fixture uses the reported frame");
      assert.ok(await page.locator(".notices").evaluate((node) => node.getBoundingClientRect().height >= 300), "warnings consume at least 300px");
      assert.equal(await page.evaluate(isWholeShelf), true, "the shelf stays outside the overflowing notice region");
      await search.evaluate((node) => { node.focus({ preventScroll: true }); node.setSelectionRange(2, 5); });
      const selection = () => search.evaluate((node) => ({ focused: document.activeElement === node, range: [node.selectionStart, node.selectionEnd],
        references: [...document.querySelectorAll('[role=option][data-citation-selected]')].map((row) => row.id) }));
      const chosen = await selection();
      const owners = () => page.evaluate(() => [document.querySelector("main"), ...document.querySelectorAll("main *")]
        .filter((node) => /auto|scroll/.test(getComputedStyle(node).overflowY) && node.clientHeight > 0 && node.scrollHeight > node.clientHeight + 1)
        .map((node) => node.className));
      await checkScan("large notice with selection");
      assert.deepEqual(await owners(), ["picker-content scroll-area"], "large notices and results share the middle scroller");
      for (const height of [360, 480, 800, 480]) {
        await page.setViewportSize({ width, height });
        await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        assert.deepEqual(await selection(), chosen, "layout changes preserve focus, query range and selected references");
        {
          await checkScan("growing the viewport restores the bounded middle");
          assert.deepEqual(await owners(), ["picker-content scroll-area"], "notice fallback survives viewport growth");
        }
      }
      for (const control of [page.getByRole("list", { name: "Copy warnings" }).locator("li").first(), page.getByRole("list", { name: "Copy warnings" }).locator("li").last(), page.getByRole("region", { name: "Preview" })]) {
        assert.equal(await reachable(control), true, "notice ends and the fixed preview remain reachable");
        await checkScan("revealing a notice keeps all three bands whole");
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
      await checkScan("dismissed notice leaves one scroller");
      assert.deepEqual(resizes, [], "no native size requests from content");
      assert.deepEqual(errors, [], "no page errors");
      await page.close(); profiles += 1; continue;
    }
    if (small) {
      await page.locator(".shelf .preview-text").filter({ hasText: /^\[@synthetic0\]$/ }).waitFor();
      await waitForCheck("results-announced", () => document.querySelector('[aria-label="Search results"]').textContent.trim().length > 0);
      for (const nextHeight of [480, 360]) {
        await page.setViewportSize({ width, height: nextHeight });
        await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        await checkScan("dragging preserves the bounded middle and whole search/footer");
      }
    }
    assert.equal(await reachable(page.getByRole("region", { name: "Preview" })), true, "preview is whole in its fixed band");
    let editor = await openEditor();
    for (const control of [page.getByLabel("Page", { exact: true }), page.getByRole("radiogroup", { name: "Citation form" })]) {
      assert.equal(await reachable(control), true, "Page/Form remain reachable in the shared overlay");
      await checkScan("details keep the fixed bands whole and one active scroller");
    }
    await closeEditor(editor);
    await search.press("Shift+Enter");
    await page.getByRole("region", { name: "Preview" }).waitFor();
    await page.setViewportSize({ width: 720, height: Math.max(height, 480) });
    editor = await openEditor(true);
    await page.getByLabel("Page", { exact: true }).fill("12");
    await page.setViewportSize({ width: 480, height: 360 });
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await checkScan("resizing while editing keeps fixed bands and one active scroller");
    const anchor = shelfControl(page, /^Edit /).first();
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
    await checkScan("shelf");
    await search.fill("refuse");
    await page.getByRole("alert").and(page.getByText(noticeText.failedSearch, { exact: true })).first().waitFor();
    await checkScan("failed search retains footer and one scroller");
    // Consumer oracle must reject a genuinely clipped frame, not only read CSS.
    await page.evaluate(() => { document.querySelector("main").style.height = "1px"; });
    assert.ok((await scan()).includes("footer action clipped"), `${caseLabel}: same geometry oracle detects clipped footer`);
    assert.deepEqual(resizes, [], "no native size requests from content");
    assert.deepEqual(errors, [], "no page errors");
    await page.close(); profiles += 1;
  }
  console.log(`Picker layout: ${profiles} browser profiles passed (default/minimum, normal/Larger/200%, light/dark, setup/loading/recovery, notices and editing).`);
} finally { await browser.close(); }
