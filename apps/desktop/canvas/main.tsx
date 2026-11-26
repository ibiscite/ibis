import { recordId } from "../src/test-fixtures";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { App } from "../src/App";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { DraftItem, PickerPreferences, PickerSnapshot, CitationIntent, FormDetails, SearchQuery } from "../src/bindings";
import type { OutputId } from "../src/format/outputs";
import { Button } from "../src/ui/Button";
import { KeyCaps } from "../src/ui/KeyCaps";
import { Group, Row, Section } from "../src/ui/Layout";
import { useAppearance } from "../src/ui/appearance";
import { englishMessages, InterfaceMessagesProvider, setInterfaceLanguage } from "../src/i18n";
import type { LibraryStatus, SearchResult } from "../src/bindings";
import { isStyle } from "../src/format/outputs";
import { setPlatform } from "../src/nativeAppearance";
export { setNativeAccent } from "../src/nativeAccent";
export { geometry } from "./geometry";
import { audit as runAudit } from "./audit";
import { SettingsApp } from "../src/settings/SettingsApp";
import { demoApi } from "../src/settings/demo";
import type { SettingsApi } from "../src/settings/api";
import "../src/tokens.css";
import tokenSource from "../src/tokens.css?raw";
import "../src/ui/Foundation.css";
import "./board.css";
import { Parts } from "./parts";
import { WritingsDialog } from "../src/features/picker/Writings";
import { SessionReferencesDialog } from "../src/features/picker/SessionReferences";
import { inventedSource, PageSources } from "../src/features/reader/source";
import type { WritingsView } from "../src/api";
import { renderNotice } from "../src/copyFeedback";
import { currentPlatform } from "../src/platform";
import "../src/ui/CopyReceipt.css";
import "../src/ui/CopiedNotice.css";
// Picker root rules must not leak into the separate Settings window.
import pickerStyles from "../src/ui/Window.css?inline";
import { fixtureDestination, formatAnswer, noFormatChosen } from "../src/testing/format";
import { sceneOf, scenarios, type Scenario } from "./scenarios";
export { scenarios };

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
  query?: string; library?: Partial<LibraryStatus>; rows?: SearchResult[]; preferences?: { density?: "compact" | "comfortable"; larger_text?: boolean; output?: string };
  /** The session's text size from ⌘+/⌘− (1 is 100%). */
  textZoom?: number;
  /** Clicked in order, each once the one before has drawn what it opens. */
  click?: string | string[]; keys?: KeyboardEventInit[];
}>> = {
  "editor-request": { preferences: { output: "apa" } },
  // A refused citation: the action off, its reason in the bar, nothing opened.
  "picker-refusal": {},
  "retained-citation": {},
  "settings-details": { click: "button.ui-details-toggle" },
  more: { library: zoteroLibrary },
  "format-menu": { library: zoteroLibrary },
  "empty-search": { query: "" },
  // The app has citations from this run, in a style: the opening screen offers its reference list.
  "reference-list": { query: "", preferences: { output: "apa" } },
  "unknown-app-empty": { query: "" },
  "page-in-search": { query: "invented 417" },
  peek: { keys: [{ key: "y", code: "KeyY", metaKey: true }] },
  shelf: { keys: [{ key: "Enter", shiftKey: true }, { key: "ArrowDown" }, { key: "Enter", shiftKey: true }] },
  "reference-details": { keys: [{ key: "Enter", shiftKey: true }], click: 'button[aria-label^="Edit "]' },
  "citation-details-larger": { preferences: { larger_text: true }, keys: [{ key: "Enter", shiftKey: true }], click: 'button[aria-label^="Edit "]' },
  reader: { keys: [{ key: "Enter", shiftKey: true }], click: ['button[aria-label^="Edit "]', 'button.citation-reveal'] },
  // The details, then More over them: More takes their place.
  "reference-details-more": { keys: [{ key: "Enter", shiftKey: true }], click: ['button[aria-label^="Edit "]', 'button[aria-label="More"]'] },
  "show-as-menu": { keys: [{ key: "Enter", shiftKey: true }], click: ['button[aria-label^="Edit "]', ".citation-field .ui-popup-button"] },
  "writings-pending": { click: 'button[aria-label="Use Field Notes"]' },
  "library-selector": { library: zoteroLibrary },
  "library-menu": { library: zoteroLibrary, click: ".library-status-trigger" },
  shortcuts: { keys: [{ key: "/", [navigator.platform.startsWith("Mac") ? "metaKey" : "ctrlKey"]: true }] },
  pinned: { keys: [{ key: "P", code: "KeyP", shiftKey: true, [navigator.platform.startsWith("Mac") ? "metaKey" : "ctrlKey"]: true }] },
  "no-matches": { rows: [] },
  missing: { query: "", library: { state: "error", has_snapshot: false, references: 0, problem: "missing", error: "the library file is unavailable; download any online-only file to this computer first" } },
  "zotero-closed": { query: "", library: { state: "error", has_snapshot: false, references: 0, zotero: true, name: "Zotero", problem: "zotero_closed", error: "Zotero’s library cannot be read while it is closed" } },
  unreadable: { query: "", library: { state: "error", has_snapshot: false, references: 0, problem: "unreadable", error: "the library file does not contain valid reference data" } },
  stale: { library: { stale: true, zotero: true, snapshot_zotero: true, name: "Zotero", snapshot_name: "Zotero", error: "Zotero’s library cannot be read while it is closed", problem: "zotero_closed", checked_at: Date.now() - 2 * 3_600_000 } },
  // The other density: Compact is the default.
  comfortable: { preferences: { density: "comfortable" } },
  "text-80": { textZoom: 0.8 },
  "text-120": { textZoom: 1.2 },
  "larger-text": { preferences: { larger_text: true } },
  "right-to-left": {},
  "long-titles": { rows: longRows },
  // APA sorts the two; moving the later token first keeps the person's order and says so.
  "citation-order": { preferences: { output: "apa" }, keys: [{ key: "ArrowDown" }, { key: "Enter", shiftKey: true }, { key: "ArrowUp" }, { key: "Enter", shiftKey: true },
    { key: "ArrowDown", [navigator.platform.startsWith("Mac") ? "metaKey" : "ctrlKey"]: true }, { key: "ArrowLeft", shiftKey: true }] },
  // A share output writes several lines per reference; the shelf keeps them in its one line.
  "share-bibtex": { preferences: { output: "bibtex" }, keys: [{ key: "Enter", shiftKey: true }, { key: "ArrowDown" }, { key: "Enter", shiftKey: true }] },
  // The engine warns about the second reference, and its details say so.
  "token-warnings": { keys: [{ key: "Enter", shiftKey: true }, { key: "ArrowDown" }, { key: "Enter", shiftKey: true }], click: '.citation-token ~ .citation-token > button[aria-label^="Edit "]' },
  // The sample references with matching rows: the way out steps aside for them.
  sample: { rows: rows.slice(0, 3), library: { sample: true, name: "Sample references", snapshot_name: "Sample references" } },
  // A numbered style's words are numbers, so each token carries its author and year.
  "citation-handles": { preferences: { output: "ieee" }, keys: [{ key: "Enter", shiftKey: true }, { key: "ArrowDown" }, { key: "Enter", shiftKey: true }] },
};

/** One invented BibTeX entry, as the engine writes it. */
const bibtex = (key: string) => `@article{${key},\n  title = {Invented reference},\n  author = {Aster, Ida},\n  year = {2026}\n}`;

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
  const writingContext = scenario.startsWith("project-");
  // The picker opens over a known app that Ibis copies into, as it usually does;
  // "unknown-app" is the one whose app could not be read, over results and over an empty search.
  const knownApp = !scenario.startsWith("unknown-app");
  const project = writings.active!;
  // A fixture's format is the one remembered for the app, as the engine would answer.
  const format = fixed?.preferences?.output ? formatAnswer(fixed.preferences.output) : noFormatChosen;
  const destination: PickerSnapshot["destination"] = knownApp ? { Ok: fixtureDestination({
    app: "Invented Editor", app_id: "invented.editor", format, cited_here: scenario === "reference-list" ? 3 : 0,
    // Paste on, as by default: the footer says Paste; an app Ibis has not seen asks Paste or Copy Only.
    can_paste: scenario === "paste" || scenario === "paste-question", paste_question: scenario === "paste-question", can_choose_paste: scenario === "paste-question",
    writing: writingContext ? project : null, writing_context: writingContext ? "invented-opening" : null, context_version: [1, 0],
  }) } : { Err: { code: "unavailable", message: { id: "operation-failures-app-read", args: {} }, recovery: { action: "retry" }, focus: "search" } };
  const refusal = ["project-check-conflict", "picker-refusal"].includes(scenario) ? "A key in thesis.bib identifies a different work. Resolve the conflict before citing it." : null;
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
  const receipts = new Map<string, { text: string; items: DraftItem[]; output: OutputId; key: string }>();
  const handlers: Record<string, (request: Request) => unknown> = {
    "plugin:window|is_visible": () => true,
    picker_snapshot: async () => ({
      presentation: 1, selection: null, library, destination,
      preferences: { Ok: await settings.pickerPreferences() },
      recents: { Ok: [{ revision: library.revision, results: scenario === "reference-list" ? rows.slice(0, 3).map((row) => ({ ...row, recent: "cited" as const })) : [] }, scenario === "reference-list", false] },
    } satisfies PickerSnapshot),
    picker_rendered: () => {
      if (scenario === "file-drag") void emit("library:file-drag", true);
      if (scenario === "undo-notice") void emit("picker:window-reset", { revision: 1, undo: 1 });
      if (scenario === "update") void emit("update:progress", { attempt: 1, revision: 1, received: 3_150_000, total: 7_400_000, phase: "downloading" });
    },
    library_status: () => library,
    // As the engine does: a trailing page number is read as the page.
    search: ({ query }) => {
      const page = query.interpret_locators ? /^(.*\S)\s+(?:pp?\.\s*)?(\d+(?:[-–]\d+)?)$/.exec(query.text) : null;
      return { revision: "1", results: query.text && query.text !== "unmatched" && !fixture ? shown : [],
        ...page ? { interpretation: { search: page[1]!, locator: { term: "page", value: page[2]!.replace("-", "–") }, typed: page[2]! } } : {} };
    },
    recent_references: () => [{ revision: "1", results: scenario === "reference-list" ? rows.slice(0, 3).map((row) => ({ ...row, recent: "cited" as const })) : [] }, scenario === "reference-list", false],
    preview_file_citation: ({ items, output, intent }) => {
      if (intent.cancel) throw { code: "cancelled" };
      const style = isStyle(output), numbered = output === "ieee";
      const ticket = `synthetic-ticket-${++sequence}`;
      // The engine writes a reference's citation key, found by its record ID, never the ID itself.
      const key = (index: number) => { const id = items[index]!.id.key; return rows.find((row) => row.id.key === id)?.key ?? ""; };
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
      // BibTeX, as the engine writes it: one entry of several lines per reference.
      if (output === "bibtex") spans.splice(0, spans.length, ...items.flatMap((_, index) => [
        ...(index ? [{ item: null, text: "\n\n" }] : []),
        { item: index, text: bibtex(key(index)) },
      ]));
      const text = spans.map((span) => span.text).join("");
      const warnings = scenario === "token-warnings" && items.length > 1 ? [{ item: 1, message: { id: "warning-source-field", args: { reference: key(1) } } }] : [];
      receipts.set(ticket, { text, items, output, key: key(0) });
      return { ticket, content_hash: ticket, text, display: { nodes: spans.map(({ item, text }) => ({ item, blocks: [], content: [{ text, italic: false, bold: false, script: "normal" }] })) }, warnings, project_refusal: refusal };
    },
    // What a way to share would copy of the shown citation, as the engine writes it.
    citation_example: ({ ticket, output }) => {
      const receipt = receipts.get(ticket);
      return receipt && output === "bibtex" ? receipt.items.map((item) => bibtex(rows.find((row) => row.id.key === item.id.key)?.key ?? "")).join("\n\n") : null;
    },
    deliver_citation: ({ ticket }) => {
      const receipt = receipts.get(ticket);
      if (!receipt) throw new Error("Unknown synthetic preview");
      return { outcome: "retained", copied: { text: receipt.text, warnings: [] }, reason: "pinned" };
    },
    // As the engine answers: a style writes names; Pandoc writes keys and has no name-only or full form.
    citation_details: ({ ticket, item }) => {
      const receipt = receipts.get(ticket);
      const current_form = receipt?.items[item]?.form ?? "normal";
      if (receipt && !isStyle(receipt.output)) return { current_form, forms: [
        { form: "normal", example: `[@${receipt.key}]`, reason: null },
        { form: "narrative", example: `@${receipt.key}`, reason: null },
        { form: "author-only", example: null, reason: "Not in this output" },
        { form: "year-only", example: `[-@${receipt.key}]`, reason: null },
        { form: "full", example: null, reason: "Not in this output" },
      ] } satisfies FormDetails;
      return { current_form, forms: [
        { form: "normal", example: "(Aster, 2026)", reason: null },
        { form: "narrative", example: "Aster (2026)", reason: null },
        { form: "author-only", example: "Aster", reason: null },
        { form: "year-only", example: "(2026)", reason: null },
      ] } satisfies FormDetails;
    },
    editor_request: () => scenario === "editor-request" ? { request: 1, format: "LaTeX", style: false } : null,
    app_context: () => { if ("Err" in destination) throw destination.Err; return { destination: destination.Ok, writings, project: null }; },
    picker_preferences: () => settings.pickerPreferences(),
    save_picker_preferences: ({ preferences }) => settings.savePickerPreferences(preferences),
    picker_preferences_saved: ({ receipt }) => settings.pickerPreferencesSaved(receipt),
    shortcut_status: () => settings.shortcutStatus(),
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
    delight_view: () => ({ fun: false, status: null, peck: false, anniversary: false, practice: scenario === "first-paste", revision: 0, build: "0.0.0 · invented · 2026-11-01" }),
    shorthand_hint: () => false,
    update_available: () => false,
    check_zotero_sharing: () => scenario === "zotero-closed" ? "closed" : "absent",
    zotero_groups: () => library.zotero ? [{ id: 1, name: "Field Notes" }, { id: 2, name: "Shared Drafts" }] : [],
    zotero_profiles: () => ({ names: [], chosen: null, running: false }),
    session_references: () => ({ revision: 0, rows: [], absent: 0, overflowed: false, undo: null, here: scenario === "reference-list" ? rows.slice(0, 3) : [] }),
    session_bibliography: () => null,
  };
  for (const command of ["quit", "set_pinned", "picker_moving", "open_library_source", "alignment_tick", "take_search_request",
    "forget_app_output", "update_checks", "update_picker_activity", "remember_app_output", "choose_project",
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
      const button = host.querySelector<HTMLButtonElement>(clicks[0]!) ?? document.querySelector<HTMLButtonElement>(`.ui-drawer ${clicks[0]!}`);
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
  if (scenario.startsWith("writings")) return <WritingsBoard variant={scenario} />;
  if (scenario === "cited-since-opening") return <CitedSinceOpeningBoard />;
  return scenario.startsWith("settings-")
    ? <SettingsApp api={settings} initialPage={scenario.slice(9)} />
    : <PageSources.Provider value={scenario === "reader" ? invented : null}><style>{pickerStyles}</style><App preview={{ query: fixtures[scenario]?.query ?? (["first-run", "empty", "file-drag", "error", "loading"].includes(scenario) ? "" : "invented") }} /></PageSources.Provider>;
}

const inventedProject = { id: "thesis", name: "Thesis", output: "autocite", bibliography: "thesis.bib" };
const writings: WritingsView = { context: "invented-opening", session_only: false, selected: "thesis", active: inventedProject, writings: [
  inventedProject,
  { id: "notes", name: "Field Notes", output: "pandoc", bibliography: null },
] };

/** The real project sheet over the real picker, with invented local choices. */
function WritingsBoard({ variant }: { variant: Scenario }) {
  const [open, setOpen] = useState(true);
  const view = writings;
  return <><style>{pickerStyles}</style><App preview={{ query: "invented" }} />
    {open && <WritingsDialog format="autocite" onClose={() => setOpen(false)} api={{
      writings: async () => view,
      editWriting: async () => variant === "writings-pending" ? new Promise<WritingsView>(() => {}) : view,
      chooseProject: async () => {}, stopProject: async () => {},
    }} />}
  </>;
}

/** The real Cited Since Opening sheet over the real picker, with invented citations. */
function CitedSinceOpeningBoard() {
  const view = { revision: 1, rows: rows.slice(0, 3), absent: 0, overflowed: false, undo: null, here: [] };
  const text = rows.slice(0, 3).map((row) => `Aster. (2026). ${row.title}. Invented Press.`).join("\n");
  return <><style>{pickerStyles}</style><App preview={{ query: "invented" }} />
    <SessionReferencesDialog style="apa" onClose={() => {}} api={{
      sessionReferences: async () => view, editSessionReferences: async () => view,
      sessionBibliography: async () => ({ content_hash: "synthetic", display: { nodes: [{ item: null, blocks: [], content: [{ text, italic: false, bold: false, script: "normal" }] }] }, left_out: [], text, warnings: [] }),
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

export function mount(host: HTMLElement, options: { scenario?: string; theme?: string; params?: Partial<Record<typeof parameters[number], string>>; textZoom?: number } = {}): () => void {
  const params = new URLSearchParams(window.location.search);
  const named = options.scenario ?? (host.dataset.ibisCanvas || params.get("scenario") || "results");
  const theme = options.theme ?? host.dataset.theme ?? params.get("theme") ?? "light";
  if (!scenarios.includes(named as Scenario)) throw new Error(`Unknown canvas scenario: ${named}`);
  // A language scene is its scenario, speaking that language: words, `lang` and direction.
  const { base: scenario, language } = sceneOf(named);
  const messages = setInterfaceLanguage(language);
  if (theme !== "light" && theme !== "dark") throw new Error(`Unknown canvas theme: ${theme}`);
  mounts.get(host)?.();
  setPlatform();
  // Parameters override tokens.css live; everything derived from them follows.
  for (const name of parameters) {
    const value = options.params?.[name]?.trim();
    if (value) document.documentElement.style.setProperty(name, value);
    else document.documentElement.style.removeProperty(name);
  }
  // The text size from ⌘+/⌘−: a scenario fixes its own, the playground may choose one.
  const zoom = options.textZoom ?? fixtures[scenario as Scenario]?.textZoom;
  if (zoom && zoom !== 1) document.documentElement.style.setProperty("--text-zoom", String(zoom));
  else document.documentElement.style.removeProperty("--text-zoom");
  const settings = demoApi({ ...fixtures[scenario as Scenario]?.preferences, appearance: theme, paste: false });
  // Citations with what people have once they cite: an app's remembered format and writings.
  if (scenario === "settings-citations") settings.formatSettings = async () => ({ apps: [{ id: "invented.editor", app: "Invented Editor", format: "apa" }], writings: writings.writings });
  // The installed app can always open settings.json (General › Advanced, and a broken file's recovery).
  settings.openSettingsFile ??= async () => null;
  // settings.json that does not parse: the engine's own Problem, as the picker gets it.
  if (scenario === "settings-broken") settings.formatSettings = () => Promise.reject({ code: "settings-invalid", message: { id: "app-settings-error-invalid", args: {} }, recovery: { action: "review-settings" }, focus: "settings" });
  if (scenario === "settings-newer") settings.formatSettings = async () => ({ apps: [], writings: [], unknown: { future_choice: "Enabled" } });
  // The states and tokens pages use the Settings window's base styles.
  const settingsWindow = scenario.startsWith("settings-") || scenario === "states" || scenario === "tokens" || scenario === "parts";
  document.documentElement.classList.toggle("settings-window", settingsWindow);
  document.documentElement.classList.toggle("notice-window", scenario === "notice");
  document.documentElement.dataset.nativeAppearance = "opaque";
  if (scenario === "right-to-left") document.documentElement.dir = "rtl";
  host.id = scenario === "notice" ? "notice" : settingsWindow ? "settings" : "root";
  const root = createRoot(host);
  const stopPicker = mockPicker(scenario as Scenario, settings, async (page) => {
    mount(host, { scenario: `settings-${page ?? "general"}`, theme });
  });
  root.render(<InterfaceMessagesProvider messages={messages}><Canvas host={host} scenario={scenario as Scenario} settings={settings} theme={theme as Theme} /></InterfaceMessagesProvider>);
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

/** Every guard of canvas/geometry.ts over the scenarios, for the runners: see canvas/audit.ts. */
export function audit(host: HTMLElement, options: Parameters<typeof runAudit>[3] = {}): Promise<string[]> {
  return runAudit(host, mount, scenarios, options);
}

const host = document.querySelector<HTMLElement>("[data-ibis-canvas]");
if (host) mount(host);
