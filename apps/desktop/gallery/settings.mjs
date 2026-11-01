// Screenshots every Settings page in light and dark from a served build, with invented data.
// pnpm -C apps/desktop exec vite preview --port 4173
// IBIS_PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs node apps/desktop/gallery/settings.mjs http://127.0.0.1:4173 OUT_DIR
const { chromium } = await import(process.env.IBIS_PLAYWRIGHT_MODULE);
const base = new URL(process.argv[2]); const out = process.argv[3];
const browser = await chromium.launch({ headless: true });
for (const theme of ["light", "dark"]) for (const panel of ["general", "look", "citations", "shortcuts", "about"]) {
  const granted = true, larger = false;
  const page = await browser.newPage({ viewport: { width: 920, height: 640 }, colorScheme: theme, deviceScaleFactor: 2 });
      await page.addInitScript(({ granted, larger, panel }) => {
        Object.defineProperty(navigator, "platform", { value: "MacIntel" });
        // Persist across a real reload to exercise close/reopen without native I/O.
        const read = (key, fallback) => JSON.parse(sessionStorage.getItem(key) || "null") ?? fallback;
        let preferences = read("preferences", { citation_style: null, density: "comfortable", larger_text: larger, show_paste_preview: true, search_mark: true, paste: true, appearance: "system", accent: "system" });
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

  const url = new URL("settings.html", base);
  url.searchParams.set("page", panel);
  await page.goto(url.href);
  await page.waitForTimeout(500);
  await page.screenshot({ path: `${out}/${theme}-${panel}.png` });
  await page.close();
}
await browser.close();
