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
  for (const [profile, width, height, larger] of [
    ["default", 920, 560, false], ["minimum", 800, 400, false],
    ["default/Larger", 920, 560, true], ["minimum/Larger", 800, 400, true],
  ]) {
    for (const panel of ["general-denied", "general-granted", "general-refusal", "shortcuts", "look-light", "look-dark", "citations", "about", "about-unavailable"]) {
      const granted = panel !== "general-denied";
      const page = await browser.newPage({ viewport: { width, height } });
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.addInitScript(({ granted, larger, panel }) => {
        Object.defineProperty(navigator, "platform", { value: "MacIntel" });
        // Persist across a real reload to exercise close/reopen without native I/O.
        const read = (key, fallback) => JSON.parse(sessionStorage.getItem(key) || "null") ?? fallback;
        let preferences = read("preferences", { citation_style: null, density: "comfortable", larger_text: larger, show_paste_preview: true, search_mark: true, paste: true, appearance: panel === "look-dark" ? "dark" : "light", accent: "system" });
        let shortcut = read("shortcut", { shortcut: "Alt+J", registered: true, error: null });
        let callbackId = 0, revision = 0, shortcutAttempts = 0;
        window.__TAURI_INTERNALS__ = {
          metadata: { currentWindow: { label: "settings" }, currentWebview: { label: "settings" } },
          transformCallback: () => ++callbackId,
          unregisterCallback: () => {},
          invoke: async (command, args) => {
            if (command === "picker_preferences") return preferences;
            if (command === "preference_revision") return ++revision;
            if (command === "set_picker_preferences") {
              preferences = { ...preferences, ...args.preferences };
              sessionStorage.setItem("preferences", JSON.stringify(preferences));
              return { preferences, copy_format: "pandoc" };
            }
            if (command === "login_item") return { name: "Synthetic App", enabled: false, problem: "refused" };
            if (command === "paste_permission") return { pastes: true, needed: true, granted, list: "Synthetic List" };
            if (command === "update_checks") return panel === "about-unavailable" ? null : false;
            if (command === "shortcut_status") return shortcut;
            if (command === "set_shortcut") {
              if (++shortcutAttempts === 1) throw "in-use";
              shortcut = { shortcut: args.shortcut ?? "CmdOrCtrl+Shift+Y", registered: true, error: null };
              sessionStorage.setItem("shortcut", JSON.stringify(shortcut));
              return shortcut;
            }
            if (command === "plugin:app|version") return "0.0.0";
            if (command === "plugin:event|listen") return ++callbackId;
            if (command === "plugin:event|unlisten") return;
            throw new Error(`Unimplemented synthetic IPC: ${command}`);
          },
        };
      }, { granted, larger, panel });
      const target = panel.startsWith("look") ? "look" : panel.startsWith("about") ? "about" : panel === "shortcuts" ? "shortcuts" : panel === "citations" ? "citations" : "general";
      const url = new URL("settings.html", base);
      url.searchParams.set("page", target);
      await page.goto(url.href);
      await page.locator(".ui-settings-row").first().waitFor();
      if (panel === "shortcuts" || panel === "general-refusal") {
        const recorder = page.locator(".settings-page button[aria-pressed]");
        await recorder.waitFor();
        const saved = await recorder.textContent();
        await recorder.click();
        await recorder.press("Alt+K");
        await page.getByRole("alert").waitFor();
        assert.equal(await recorder.textContent(), saved, "refusal retains working shortcut");
        assert.equal(await page.locator(".ui-settings-row:has([role=alert]) button[aria-pressed]").count(), 1, "refusal beside its recorder");
        // Success and reopening go through the same General/Shortcuts owner.
        await recorder.click();
        await recorder.press("Alt+L");
        await page.waitForFunction(() => JSON.parse(sessionStorage.getItem("shortcut"))?.shortcut === "Alt+L");
        await page.reload();
        await recorder.waitFor();
        assert.notEqual(await recorder.textContent(), saved, "saved shortcut reopens");
        await recorder.click(); // The longer recording prompt is included in layout checks.
      } else if (panel.startsWith("look")) {
        const accent = page.locator('.ui-picture-choice:has(.ui-accent-dot[data-accent="ibis"])');
        await accent.click();
        await page.waitForFunction(() => JSON.parse(sessionStorage.getItem("preferences"))?.accent === "ibis");
        await page.reload();
        await page.locator('html[data-accent="ibis"]').waitFor();
        assert.equal(await accent.getAttribute("data-selected"), "true", "Ibis accent reopens selected");
      }
      const result = await page.evaluate(() => {
        const scan = () => {
          const failures = [];
          const shell = document.querySelector(".settings");
          const pane = document.querySelector(".settings-page");
          const paneBounds = pane.getBoundingClientRect(), paneStyle = getComputedStyle(pane);
          if (document.documentElement.scrollWidth > innerWidth || shell.getBoundingClientRect().width > innerWidth + 1) failures.push("horizontal viewport overflow");
          if (pane.scrollWidth > pane.clientWidth + 1) failures.push("horizontal page overflow");
          if (shell.querySelector("details, summary")) failures.push("native disclosure");
          if (/Coming soon/.test(shell.textContent)) failures.push("unfinished setting");
          if (paneStyle.scrollbarGutter !== "stable") failures.push("scrollbar gutter missing");
          pane.scrollTop = 0;
          const title = pane.querySelector("h1").getBoundingClientRect();
          if (Math.abs(title.top - paneBounds.top - parseFloat(paneStyle.paddingTop)) > 1) failures.push("extra gap above page title");
          let previous = pane.querySelector("h1");
          for (const group of pane.querySelectorAll(":scope > .ui-settings-group")) {
            if (group.getBoundingClientRect().top - previous.getBoundingClientRect().bottom < parseFloat(paneStyle.rowGap) - 1) failures.push("sections touching");
            previous = group;
          }
          const pictures = [...pane.querySelectorAll(".ui-picture-choices:not([data-dots])")];
          for (const choices of pictures) {
            const cards = [...choices.children].map((card) => card.getBoundingClientRect());
            if (cards.some((card) => Math.abs(card.top - cards[0].top) > 1)) failures.push("picture choices wrapped");
          }
          const walker = document.createTreeWalker(shell, NodeFilter.SHOW_TEXT);
          let node;
          while ((node = walker.nextNode())) {
            const value = node.textContent.trim();
            if (!value || node.parentElement.closest(".ui-visually-hidden")) continue;
            // Chords keep their platform formatter; IPC app names remain literal.
            const native = ["Synthetic App", "⌥J", "⌥L", "⇧⌘Y", "Ctrl+Shift+Y"];
            if (!value.includes("⟦") && !native.includes(value) && !node.parentElement.closest("kbd")) failures.push(`untranslated: ${value}`);
            const card = node.parentElement.closest(".ui-settings-card, .ui-side-navigation-item");
            if (!card) continue;
            const bounds = card.getBoundingClientRect();
            const range = document.createRange();
            range.selectNodeContents(node);
            for (const rect of range.getClientRects()) {
              if (rect.left < bounds.left - 1 || rect.right > bounds.right + 1 || rect.top < bounds.top - 1 || rect.bottom > bounds.bottom + 1) failures.push(`clipped: ${value}`);
              if (card.closest(".settings-page") && (rect.right > paneBounds.left + pane.clientWidth - parseFloat(paneStyle.paddingRight) + 1)) failures.push(`scrollbar overlap: ${value}`);
              for (let ancestor = node.parentElement; ancestor; ancestor = ancestor.parentElement) {
                const style = getComputedStyle(ancestor), clip = ancestor.getBoundingClientRect();
                if ([style.overflowX, style.overflowY].some((overflow) => /auto|scroll/.test(overflow))) break;
                if ((/hidden|clip/.test(style.overflowX) && (rect.left < clip.left - 1 || rect.right > clip.right + 1))
                  || (/hidden|clip/.test(style.overflowY) && (rect.top < clip.top - 1 || rect.bottom > clip.bottom + 1))) failures.push(`clipped ancestor: ${value}`);
              }
            }
          }
          for (const row of pane.querySelectorAll(".ui-settings-row")) {
            const bounds = row.getBoundingClientRect(), style = getComputedStyle(row);
            for (const child of row.children) {
              const box = child.getBoundingClientRect();
              if (box.top < bounds.top + parseFloat(style.paddingTop) - 1 || box.bottom > bounds.bottom - parseFloat(style.paddingBottom) + 1) failures.push("row loses wrapped text padding");
            }
          }
          // The page can reach its last row without moving the titlebar/sidebar.
          pane.scrollTop = pane.scrollHeight;
          const rows = pane.querySelectorAll(".ui-settings-row");
          const last = rows[rows.length - 1].getBoundingClientRect();
          if (last.bottom > paneBounds.bottom - parseFloat(paneStyle.paddingBottom) + 1 || shell.getBoundingClientRect().top !== 0 || document.documentElement.scrollTop !== 0) failures.push("last control unreachable or shell moved");
          return failures;
        };
        const issues = scan();
        // Resolve CSS colour functions through canvas to test rendered accent roles.
        if (document.documentElement.dataset.accent === "ibis") {
          const canvas = document.createElement("canvas");
          canvas.width = canvas.height = 1;
          const ctx = canvas.getContext("2d", { willReadFrequently: true });
          const rgb = (css) => { ctx.clearRect(0, 0, 1, 1); ctx.fillStyle = css; ctx.fillRect(0, 0, 1, 1); return [...ctx.getImageData(0, 0, 1, 1).data].slice(0, 3); };
          const luminance = (channels) => channels.map((channel) => {
            const n = channel / 255;
            return n <= .04045 ? n / 12.92 : ((n + .055) / 1.055) ** 2.4;
          }).reduce((sum, n, i) => sum + n * [.2126, .7152, .0722][i], 0);
          const ratio = (a, b) => { const x = luminance(rgb(a)), y = luminance(rgb(b)); return (Math.max(x, y) + .05) / (Math.min(x, y) + .05); };
          const probe = document.createElement("button");
          probe.className = "ui-button";
          probe.dataset.variant = "primary";
          probe.textContent = "⟦Contrast probe⟧";
          document.querySelector(".ui-settings-card").append(probe);
          const surface = getComputedStyle(probe.parentElement).backgroundColor;
          for (const state of ["normal", "hovered", "pressed"]) {
            delete probe.dataset.hovered; delete probe.dataset.pressed;
            if (state !== "normal") probe.dataset[state] = "true";
            const style = getComputedStyle(probe);
            if (ratio(style.backgroundColor, style.color) < 4.5) issues.push(`accent text contrast: ${state}`);
            if (ratio(style.backgroundColor, surface) < 3) issues.push(`accent control contrast: ${state}`);
          }
          const selected = document.querySelector('.ui-side-navigation-item[aria-current="page"]');
          const selection = getComputedStyle(selected);
          if (ratio(selection.backgroundColor, selection.color) < 4.5) issues.push("selected text contrast");
          for (const toggle of document.querySelectorAll(".ui-switch[data-selected]")) {
            const track = getComputedStyle(toggle.querySelector(".ui-switch-track"));
            const thumb = getComputedStyle(toggle.querySelector(".ui-switch-thumb"));
            if (ratio(track.backgroundColor, thumb.backgroundColor) < 3) issues.push("switch thumb contrast");
          }
          probe.remove();
        }
        // Prove the oracle detects both missed localization and real clipping.
        const sentinel = document.createElement("p");
        sentinel.textContent = "Untranslated fixture sentinel";
        document.querySelector(".settings-page").append(sentinel);
        const detectsUntranslated = scan().includes("untranslated: Untranslated fixture sentinel");
        sentinel.remove();
        const label = document.querySelector(".ui-settings-row-text");
        label.style.height = "1px";
        label.style.overflow = "hidden";
        const detectsClipping = scan().some((issue) => issue.startsWith("clipped ancestor:"));
        return { issues, detectsUntranslated, detectsClipping };
      });
      assert.deepEqual(errors, [], `${profile}/${panel}: page errors`);
      assert.deepEqual(result.issues, [], `${profile}/${panel}: labels, layout and contrast`);
      assert.equal(result.detectsUntranslated, true, "oracle detects untranslated labels");
      assert.equal(result.detectsClipping, true, "oracle detects clipped labels");
      await page.close();
      profiles += 1;
    }
  }
  console.log(`Settings pseudolocale: ${profiles} browser profiles passed (default/minimum, Default/Larger; General, Shortcuts, Look, Citations and About).`);
} finally { await browser.close(); }
