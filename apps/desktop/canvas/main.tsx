import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { App, type PickerApi } from "../src/App";
import { Button } from "../src/ui/Button";
import { KeyCaps } from "../src/ui/KeyCaps";
import { SettingsGroup, SettingsRow } from "../src/ui/SettingsGroup";
import { useAppearance } from "../src/ui/appearance";
import { englishMessages } from "../src/i18n";
import type { LibraryStatus, SearchResult } from "../src/bindings";
import { isStyle } from "../src/format/outputs";
import { setPlatform } from "../src/nativeAppearance";
export { setNativeAccent } from "../src/nativeAccent";
export { geometry } from "./geometry";
import { SettingsApp } from "../src/settings/SettingsApp";
import { demoApi } from "../src/settings/demo";
import type { SettingsApi } from "../src/settings/api";
import "../src/tokens.css";
import tokenSource from "../src/tokens.css?raw";
import "../src/ui/Foundation.css";
import "./board.css";
import { Parts } from "./parts";
import { WritingProjectsDialog } from "../src/features/picker/WritingProjects";
import { inventedSource, PageSources } from "../src/features/reader/source";
import type { WritingProjectsView } from "../src/api";
import { renderNotice } from "../src/copyFeedback";
import { currentPlatform } from "../src/platform";
import "../src/ui/CopyReceipt.css";
import "../src/notice.css";
// Picker root rules must not leak into the separate Settings window.
import pickerStyles from "../src/styles.css?inline";

export const scenarios = ["notice", "results", "first-run", "more", "format-menu", "writing-projects", "writing-projects-pending", "project-check-conflict", "states", "parts", "empty", "file-drag", "error", "loading", "empty-search", "shelf", "reference-details", "show-as-menu", "reader", "commands", "page-in-search", "peek", "library-menu", "shortcuts", "pinned", "no-matches", "missing", "zotero-closed", "unreadable", "stale", "compact", "larger-text", "right-to-left", "long-titles", "update", "undo-notice", "tokens", "settings-general", "settings-look", "settings-citations", "settings-shortcuts", "settings-about"] as const;
type Scenario = typeof scenarios[number];
type Theme = "light" | "dark";

// Same invented records as picker-layout.browser.mjs; that executable browser
// probe cannot be imported without launching Playwright and a local server.
const rows: SearchResult[] = Array.from({ length: 14 }, (_, i) => ({
  revision: "1", id: { library: "invented.bib", key: `synthetic${i}` }, cite: `synthetic-cite-${i}`,
  source: "invented.bib", key: `synthetic${i}`, title: `Invented reference ${i}`,
  authors: "Aster", first_author: "Aster", author_count: 1, year: "2026", container: null,
  item_type: "article-journal", partial: false, matched: ["title"],
}));

const longTitle = "An invented reference whose title runs well past the end of the row so the second line and its ellipsis are visible";
const longRows = rows.map((row) => ({ ...row, title: longTitle, authors: "Aster, Birch, Cedar, Dune, Elm and Fern", container: "The Invented Journal of Very Long Names" }));

/** What differs from the loaded picker. Each entry is real app state: the
 * picture is whatever the app draws for it. */
const fixtures: Partial<Record<Scenario, {
  query?: string; library?: Partial<LibraryStatus>; rows?: SearchResult[]; preferences?: { density?: "compact"; larger_text?: boolean };
  /** Clicked in order, each once the one before has drawn what it opens. */
  click?: string | string[]; keys?: KeyboardEventInit[];
}>> = {
  "empty-search": { query: "" },
  commands: { query: ">" },
  "page-in-search": { query: "invented 417" },
  peek: { keys: [{ key: "y", code: "KeyY", metaKey: true }] },
  shelf: { keys: [{ key: "Enter", shiftKey: true }, { key: "ArrowDown" }, { key: "Enter", shiftKey: true }] },
  "reference-details": { keys: [{ key: "Enter", shiftKey: true }], click: 'button[aria-label^="Edit "]' },
  reader: { keys: [{ key: "Enter", shiftKey: true }], click: ['button[aria-label^="Edit "]', 'button[aria-expanded="false"].inspector-action'] },
  "show-as-menu": { keys: [{ key: "Enter", shiftKey: true }], click: ['button[aria-label^="Edit "]', ".citation-field .ui-popup-button"] },
  "writing-projects-pending": { click: 'button[aria-label="Use Field Notes"]' },
  "library-menu": { click: ".library-bar button" },
  shortcuts: { keys: [{ key: "/", [navigator.platform.startsWith("Mac") ? "metaKey" : "ctrlKey"]: true }] },
  pinned: { click: ".pin-button" },
  "no-matches": { rows: [] },
  missing: { query: "", library: { state: "error", has_snapshot: false, references: 0, problem: "missing", error: "the library file is unavailable; download any online-only file to this computer first" } },
  "zotero-closed": { query: "", library: { state: "error", has_snapshot: false, references: 0, zotero: true, name: "Zotero", problem: "zotero_closed", error: "Zotero’s library cannot be read while it is closed" } },
  unreadable: { query: "", library: { state: "error", has_snapshot: false, references: 0, problem: "unreadable", error: "the library file does not contain valid reference data" } },
  stale: { library: { stale: true, zotero: true, snapshot_zotero: true, name: "Zotero", snapshot_name: "Zotero", error: "Zotero’s library cannot be read while it is closed", problem: "zotero_closed", checked_at: Date.now() - 2 * 3_600_000 } },
  compact: { preferences: { density: "compact" } },
  "larger-text": { preferences: { larger_text: true } },
  "right-to-left": {},
  "long-titles": { rows: longRows },
};

function pickerApi(scenario: Scenario, settings: SettingsApi, openSettings: (page: string | null) => Promise<void>): PickerApi {
  const firstRun = scenario === "first-run";
  const fixture = ["empty", "file-drag", "error", "loading"].includes(scenario);
  const fixed = fixtures[scenario];
  const shown = fixed?.rows ?? rows;
  const projectContext = scenario.startsWith("project-");
  const project = writingProjects.active!;
  const refusal = scenario === "project-check-conflict" ? "A key in thesis.bib identifies a different work. Resolve the conflict before citing it." : null;
  const library: LibraryStatus = {
    source_generation: "1", source_is_requested: true,
    state: firstRun ? "none" : scenario === "loading" ? "loading" : scenario === "error" ? "error" : "loaded", revision: "1", has_snapshot: !firstRun && !["loading", "error"].includes(scenario),
    zotero: false, snapshot_zotero: false, zotero_libraries: [],
    name: firstRun ? null : "invented.bib", snapshot_name: firstRun ? null : "invented.bib",
    references: firstRun || fixture ? 0 : rows.length, source_counts: firstRun ? {} : { "invented.bib": rows.length },
    warnings: 0, reading: [], live_updates_off: false, error: scenario === "error" ? "the chosen file is not on this computer" : null, problem: scenario === "error" ? "other" : null, note: null, settings_notice: null,
    // The backend's own words and times, so each picture shows what a person would see.
    stale: false, updated_at: null, checked_at: firstRun ? null : Date.now() - 3 * 60_000, ...fixed?.library,
  };
  const noop = async () => {};
  const subscribe = async () => () => {};
  let sequence = 0;
  const receipts = new Map<string, string>();
  return {
    libraryStatus: async () => library,
    // As the engine does: a trailing page number is read as the page ("invented 417").
    search: async (query, _limit, _scope, interpret) => {
      const page = interpret !== false ? /^(.*\S)\s+(?:pp?\.\s*)?(\d+(?:[-–]\d+)?)$/.exec(query) : null;
      return { revision: "1", results: query && !fixture ? shown : [],
        ...page ? { interpretation: { search: page[1]!, locator: { label: "page" as const, value: page[2]!.replace("-", "–") }, typed: page[2]! } } : {} };
    },
    previewFileCitation: async (items, output) => {
      const style = isStyle(output);
      const ticket = `synthetic-ticket-${++sequence}`;
      const spans = items.flatMap((item, index) => [
        ...(index ? [{ item: null, text: "; " }] : []),
        { item: index, text: style ? "(Aster, 2026)" : `@${(item.id as { key: string }).key}${item.locator ? `, p. ${item.locator.value}` : ""}` },
      ]);
      if (!style) { spans.unshift({ item: null, text: "[" }); spans.push({ item: null, text: "]" }); }
      const text = spans.map((span) => span.text).join("");
      receipts.set(ticket, text);
      return { ticket, content_hash: ticket, text, spans, html: null, warnings: [], project_refusal: refusal };
    },
    copyFileCitation: async (ticket) => {
      const text = receipts.get(ticket);
      if (text === undefined) throw new Error("Unknown synthetic preview");
      return { text, warnings: [] };
    },
    pickerDestination: projectContext ? async () => ({ app: "Invented Editor", app_id: "invented.editor", app_identity: null,
      automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null, paste: null, paste_question: false, can_choose_paste: false,
      writing_project: project, writing_context: "invented-opening", context_version: [1, 0] }) : undefined,
    rememberAppOutput: projectContext ? async () => null : undefined,
    writingProjects: projectContext ? async () => writingProjects : undefined,
    editWritingProject: projectContext ? async () => writingProjects : undefined,
    retryProject: projectContext ? noop : undefined,
    chooseProject: projectContext ? noop : undefined,
    pickerPreferences: settings.pickerPreferences, savePickerPreferences: settings.savePickerPreferences, pickerPreferencesSaved: settings.pickerPreferencesSaved,
    shortcutStatus: settings.shortcutStatus, onShortcutStatus: settings.onShortcutStatus,
    onPreferencesChanged: async (changed) => settings.onPreferencesChanged(() => changed(null)),
    pickerPresentation: async () => 0, pickerVisible: async () => true,
    hidePicker: async () => true, setPinned: noop, quit: noop,
    // No chooser, clipboard, native settings, files or discovery on the canvas.
    chooseLibrary: async () => { throw new Error("File chooser unavailable on canvas"); },
    useZotero: async () => { throw new Error("Zotero unavailable on canvas"); },
    useSampleLibrary: async () => { throw new Error("Use the results scenario for invented references"); },
    findSources: async () => ({ zotero: "absent", list: 1, files: [], cut_short: false }),
    openFoundLibrary: async () => null,
    reloadLibrary: noop, openLibrarySource: async () => null, openSettings,
    onFileDrag: scenario === "file-drag" ? async (change) => { change(true); return () => {}; } : undefined,
    onWindowReset: scenario === "undo-notice" ? async (reset) => { reset({ revision: 1, undo: 1 }); return () => {}; } : undefined,
    readWindowPositionUndo: scenario === "undo-notice" ? async () => 1 : undefined,
    undoWindowPosition: scenario === "undo-notice" ? async () => true : undefined,
    dismissWindowPositionUndo: scenario === "undo-notice" ? noop : undefined,
    onUpdateProgress: scenario === "update" ? async (progress) => { progress({ attempt: 1, revision: 1, received: 3_150_000, total: 7_400_000, phase: "downloading" }); return () => {}; } : undefined,
    onLibraryChanged: subscribe, onPickerShown: subscribe, onPickerHidden: subscribe,
  };
}

const tokensStyle = `
.canvas-tokens { padding: var(--space-6); overflow: auto; height: 100%; box-sizing: border-box; color: var(--text); }
.canvas-tokens h2 { font-size: var(--type-title-size); margin: var(--space-4) 0 var(--space-2); }
.canvas-token-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr)); gap: var(--space-2) var(--space-4); }
.canvas-token-grid span { display: flex; align-items: center; gap: var(--space-2); min-width: 0; min-height: calc(2 * var(--type-body-line)); }
.canvas-token-grid i { flex: none; width: var(--space-6); height: var(--space-6); box-shadow: inset 0 0 0 1px var(--separator); }
/* A long name wraps at its hyphens onto a second line, never inside a word. */
.canvas-token-grid code { min-width: 0; font: var(--type-label-size) / var(--type-label-line) var(--mono); color: var(--text-secondary); overflow-wrap: normal; }
`;

function States({ theme }: { theme: Theme }) {
  useAppearance({ appearance: theme });
  // One control in every row, each state drawn by the part's own attribute through its
  // render prop, so the picture is the real rule.
  const labels = ["canvas-hovered", "canvas-pressed", "canvas-focused"] as const;
  return <div className="canvas-parts"><SettingsGroup id="canvas-states">{(["hovered", "pressed", "focus-visible"] as const).map((state, index) => <SettingsRow key={state} id={`canvas-${state}`} label={englishMessages.text(labels[index]!)}>
    <Button variant="secondary" render={(native) => <button {...native} {...{ [`data-${state}`]: true }} />}>Check Now<KeyCaps accelerator="CmdOrCtrl+K" /></Button>
  </SettingsRow>)}</SettingsGroup></div>;
}

const tokenNames = [...new Set([...tokenSource.matchAll(/(--[a-z][\w-]*):/g)].map((match) => match[1]!))];
const family = (...prefixes: string[]) => tokenNames.filter((name) => prefixes.some((prefix) => name === prefix || name.startsWith(`${prefix}-`)));

/** A type role's size in pixels, as the page resolves it, so the roles list from small to large. */
function typeSize(name: string): number {
  const probe = document.createElement("span");
  probe.style.fontSize = `var(${name})`;
  document.body.append(probe);
  try { return parseFloat(getComputedStyle(probe).fontSize) || 0; } finally { probe.remove(); }
}

/** The tokens as the app resolves them, so this page cannot drift from the code. */
function Tokens({ theme }: { theme: Theme }) {
  useAppearance({ appearance: theme });
  // Only the tokens that resolve to a colour: sizes such as --status-dot and --ring-width share these prefixes.
  // A browser without the resolved value (a DOM without a cascade) keeps them all.
  const resolved = getComputedStyle(document.documentElement);
  const colour = (name: string) => { const value = resolved.getPropertyValue(name).trim(); return !value || typeof CSS === "undefined" || CSS.supports("color", value); };
  const colours = family("--text", "--surface", "--sheet", "--accent", "--selected", "--fill", "--separator", "--scarlet", "--status", "--ring").filter(colour);
  return <div className="canvas-tokens"><style>{tokensStyle}</style>
    <h2>Colour</h2>
    <div className="canvas-token-grid">{colours.map((name) => <span key={name}><i style={{ background: `var(${name})` }} /><code>{name}</code></span>)}</div>
    <h2>Corners</h2>
    <div className="canvas-token-grid">{family("--r").map((name) => <span key={name}><i style={{ borderRadius: `var(${name})` }} data-edge /><code>{name}</code></span>)}</div>
    <h2>Space</h2>
    <div className="canvas-token-grid">{family("--space").map((name) => <span key={name}><i style={{ width: `var(${name})`, height: `var(${name})` }} /><code>{name}</code></span>)}</div>
    <h2>Type</h2>
    {family("--type").filter((name) => name.endsWith("-size")).sort((a, b) => typeSize(a) - typeSize(b)).map((name) => <p key={name} style={{ fontSize: `var(${name})`, lineHeight: `var(${name.replace("-size", "-line")})` }}>{name.slice(7, -5)}: The quiet garden</p>)}
  </div>;
}

function ThemedParts({ theme }: { theme: Theme }) {
  useAppearance({ appearance: theme });
  return <><style>{pickerStyles}</style><Parts /></>;
}

/** The production notice renderer, with an invented citation and no native IPC. */
function Notice({ theme }: { theme: Theme }) {
  const root = useRef<HTMLDivElement>(null);
  useAppearance({ appearance: theme });
  useEffect(() => {
    const element = root.current;
    if (!element) return;
    renderNotice(element, currentPlatform())({ text: "[Aster, 2026]", more: 0,
      delivered: { outcome: "copied", reason: "paste-off" } });
    return () => element.replaceChildren();
  }, []);
  return <div ref={root} />;
}

/** The invented book, the same for every reference: the reader's shell needs no PDF (decision 0038). */
const book = inventedSource();
const invented = () => book;

function Canvas({ host, scenario, settings, picker, theme }: { host: HTMLElement; scenario: Scenario; settings: SettingsApi; picker: PickerApi; theme: Theme }) {
  useEffect(() => {
    const fixed = fixtures[scenario];
    const clicks = scenario === "more" ? ['button[aria-label="More"]'] : scenario === "format-menu" ? ['button[aria-label^="Copy as"]'] : [fixed?.click ?? []].flat();
    const keys = [...fixed?.keys ?? []];
    if (!clicks.length && !keys.length) return;
    // Act on the real controls once they are ready; no polling or stale completion.
    const open = () => {
      const search = host.querySelector<HTMLInputElement>("#search");
      if (keys.length && (!search || !host.querySelector('[role="option"]'))) return;
      // One key per commit: each acts on the state the last one left.
      if (keys.length) { search!.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...keys.shift()! })); return; }
      if (!clicks.length) { observer.disconnect(); return; }
      const button = host.querySelector<HTMLButtonElement>(clicks[0]!);
      if (!button || button.disabled || document.documentElement.hasAttribute("data-picker-hidden")) return;
      clicks.shift();
      if (!clicks.length) observer.disconnect();
      button.click();
    };
    const observer = new MutationObserver(open);
    observer.observe(host, { childList: true, subtree: true, attributes: true });
    open();
    return () => observer.disconnect();
  }, [host, scenario]);
  if (scenario === "notice") return <Notice theme={theme} />;
  if (scenario === "states") return <States theme={theme} />;
  if (scenario === "tokens") return <Tokens theme={theme} />;
  if (scenario === "parts") return <ThemedParts theme={theme} />;
  if (scenario.startsWith("writing-projects")) return <WritingProjectsBoard picker={picker} variant={scenario} />;
  return scenario.startsWith("settings-")
    ? <SettingsApp api={settings} initialPage={scenario.slice(9)} />
    : <PageSources.Provider value={scenario === "reader" ? invented : null}><style>{pickerStyles}</style><App api={picker} preview={{ query: fixtures[scenario]?.query ?? (["first-run", "empty", "file-drag", "error", "loading"].includes(scenario) ? "" : "invented") }} /></PageSources.Provider>;
}

const inventedProject = { id: "thesis", name: "Thesis", output: "autocite", bibliography: "thesis.bib" };
const writingProjects: WritingProjectsView = { context: "invented-opening", session_only: false, selected: "thesis", active: inventedProject, projects: [
  inventedProject,
  { id: "notes", name: "Field Notes", output: "pandoc", bibliography: null },
] };

/** The real project sheet over the real picker, with invented local choices. */
function WritingProjectsBoard({ picker, variant }: { picker: PickerApi; variant: Scenario }) {
  const [open, setOpen] = useState(true);
  const view = writingProjects;
  return <><style>{pickerStyles}</style><App api={picker} preview={{ query: "invented" }} />
    {open && <WritingProjectsDialog format="autocite" onClose={() => setOpen(false)} api={{
      writingProjects: async () => view,
      editWritingProject: async () => variant === "writing-projects-pending" ? new Promise<WritingProjectsView>(() => {}) : view,
      chooseProject: async () => {}, stopProject: async () => {},
    }} />}
  </>;
}

const mounts = new WeakMap<HTMLElement, () => void>();

/** One board per iframe/document, as in the app's separate native windows.
 * Load ibis-canvas.css and ibis-canvas.js, then mount or use data-ibis-canvas.
 * URL: ?scenario=settings-look&theme=dark; attributes override the URL. */
/** The design parameters a canvas board may turn (tokens.css, "Parameters"). */
export const parameters = ["--unit", "--radius", "--type-base", "--ibis", "--ibis-dark"] as const;

export function mount(host: HTMLElement, options: { scenario?: string; theme?: string; params?: Partial<Record<typeof parameters[number], string>> } = {}): () => void {
  const params = new URLSearchParams(window.location.search);
  const scenario = options.scenario ?? (host.dataset.ibisCanvas || params.get("scenario") || "results");
  const theme = options.theme ?? host.dataset.theme ?? params.get("theme") ?? "light";
  if (!scenarios.includes(scenario as Scenario)) throw new Error(`Unknown canvas scenario: ${scenario}`);
  if (theme !== "light" && theme !== "dark") throw new Error(`Unknown canvas theme: ${theme}`);
  mounts.get(host)?.();
  setPlatform();
  // Parameters override tokens.css live; everything derived from them follows.
  for (const name of parameters) {
    const value = options.params?.[name]?.trim();
    if (value) document.documentElement.style.setProperty(name, value);
    else document.documentElement.style.removeProperty(name);
  }
  const settings = demoApi();
  void settings.savePickerPreferences({ ...fixtures[scenario as Scenario]?.preferences, appearance: theme as Theme, paste: false }).then(settings.pickerPreferencesSaved);
  // The states and tokens pages use the Settings window's base styles.
  const settingsWindow = scenario.startsWith("settings-") || scenario === "states" || scenario === "tokens" || scenario === "parts";
  document.documentElement.classList.toggle("settings-window", settingsWindow);
  document.documentElement.classList.toggle("notice-window", scenario === "notice");
  document.documentElement.dataset.nativeAppearance = "opaque";
  document.documentElement.dir = scenario === "right-to-left" ? "rtl" : "ltr";
  host.id = scenario === "notice" ? "notice" : settingsWindow ? "settings" : "root";
  const root = createRoot(host);
  const picker = pickerApi(scenario as Scenario, settings, async (page) => {
    mount(host, { scenario: `settings-${page ?? "general"}`, theme });
  });
  root.render(<Canvas host={host} scenario={scenario as Scenario} settings={settings} picker={picker} theme={theme as Theme} />);
  const unmount = () => {
    if (mounts.get(host) !== unmount) return;
    mounts.delete(host);
    root.unmount();
    document.documentElement.classList.remove("settings-window", "notice-window");
  };
  mounts.set(host, unmount);
  return unmount;
}

const host = document.querySelector<HTMLElement>("[data-ibis-canvas]");
if (host) mount(host);
