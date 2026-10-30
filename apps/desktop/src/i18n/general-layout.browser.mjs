// Run against `pnpm build:pseudo` served by `pnpm exec vite preview`.
// IBIS_PLAYWRIGHT_MODULE may point to an existing Playwright installation.
// Invented IPC replies exercise the real built Settings entrypoint, not a clone.
import assert from "node:assert/strict";
const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE || "playwright");
const base = new URL(process.argv[2] || "http://127.0.0.1:4173");
assert.ok(["127.0.0.1", "localhost"].includes(base.hostname), "Use a loopback fixture server");
const browser = await chromium.launch({ headless: true });
let profiles = 0;
try {
  for (const [profile, width, height, scale] of [["normal", 720, 560, 1], ["small", 480, 400, 1], ["Larger", 480, 400, 1.15]]) {
    for (const granted of [false, true]) {
      const page = await browser.newPage({ viewport: { width, height } });
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.addInitScript(({ granted, scale }) => {
        let preferences = { citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true, search_mark: false, paste: true };
        let callbackId = 0;
        window.__TAURI_INTERNALS__ = {
          metadata: { currentWindow: { label: "settings" }, currentWebview: { label: "settings" } },
          transformCallback: () => ++callbackId,
          unregisterCallback: () => {},
          invoke: async (command, args) => {
            if (command === "picker_preferences") return preferences;
            if (command === "set_picker_preferences") {
              preferences = { ...preferences, ...args.preferences };
              return { preferences, copy_format: "pandoc" };
            }
            if (command === "login_item") return { name: "Synthetic App", enabled: false, problem: "refused" };
            if (command === "paste_permission") return { pastes: true, needed: true, granted, list: "Synthetic List" };
            if (command === "update_checks") return false;
            if (command === "shortcut_status") return { shortcut: "Ctrl+Shift+Y", registered: true, error: null };
            if (command === "plugin:app|version") return "0.0.0";
            if (command === "plugin:event|listen") return ++callbackId;
            if (command === "plugin:event|unlisten") return;
            throw new Error(`Unimplemented synthetic IPC: ${command}`);
          },
        };
        document.addEventListener("DOMContentLoaded", () => { document.documentElement.style.fontSize = `${16 * scale}px`; });
      }, { granted, scale });
      await page.goto(new URL("settings.html", base).href);
      await page.locator("#search-mark").waitFor();
      const result = await page.evaluate(() => {
        const scan = () => {
          const failures = [];
          const shell = document.querySelector(".settings");
          const pane = document.querySelector(".settings-page");
          if (document.documentElement.scrollWidth > innerWidth || shell.getBoundingClientRect().width > innerWidth + 1) failures.push("horizontal viewport overflow");
          if (pane.scrollWidth > pane.clientWidth + 1) failures.push("horizontal page overflow");
          const walker = document.createTreeWalker(shell, NodeFilter.SHOW_TEXT);
          let node;
          while ((node = walker.nextNode())) {
            const value = node.textContent.trim();
            if (!value) continue;
            if (!value.includes("⟦") && value !== "Synthetic App") failures.push(`untranslated: ${value}`);
            const card = node.parentElement.closest(".ui-settings-card, .ui-side-navigation-item");
            if (!card) continue;
            const bounds = card.getBoundingClientRect();
            const range = document.createRange();
            range.selectNodeContents(node);
            for (const rect of range.getClientRects()) {
              if (rect.left < bounds.left - 1 || rect.right > bounds.right + 1 || rect.top < bounds.top - 1 || rect.bottom > bounds.bottom + 1) failures.push(`clipped: ${value}`);
              for (let ancestor = node.parentElement; ancestor; ancestor = ancestor.parentElement) {
                const style = getComputedStyle(ancestor), clip = ancestor.getBoundingClientRect();
                if ([style.overflowX, style.overflowY].some((overflow) => /auto|scroll/.test(overflow))) break;
                if ((/hidden|clip/.test(style.overflowX) && (rect.left < clip.left - 1 || rect.right > clip.right + 1))
                  || (/hidden|clip/.test(style.overflowY) && (rect.top < clip.top - 1 || rect.bottom > clip.bottom + 1))) failures.push(`clipped ancestor: ${value}`);
              }
            }
          }
          // The page can reach its last control without moving the titlebar/sidebar.
          pane.scrollTop = pane.scrollHeight;
          const control = document.querySelector('[aria-labelledby="search-mark"]');
          const bounds = control.getBoundingClientRect(), visible = pane.getBoundingClientRect();
          if (bounds.bottom > visible.bottom + 1 || bounds.top < visible.top - 1 || shell.getBoundingClientRect().top !== 0 || document.documentElement.scrollTop !== 0) failures.push("last control unreachable or shell moved");
          return failures;
        };
        const issues = scan();
        // Prove the oracle detects both missed localization and real clipping.
        const sentinel = document.createElement("p");
        sentinel.textContent = "Untranslated fixture sentinel";
        document.querySelector(".settings-page").append(sentinel);
        const detectsUntranslated = scan().some((issue) => issue === "untranslated: Untranslated fixture sentinel");
        sentinel.remove();
        const label = document.querySelector(".ui-settings-row-text");
        label.style.height = "1px";
        label.style.overflow = "hidden";
        const detectsClipping = scan().some((issue) => issue.startsWith("clipped ancestor:"));
        return { issues, detectsUntranslated, detectsClipping };
      });
      assert.deepEqual(errors, [], `${profile}/${granted}: page errors`);
      assert.deepEqual(result.issues, [], `${profile}/${granted}: labels must be translated, wrapped and reachable`);
      assert.equal(result.detectsUntranslated, true, "oracle detects untranslated labels");
      assert.equal(result.detectsClipping, true, "oracle detects clipped labels");
      await page.close();
      profiles += 1;
    }
  }
  console.log(`General pseudolocale: ${profiles} browser profiles passed (normal/small/Larger, permission denied/granted).`);
} finally { await browser.close(); }
