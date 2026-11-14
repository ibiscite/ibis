import { recordId, citationDisplay } from "../src/test-fixtures";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { App } from "../src/App";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { DraftItem, PickerPreferences, CitationIntent, FormDetails, SearchQuery } from "../src/bindings";
import type { OutputId } from "../src/format/outputs";
import { Button } from "../src/ui/Button";
import { KeyCaps } from "../src/ui/KeyCaps";
import { Group, Row, Section } from "../src/ui/Layout";
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
import { SessionReferencesDialog } from "../src/features/picker/SessionReferences";
import { inventedSource, PageSources } from "../src/features/reader/source";
import type { WritingProjectsView } from "../src/api";
import { renderNotice } from "../src/copyFeedback";
import { currentPlatform } from "../src/platform";
import "../src/ui/CopyReceipt.css";
import "../src/ui/CopiedNotice.css";
// Picker root rules must not leak into the separate Settings window.
import pickerStyles from "../src/ui/Window.css?inline";

export const scenarios = ["notice", "results", "first-run", "more", "format-menu", "writing-projects", "writing-projects-pending", "cited-since-opening", "project-check-conflict", "states", "parts", "empty", "file-drag", "error", "loading", "empty-search", "shelf", "reference-details", "show-as-menu", "reader", "commands", "page-in-search", "peek", "library-selector", "library-menu", "shortcuts", "pinned", "no-matches", "missing", "zotero-closed", "unreadable", "stale", "compact", "larger-text", "right-to-left", "long-titles", "update", "undo-notice", "citation-order", "citation-handles", "citation-details-larger", "tokens", "settings-general", "settings-look", "settings-citations", "settings-shortcuts", "settings-about"] as const;
type Scenario = typeof scenarios[number];
type Theme = "light" | "dark";

// Same invented records as picker-layout.browser.mjs; that executable browser
// probe cannot be imported without launching Playwright and a local server.
const rows: SearchResult[] = Array.from({ length: 14 }, (_, i) => ({
  revision: "1", id: { library: "invented.bib", key: recordId(`SENTINEL-ID-canvas-${i}`) }, cite: `synthetic-cite-${i}`,
  source: "invented.bib", key: `synthetic${i}`, title: `Invented reference ${i}`,
  authors: "Aster", first_author: "Aster", author_count: 1, year: "2026", container: null,
  item_type: "article-journal", partial: false, matched: ["title"],
}));

const longTitle = "An invented reference whose title runs well past the end of the row so the second line and its ellipsis are visible";
const longRows = rows.map((row) => ({ ...row, title: longTitle, authors: "Aster, Birch, Cedar, Dune, Elm and Fern", container: "The Invented Journal of Very Long Names" }));

// The footer and both palettes share a populated selector fixture. Group names
// and counts are invented; the menu shows its anchor and the rows above it.
const zoteroLibrary: Partial<LibraryStatus> = { zotero: true, snapshot_zotero: true, name: "Zotero", snapshot_name: "Zotero",
  references: 190, zotero_libraries: ["user", { group: 1 }] };

/** What differs from the loaded picker. Each entry is real app state: the
 * picture is whatever the app draws for it. */
const fixtures: Partial<Record<Scenario, {
  query?: string; library?: Partial<LibraryStatus>; rows?: SearchResult[]; preferences?: { density?: "compact"; larger_text?: boolean; output?: string };
  /** Clicked in order, each once the one before has drawn what it opens. */
  click?: string | string[]; keys?: KeyboardEventInit[];
}>> = {
  more: { library: zoteroLibrary },
  "format-menu": { library: zoteroLibrary },
  "empty-search": { query: "" },
  commands: { query: ">" },
  "page-in-search": { query: "invented 417" },
  peek: { keys: [{ key: "y", code: "KeyY", metaKey: true }] },
  shelf: { keys: [{ key: "Enter", shiftKey: true }, { key: "ArrowDown" }, { key: "Enter", shiftKey: true }] },
  "reference-details": { keys: [{ key: "Enter", shiftKey: true }], click: 'button[aria-label^="Edit "]' },
  "citation-details-larger": { preferences: { larger_text: true }, keys: [{ key: "Enter", shiftKey: true }], click: 'button[aria-label^="Edit "]' },
  reader: { keys: [{ key: "Enter", shiftKey: true }], click: ['button[aria-label^="Edit "]', 'button.citation-reveal'] },
  "show-as-menu": { keys: [{ key: "Enter", shiftKey: true }], click: ['button[aria-label^="Edit "]', ".citation-field .ui-popup-button"] },
  "writing-projects-pending": { click: 'button[aria-label="Use Field Notes"]' },
  "library-selector": { library: zoteroLibrary },
  "library-menu": { library: zoteroLibrary, click: ".library-status-trigger" },
  shortcuts: { keys: [{ key: "/", [navigator.platform.startsWith("Mac") ? "metaKey" : "ctrlKey"]: true }] },
  pinned: { keys: [{ key: "P", code: "KeyP", shiftKey: true, [navigator.platform.startsWith("Mac") ? "metaKey" : "ctrlKey"]: true }] },
  "no-matches": { rows: [] },
  missing: { query: "", library: { state: "error", has_snapshot: false, references: 0, problem: "missing", error: "the library file is unavailable; download any online-only file to this computer first" } },
  "zotero-closed": { query: "", library: { state: "error", has_snapshot: false, references: 0, zotero: true, name: "Zotero", problem: "zotero_closed", error: "Zotero’s library cannot be read while it is closed" } },
  unreadable: { query: "", library: { state: "error", has_snapshot: false, references: 0, problem: "unreadable", error: "the library file does not contain valid reference data" } },
  stale: { library: { stale: true, zotero: true, snapshot_zotero: true, name: "Zotero", snapshot_name: "Zotero", error: "Zotero’s library cannot be read while it is closed", problem: "zotero_closed", checked_at: Date.now() - 2 * 3_600_000 } },
  compact: { preferences: { density: "compact" } },
  "larger-text": { preferences: { larger_text: true } },
  "right-to-left": {},
  "long-titles": { rows: longRows },
  // APA sorts the two; moving the later token first keeps the person's order and says so.
  "citation-order": { preferences: { output: "apa" }, keys: [{ key: "ArrowDown" }, { key: "Enter", shiftKey: true }, { key: "ArrowUp" }, { key: "Enter", shiftKey: true },
    { key: "ArrowDown", [navigator.platform.startsWith("Mac") ? "metaKey" : "ctrlKey"]: true }, { key: "ArrowLeft", shiftKey: true }] },
  // A numbered style's words are numbers, so each token carries its author and year.
  "citation-handles": { preferences: { output: "ieee" }, keys: [{ key: "Enter", shiftKey: true }, { key: "ArrowDown" }, { key: "Enter", shiftKey: true }] },
};

// Invented command replies for the real picker. The canvas intercepts IPC at
// the same boundary as interface tests; App needs no alternate API or feature flags.
function mockPicker(scenario: Scenario, settings: SettingsApi, openSettings: (page: string | null) => Promise<void>) {
  type Request = {
    query: SearchQuery; items: DraftItem[]; output: OutputId; intent: CitationIntent;
    ticket: string; item: number; preferences: Partial<PickerPreferences>; receipt: number; page: string | null;
  };
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
    stale: false, updated_at: null, checked_at: firstRun ? null : Date.now() - 3 * 60_000, ...fixed?.library,
  };
  let sequence = 0;
  const receipts = new Map<string, { text: string; items: DraftItem[] }>();
  const handlers: Record<string, (request: Request) => unknown> = {
    "plugin:window|is_visible": () => true,
    library_status: () => library,
    // As the engine does: a trailing page number is read as the page.
    search: ({ query }) => {
      const page = query.interpret_locators ? /^(.*\S)\s+(?:pp?\.\s*)?(\d+(?:[-–]\d+)?)$/.exec(query.text) : null;
      return { revision: "1", results: query.text && !fixture ? shown : [],
        ...page ? { interpretation: { search: page[1]!, locator: { term: "page", value: page[2]!.replace("-", "–") }, typed: page[2]! } } : {} };
    },
    recent_references: () => [{ revision: "1", results: [] }, false, false],
    preview_file_citation: ({ items, output, intent }) => {
      if (intent.cancel) throw { code: "cancelled" };
      const style = isStyle(output), numbered = output === "ieee";
      const ticket = `synthetic-ticket-${++sequence}`;
      const key = (index: number) => shown.find((row) => JSON.stringify(row.id) === JSON.stringify(items[index]!.id))?.key ?? "";
      const page = (index: number) => items[index]!.locator?.value ?? items[index]!.locator_input?.text;
      const order = items.map((_, index) => index);
      if (style && !intent.keep_order) order.sort((a, b) => key(a).localeCompare(key(b)));
      const words = (index: number) => numbered ? String(order.indexOf(index) + 1)
        : style ? `Aster, 2026${String.fromCharCode(97 + Number(key(index).replace(/\D/g, "")) % 26)}${page(index) ? `, p. ${page(index)}` : ""}`
        : `@${key(index)}${page(index) ? `, p. ${page(index)}` : ""}`;
      const spans = order.flatMap((index, at) => [
        ...(at ? [{ item: null, text: numbered ? "], [" : "; " }] : []),
        { item: index, text: words(index) },
      ]);
      spans.unshift({ item: null, text: !style || numbered ? "[" : "(" });
      spans.push({ item: null, text: !style || numbered ? "]" : ")" });
      const text = spans.map((span) => span.text).join("");
      receipts.set(ticket, { text, items });
      return { ticket, content_hash: ticket, text, display: citationDisplay(spans), html: null, warnings: [], project_refusal: refusal };
    },
    copy_file_citation: ({ ticket }) => {
      const receipt = receipts.get(ticket);
      if (!receipt) throw new Error("Unknown synthetic preview");
      return { text: receipt.text, warnings: [] };
    },
    citation_details: ({ ticket, item }) => ({ current_form: receipts.get(ticket)?.items[item]?.form ?? "normal", forms: [
      { form: "normal", example: "(Aster, 2026)", reason: null },
      { form: "narrative", example: "Aster (2026)", reason: null },
      { form: "author-only", example: "Aster", reason: null },
      { form: "year-only", example: "(2026)", reason: null },
    ] } satisfies FormDetails),
    picker_destination: () => projectContext ? { app: "Invented Editor", app_id: "invented.editor", app_identity: null,
      automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null, paste: null, paste_question: false, can_choose_paste: false,
      writing_project: project, writing_context: "invented-opening", context_version: [1, 0] } : null,
    writing_projects: () => writingProjects,
    edit_writing_project: () => writingProjects,
    picker_preferences: () => settings.pickerPreferences(),
    save_picker_preferences: ({ preferences }) => settings.savePickerPreferences(preferences),
    picker_preferences_saved: ({ receipt }) => settings.pickerPreferencesSaved(receipt),
    shortcut_status: () => settings.shortcutStatus(),
    picker_presentation: () => 0,
    hide_picker: () => true,
    choose_library: () => { throw new Error("File chooser unavailable on canvas"); },
    use_zotero: () => { throw new Error("Zotero unavailable on canvas"); },
    use_sample_library: () => { throw new Error("Use the results scenario for invented references"); },
    find_sources: () => ({ zotero: "absent", list: 1, files: [], cut_short: false }),
    find_zotero: () => "absent",
    library_source: () => ({ kind: library.zotero ? "zotero" : "file", name: library.name, icon: null, status_icon: null }),
    open_settings: ({ page }) => openSettings(page),
    read_window_position_undo: () => scenario === "undo-notice" ? 1 : null,
    undo_window_position: () => scenario === "undo-notice",
    delight_view: () => ({ fun: false, status: null, peck: false, anniversary: false, revision: 0, build: "0.0.0 · invented · 2026-11-01" }),
    shorthand_hint: () => false,
    update_available: () => false,
    check_zotero_sharing: () => scenario === "zotero-closed" ? "closed" : "absent",
    zotero_groups: () => library.zotero ? [{ id: 1, name: "Field Notes" }, { id: 2, name: "Shared Drafts" }] : [],
    zotero_profiles: () => ({ names: [], chosen: null, running: false }),
    session_references: () => ({ revision: 0, rows: [], absent: 0, overflowed: false, undo: null }),
    session_bibliography: () => null,
  };
  for (const command of ["quit", "set_pinned", "picker_moving", "open_library_source", "alignment_tick", "take_search_request",
    "project_status", "editor_request", "update_checks", "update_picker_activity", "remember_app_output", "retry_project", "choose_project",
    "open_found_library", "reload_library", "dismiss_window_position_undo"]) handlers[command] = () => null;
  mockWindows("main");
  mockIPC((command, payload) => {
    const handler = handlers[command];
    if (!handler) {
      console.error(`Command unavailable on canvas: ${command}`);
      throw new Error(`Command unavailable on canvas: ${command}`);
    }
    return handler(payload as Request);
  }, { shouldMockEvents: true });
  const subscription = settings.onPreferencesChanged(() => { void emit("preferences:changed", null); });
  return () => { void subscription.then((stop) => stop()); };
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
  return <div className="canvas-parts"><Section id="canvas-states"><Group>{(["hovered", "pressed", "focus-visible"] as const).map((state, index) => <Row key={state} id={`canvas-${state}`} label={englishMessages.text(labels[index]!)}>
    <Button variant="secondary" render={(native) => <button {...native} {...{ [`data-${state}`]: true }} />}>Check Now<KeyCaps accelerator="CmdOrCtrl+K" /></Button>
  </Row>)}</Group></Section></div>;
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
  const colours = family("--text", "--surface", "--accent", "--selected", "--fill", "--separator", "--scarlet", "--status", "--ring").filter(colour);
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

function Canvas({ host, scenario, settings, theme }: { host: HTMLElement; scenario: Scenario; settings: SettingsApi; theme: Theme }) {
  useEffect(() => {
    if (scenario === "file-drag") void emit("library:file-drag", true);
    if (scenario === "undo-notice") void emit("picker:window-reset", { revision: 1, undo: 1 });
    if (scenario === "update") void emit("update:progress", { attempt: 1, revision: 1, received: 3_150_000, total: 7_400_000, phase: "downloading" });
  }, [scenario]);
  useEffect(() => {
    const fixed = fixtures[scenario];
    const clicks = scenario === "more" ? ['button[aria-label="More"]'] : scenario === "format-menu" ? ['button[aria-label^="Citation format"]'] : [fixed?.click ?? []].flat();
    const keys = [...fixed?.keys ?? []];
    if (!clicks.length && !keys.length) return;
    // Act on the real controls once they are ready; no polling or stale completion.
    const open = () => {
      const search = host.querySelector<HTMLInputElement>("#search");
      if (keys.length && (!search || !host.querySelector('[role="option"]'))) return;
      // A key for the citation's tokens waits until the line has drawn them.
      if (keys[0]?.key === "ArrowDown" && (keys[0].metaKey || keys[0].ctrlKey) && !host.querySelector(".citation-token-button")) return;
      // One key per commit: each acts on the state the last one left.
      // Keys go where focus is, so ⌘↓ can hand the next ones to a token.
      const focused = host.contains(document.activeElement) ? document.activeElement : search;
      if (keys.length) { focused!.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...keys.shift()! })); return; }
      if (!clicks.length) { observer.disconnect(); return; }
      const button = host.querySelector<HTMLButtonElement>(clicks[0]!) ?? document.querySelector<HTMLButtonElement>(`.ui-popover ${clicks[0]!}`);
      if (!button || button.disabled || document.documentElement.hasAttribute("data-picker-hidden")) return;
      clicks.shift();
      if (!clicks.length) observer.disconnect();
      button.click();
    };
    const observer = new MutationObserver(open);
    observer.observe(document.body, { childList: true, subtree: true, attributes: true });
    open();
    return () => observer.disconnect();
  }, [host, scenario]);
  if (scenario === "notice") return <Notice theme={theme} />;
  if (scenario === "states") return <States theme={theme} />;
  if (scenario === "tokens") return <Tokens theme={theme} />;
  if (scenario === "parts") return <ThemedParts theme={theme} />;
  if (scenario.startsWith("writing-projects")) return <WritingProjectsBoard variant={scenario} />;
  if (scenario === "cited-since-opening") return <CitedSinceOpeningBoard />;
  return scenario.startsWith("settings-")
    ? <SettingsApp api={settings} initialPage={scenario.slice(9)} />
    : <PageSources.Provider value={scenario === "reader" ? invented : null}><style>{pickerStyles}</style><App preview={{ query: fixtures[scenario]?.query ?? (["first-run", "empty", "file-drag", "error", "loading"].includes(scenario) ? "" : "invented") }} /></PageSources.Provider>;
}

const inventedProject = { id: "thesis", name: "Thesis", output: "autocite", bibliography: "thesis.bib" };
const writingProjects: WritingProjectsView = { context: "invented-opening", session_only: false, selected: "thesis", active: inventedProject, projects: [
  inventedProject,
  { id: "notes", name: "Field Notes", output: "pandoc", bibliography: null },
] };

/** The real project sheet over the real picker, with invented local choices. */
function WritingProjectsBoard({ variant }: { variant: Scenario }) {
  const [open, setOpen] = useState(true);
  const view = writingProjects;
  return <><style>{pickerStyles}</style><App preview={{ query: "invented" }} />
    {open && <WritingProjectsDialog format="autocite" onClose={() => setOpen(false)} api={{
      writingProjects: async () => view,
      editWritingProject: async () => variant === "writing-projects-pending" ? new Promise<WritingProjectsView>(() => {}) : view,
      chooseProject: async () => {}, stopProject: async () => {},
    }} />}
  </>;
}

/** The real Cited Since Opening sheet over the real picker, with invented citations. */
function CitedSinceOpeningBoard() {
  const view = { revision: 1, rows: rows.slice(0, 3), absent: 0, overflowed: false, undo: null };
  const text = rows.slice(0, 3).map((row) => `Aster. (2026). ${row.title}. Invented Press.`).join("\n");
  return <><style>{pickerStyles}</style><App preview={{ query: "invented" }} />
    <SessionReferencesDialog style="apa" onClose={() => {}} api={{
      sessionReferences: async () => view, editSessionReferences: async () => view,
      sessionBibliography: async () => ({ content_hash: "synthetic", html: null, left_out: [], text, warnings: [] }),
      copySessionBibliography: async () => null, exportSessionBibliography: async () => null,
    }} />
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
  const stopPicker = mockPicker(scenario as Scenario, settings, async (page) => {
    mount(host, { scenario: `settings-${page ?? "general"}`, theme });
  });
  root.render(<Canvas host={host} scenario={scenario as Scenario} settings={settings} theme={theme as Theme} />);
  const unmount = () => {
    if (mounts.get(host) !== unmount) return;
    mounts.delete(host);
    root.unmount();
    stopPicker();
    document.documentElement.classList.remove("settings-window", "notice-window");
  };
  mounts.set(host, unmount);
  return unmount;
}

const host = document.querySelector<HTMLElement>("[data-ibis-canvas]");
if (host) mount(host);
