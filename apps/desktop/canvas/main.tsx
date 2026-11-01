import { useEffect } from "react";
import { createRoot } from "react-dom/client";
import { App, type PickerApi } from "../src/App";
import { Button } from "../src/ui/Button";
import { Switch } from "../src/ui/Switch";
import { KeyCaps } from "../src/ui/KeyCaps";
import { SettingsGroup, SettingsRow } from "../src/ui/SettingsGroup";
import { useAppearance } from "../src/ui/appearance";
import { englishMessages } from "../src/i18n";
import type { LibraryStatus, SearchResult } from "../src/bindings";
import { setPlatform } from "../src/nativeAppearance";
import { SettingsApp } from "../src/settings/SettingsApp";
import { demoApi } from "../src/settings/demo";
import type { SettingsApi } from "../src/settings/api";
import "../src/tokens.css";
import "../src/ui/Foundation.css";
// Picker root rules must not leak into the separate Settings window.
import pickerStyles from "../src/styles.css?inline";

export const scenarios = ["results", "first-run", "more", "format-menu", "states", "empty", "error", "loading", "settings-general", "settings-look", "settings-citations", "settings-shortcuts", "settings-about"] as const;
type Scenario = typeof scenarios[number];
type Theme = "light" | "dark";

// Same invented records as picker-layout.browser.mjs; that executable browser
// probe cannot be imported without launching Playwright and a local server.
const rows: SearchResult[] = Array.from({ length: 14 }, (_, i) => ({
  revision: "1", id: { library: "invented.bib", key: `synthetic${i}` }, cite: `synthetic-cite-${i}`,
  source: "invented.bib", key: `synthetic${i}`, title: `Invented reference ${i}`,
  authors: "Aster", first_author: "Aster", author_count: 1, year: 2026, container: null,
  item_type: "article-journal", partial: false, matched: ["title"],
}));

function pickerApi(scenario: Scenario, settings: SettingsApi, openSettings: (page: string | null) => Promise<void>): PickerApi {
  const firstRun = scenario === "first-run";
  const fixture = ["empty", "error", "loading"].includes(scenario);
  const library: LibraryStatus = {
    state: firstRun ? "none" : scenario === "loading" ? "loading" : scenario === "error" ? "error" : "loaded", revision: "1", has_snapshot: !firstRun && !["loading", "error"].includes(scenario),
    zotero: false, snapshot_zotero: false, zotero_libraries: [],
    name: firstRun ? null : "invented.bib", snapshot_name: firstRun ? null : "invented.bib",
    references: firstRun || fixture ? 0 : rows.length, source_counts: firstRun ? {} : { "invented.bib": rows.length },
    warnings: 0, error: scenario === "error" ? "The invented library could not be opened." : null, problem: scenario === "error" ? "other" : null, note: null, settings_notice: null,
    stale: false, updated_at: null, checked_at: null,
  };
  const noop = async () => {};
  const subscribe = async () => () => {};
  let sequence = 0;
  const receipts = new Map<string, string>();
  return {
    libraryStatus: async () => library,
    search: async (query) => ({ revision: "1", results: query && !fixture ? rows : [] }),
    previewFileCitation: async (items, _format, style) => {
      const ticket = `synthetic-ticket-${++sequence}`;
      const spans = items.flatMap((item, index) => [
        ...(index ? [{ item: null, text: "; " }] : []),
        { item: index, text: style ? "(Aster, 2026)" : `@${(item.id as { key: string }).key}${item.locator ? `, p. ${item.locator.value}` : ""}` },
      ]);
      if (!style) { spans.unshift({ item: null, text: "[" }); spans.push({ item: null, text: "]" }); }
      const text = spans.map((span) => span.text).join("");
      receipts.set(ticket, text);
      return { ticket, content_hash: ticket, text, spans, html: null, warnings: [] };
    },
    copyFileCitation: async (ticket) => {
      const text = receipts.get(ticket);
      if (text === undefined) throw new Error("Unknown synthetic preview");
      return { text, warnings: [] };
    },
    copyFormat: async () => (await settings.pickerPreferences()).copy_format ?? "pandoc",
    pickerPreferences: settings.pickerPreferences, setPickerPreferences: settings.setPickerPreferences,
    shortcutStatus: settings.shortcutStatus, onShortcutStatus: settings.onShortcutStatus,
    onPreferencesChanged: async (changed) => settings.onPreferencesChanged(() => changed()),
    pickerPresentation: async () => 0, pickerVisible: async () => true,
    hidePicker: async () => true, setPinned: noop, quit: noop,
    // No chooser, clipboard, native settings, files or discovery on the canvas.
    chooseLibrary: async () => { throw new Error("File chooser unavailable on canvas"); },
    useZotero: async () => { throw new Error("Zotero unavailable on canvas"); },
    useSampleLibrary: async () => { throw new Error("Use the results scenario for invented references"); },
    findSources: async () => ({ zotero: "absent", list: 1, files: [], cut_short: false }),
    openFoundLibrary: async () => null,
    reloadLibrary: noop, openLibrarySource: async () => null, openSettings,
    onLibraryChanged: subscribe, onPickerShown: subscribe, onPickerHidden: subscribe,
  };
}

function States({ theme }: { theme: Theme }) {
  useAppearance({ appearance: theme });
  return <SettingsGroup id="canvas-states">{(["hovered", "pressed", "focus-visible"] as const).map((state, index) => <SettingsRow key={state} id={`canvas-${state}`} label={englishMessages.text((["canvas-hovered", "canvas-pressed", "canvas-focused"] as const)[index]!)}><Switch aria-label={englishMessages.text((["canvas-hovered", "canvas-pressed", "canvas-focused"] as const)[index]!)} isSelected={index === 1} onChange={() => {}} {...{ [`data-${state}`]: true }} /><Button variant={index === 1 ? "primary" : index === 2 ? "quiet" : "secondary"} render={(native) => <button {...native} {...{ [`data-${state}`]: true }} />}><KeyCaps accelerator="CmdOrCtrl+K" /></Button></SettingsRow>)}</SettingsGroup>;
}

function Canvas({ host, scenario, settings, picker, theme }: { host: HTMLElement; scenario: Scenario; settings: SettingsApi; picker: PickerApi; theme: Theme }) {
  useEffect(() => {
    if (scenario !== "more" && scenario !== "format-menu") return;
    // Open the real control once it is enabled; no polling or stale completion.
    const selector = scenario === "more" ? 'button[aria-label="More"]' : 'button[aria-label^="Copy as"]';
    const open = () => {
      const button = host.querySelector<HTMLButtonElement>(selector);
      if (!button || button.disabled || document.documentElement.hasAttribute("data-picker-hidden")) return;
      observer.disconnect();
      button.click();
    };
    const observer = new MutationObserver(open);
    observer.observe(host, { childList: true, subtree: true, attributes: true });
    open();
    return () => observer.disconnect();
  }, [host, scenario]);
  if (scenario === "states") return <States theme={theme} />;
  return scenario.startsWith("settings-")
    ? <SettingsApp api={settings} initialPage={scenario.slice(9)} />
    : <><style>{pickerStyles}</style><App api={picker} preview={{ query: ["first-run", "empty", "error", "loading"].includes(scenario) ? "" : "invented" }} /></>;
}

const mounts = new WeakMap<HTMLElement, () => void>();

/** One board per iframe/document, as in the app's separate native windows.
 * Load ibis-canvas.css and ibis-canvas.js, then mount or use data-ibis-canvas.
 * URL: ?scenario=settings-look&theme=dark; attributes override the URL. */
export function mount(host: HTMLElement, options: { scenario?: string; theme?: string } = {}): () => void {
  const params = new URLSearchParams(window.location.search);
  const scenario = options.scenario ?? (host.dataset.ibisCanvas || params.get("scenario") || "results");
  const theme = options.theme ?? host.dataset.theme ?? params.get("theme") ?? "light";
  if (!scenarios.includes(scenario as Scenario)) throw new Error(`Unknown canvas scenario: ${scenario}`);
  if (theme !== "light" && theme !== "dark") throw new Error(`Unknown canvas theme: ${theme}`);
  mounts.get(host)?.();
  setPlatform();
  const settings = demoApi();
  void settings.setPickerPreferences({ appearance: theme as Theme, paste: false }, null, 0);
  const settingsWindow = scenario.startsWith("settings-");
  document.documentElement.classList.toggle("settings-window", settingsWindow);
  document.documentElement.dataset.nativeAppearance = "opaque";
  host.id = settingsWindow ? "settings" : "root";
  const root = createRoot(host);
  const picker = pickerApi(scenario as Scenario, settings, async (page) => {
    mount(host, { scenario: `settings-${page ?? "general"}`, theme });
  });
  root.render(<Canvas host={host} scenario={scenario as Scenario} settings={settings} picker={picker} theme={theme as Theme} />);
  const unmount = () => {
    if (mounts.get(host) !== unmount) return;
    mounts.delete(host);
    root.unmount();
    document.documentElement.classList.remove("settings-window");
  };
  mounts.set(host, unmount);
  return unmount;
}

const host = document.querySelector<HTMLElement>("[data-ibis-canvas]");
if (host) mount(host);
