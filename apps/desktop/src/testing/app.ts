import { displayTree } from "../test-fixtures";
export { displayTree } from "../test-fixtures";
// The lightweight mocked Rust side and shared steps for whole-picker tests.
// For clipboard/document outcomes and reordered workers, use ./delivery instead.
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { vi } from "vitest";
import { emit, listen } from "@tauri-apps/api/event";
import type * as api from "../api";
import type { CitedWarning, ClosestMatch, DeliveryOutcome, DisplayTree, Preview, DraftItem, CitationIntent } from "../bindings";
import { recordId } from "../test-fixtures";
import { isStyle, listedOutput } from "../format/outputs";
import { fixtureDestination, formatAnswer } from "./format";
import { exampleProposals } from "./proposals";
import { forgetRecentFormats } from "../features/picker/FormatMenu";

let openingSnapshot: (() => Promise<api.PickerSnapshot | null>) | null = null;
let presentation = 0;
let summon: object | null = null;

/** A native summon builds its observation before preparing and showing the window. */
export function showPicker(requested?: number | null, selection: string | null = null) {
  if (!openingSnapshot) throw new Error("Set up the native backend before summoning the picker");
  const ticket = summon = {};
  const owner = presentation = Math.max(presentation + 1, requested ?? 0);
  void openingSnapshot().then(async (snapshot) => {
    if (!snapshot || summon !== ticket) return;
    await emit("picker:preparing", { ...snapshot, presentation: owner, selection });
    await emit("picker:shown", owner);
  });
}

export async function resetApp() {
  openingSnapshot = null;
  summon = null;
  forgetRecentFormats();
  cleanup();
  // Unmount effects and React Aria's focus restoration must finish on the clock
  // that created them, before discarding fake timers or the mocked native API.
  if (vi.isFakeTimers()) {
    await act(() => vi.runOnlyPendingTimersAsync());
    vi.useRealTimers();
  }
  // Event listeners are removed asynchronously after unmount; let that
  // finish before the mocked Tauri internals are cleared.
  await new Promise((resolve) => setTimeout(resolve, 0));
  // Closing a React Aria scope also restores focus on the next frame.
  await act(async () => { await new Promise(requestAnimationFrame); });
  clearMocks();
  receipts.clear();
  delete document.documentElement.dataset.pickerHidden;
}

/** A palette row by its name, as a person reads it (the label, not the detail or keys). */
export function paletteRow(palette: HTMLElement, name: string | RegExp): HTMLElement {
  const label = (option: HTMLElement) => option.querySelector(".ui-palette-label")?.textContent ?? "";
  const row = within(palette).queryAllByRole("option").find((option) => typeof name === "string" ? label(option) === name : name.test(label(option)));
  if (!row) throw new Error(`No palette row named ${name}: ${within(palette).queryAllByRole("option").map((option) => option.querySelector(".ui-palette-label")?.textContent).join(", ")}`);
  return row;
}

/** Opens More (the actions palette) and runs an action from it, as a user would. */
export async function chooseFromMore(name: string | RegExp) {
  if (!screen.queryByRole("dialog", { name: "Actions" })) fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  const palette = await screen.findByRole("dialog", { name: "Actions" });
  fireEvent.click(await waitFor(() => paletteRow(palette, name)));
}

/** Turns Keep Window Open on from More, or off from the footer's pin, as a person would. */
export async function toggleKeepOpen() {
  const pinned = screen.queryByRole("button", { name: "Keep Window Open", pressed: true });
  if (pinned) fireEvent.click(pinned);
  else await chooseFromMore("Keep Window Open");
}

export function libraryButton(): HTMLButtonElement {
  return document.querySelector<HTMLButtonElement>(".library-status-trigger")!;
}

/** Open the footer's library menu, leaving another menu first if needed. */
export async function chooseFromLibrary(name: string, role: "menuitem" | "menuitemcheckbox" = "menuitem") {
  if (!document.getElementById("library-menu")) {
    const other = screen.queryByRole("menu");
    if (other) fireEvent.keyDown(other, { key: "Escape" });
    fireEvent.click(libraryButton());
  }
  const item = await screen.findByRole(role, { name });
  act(() => item.focus());
  fireEvent.click(item);
}

/** Details are on demand; opening them does not grow the citation band. */
export async function openCitationDetails(): Promise<HTMLInputElement> {
  if (!screen.queryByRole("textbox", { name: "Page" })) {
    const button = await waitFor(() => {
      const target = screen.queryByRole("button", { name: "Edit citation" }) ?? screen.queryAllByRole("button", { name: /^Edit/ })[0] ?? screen.queryByRole("button", { name: "Details" });
      if (!target) throw new Error("Citation details action is not ready");
      return target;
    });
    fireEvent.click(button);
  }
  return screen.findByRole<HTMLInputElement>("textbox", { name: "Page" });
}

/** The open popover of a token's details, or null. */
export function citationDetails(): HTMLElement | null {
  return screen.queryByRole("dialog", { name: /^Citation details/ });
}

/** Picks how the citation shows, by what its choice reads: the engine's example
 * for that form (default backend: "(Smith, 2020)", "Smith (2020)", "(2020)"), or
 * the form's name before the engine answers. */
export async function chooseForm(name: string) {
  const details = await waitFor(() => within(citationDetails()!));
  fireEvent.click(await waitFor(() => details.getByRole("button", { name: /^Show as / })));
  fireEvent.click(await screen.findByRole("menuitemradio", { name }));
}

/** The details' Text before or Text after field, asked for if it is not shown yet. */
export function textField(name: "Text before" | "Text after"): HTMLInputElement {
  const details = within(citationDetails()!);
  if (!details.queryByRole("textbox", { name })) fireEvent.click(details.getByRole("button", { name }));
  return details.getByRole<HTMLInputElement>("textbox", { name });
}

export function result(key: string, title: string, source = "refs.bib") {
  const id = { library: source, key: recordId(key) };
  return {
    revision: "1", id, cite: `synthetic-cite-${key}`, source, key, title, authors: "Smith", year: "2020", matched: ["title" as const],
    first_author: "Smith", author_count: 1, container: null as string | null, item_type: "article-journal", partial: false,
  };
}

export type SyntheticId = DraftItem["id"];
export type PreviewRequest = {
  items: (Omit<DraftItem, "id"> & { id: SyntheticId })[];
  /** The engine output id the picker asked for. */
  output: string;
  expectedRevision: string;
  intent: CitationIntent;
};
type Prepared = { request: PreviewRequest; preview: Preview };
const receipts = new Map<string, Prepared>();
let receiptNumber = 0;

/** A synthetic engine result. Tests choose output; no renderer runs in the browser. */
export function receipt(text: string, display: DisplayTree | null = null, warnings: CitedWarning[] = []): Preview {
  const number = ++receiptNumber;
  return { effective_output: null, ticket: `synthetic-ticket-${number}`, content_hash: `synthetic-hash-${number}`, text, display: display ?? displayTree([{ item: 0, text }]), warnings };
}

/** A synthetic result with each reference's own words as its own token, joined by "; ". */
export function lineReceipt(words: readonly string[], warnings: CitedWarning[] = []): Preview {
  const spans = words.flatMap((text, item) => [...(item ? [{ item: null, text: "; " }] : []), { item, text }]);
  return { ...receipt(spans.map((span) => span.text).join(""), null, warnings), display: displayTree(spans) };
}

/** The keys of the references in the citation being built, in the line's order. */
export function citationKeys(): string[] {
  return [...document.querySelectorAll<HTMLElement>(".citation-token[data-built]")].map((token) => token.dataset.referenceKey!);
}

/** Resolve the exact native copy receipt, including its content hash. */
export function prepared(args: unknown): Prepared {
  const { ticket, contentHash } = args as { ticket: string; contentHash: string };
  const found = receipts.get(ticket);
  if (!found || found.preview.content_hash !== contentHash) throw { code: "snapshot_changed" };
  return found;
}

/** The native result for a citation that stays in the picker. */
export function retainedCitation(args: unknown): Extract<DeliveryOutcome, { outcome: "retained" }> {
  const { preview } = prepared(args);
  const { closestMatches } = args as { closestMatches: ClosestMatch[] };
  const warnings = [...preview.warnings, ...closestMatches.map(({ item, title }) => ({ item, message: { id: "picker-closest-match-warning", args: { title } } }))];
  return { outcome: "retained", copied: { text: preview.text, warnings }, reason: warnings.length ? "warnings" : "pinned" };
}

export type Handler = (args: Record<string, unknown> & PreviewRequest & { query: api.SearchQuery; ticket: string; contentHash: string }) => unknown;

/** Mock the Rust commands; returns the list of commands called. */
export function backend(overrides: Record<string, Handler> = {}): string[] {
  let proposals = exampleProposals();
  let attachmentTextOutsideLibrary = false;
  const calls: string[] = [];
  const citationKeys = new Map<string, string>();
  let lastLibrary: Partial<api.LibraryStatus> = {};
  let activityTicket = 0;
  let preferences = { output: "pandoc", density: "comfortable", larger_text: false, paste: true, menu_bar: true } as api.PickerPreferences;
  let revision = 0;
  const written = new Map<string, number>();
  const submitted = new Map<number, Partial<api.PickerPreferences>>();
  receipts.clear();
  const defaults: Record<string, Handler> = {
    waiting_proposals: () => proposals,
    apply_proposal: ({ id }) => { proposals = proposals.filter((proposal) => proposal.id !== id); return { status: "applied", backup: "signals.bib.ibis-backup-1" }; },
    reject_app_proposals: ({ id }) => { const app = proposals.find((proposal) => proposal.id === id)?.source_app; proposals = proposals.filter((proposal) => proposal.source_app !== app); return { status: "rejected" }; },
    reject_proposal: ({ id }) => { proposals = proposals.filter((proposal) => proposal.id !== id); return { status: "rejected" }; },
    shortcut_status: () => ({ shortcut: "CommandOrControl+Shift+Y", registered: true, error: null }),
    search: () => [],
    "project-status": () => null,
    editor_request: () => null,
    session_references: () => ({ revision: 0, rows: [], absent: 0, overflowed: false, undo: null }),
    session_bibliography: () => null,
    hide_picker: () => true,
    picker_moving: () => null,
    delight_view: () => ({ status: null, peck: false, anniversary: false, build: "0.0.0 · invented · 2026-11-01" }),
    set_pinned: () => null,
    read_window_position_undo: () => null,
    undo_window_position: () => false,
    dismiss_window_position_undo: () => null,
    preview_file_citation: ({ items, output }) => {
      const style = isStyle(listedOutput(output as string) ?? "pandoc");
      return lineReceipt(items.map(({ id }) => style ? "(Smith, 2020)" : `@${citationKeys.get(JSON.stringify(id)) ?? ""}`));
    },
    recent_references: (args) => overrides.search?.({ ...args, query: { text: "", interpret_locators: true } }) ?? [],
    citation_details: (args) => ({ current_form: prepared(args).request.items[Number(args.item)]?.form ?? "normal", forms: [
      { form: "normal", example: "(Smith, 2020)", reason: null },
      { form: "narrative", example: "Smith (2020)", reason: null },
      { form: "year-only", example: "(2020)", reason: null },
    ] }),
    // A way to share references, as the engine would copy the shown citation in it.
    citation_example: (args) => prepared(args).request.items.map(({ id }) => `@misc{${citationKeys.get(JSON.stringify(id)) ?? ""}}`).join("\n"),
    deliver_citation: retainedCitation,
    picker_snapshot: async () => {
      const read = (command: string) => Promise.resolve((overrides[command] ?? defaults[command]!)({ snapshot: true, forget: false } as unknown as Parameters<Handler>[0]));
      const outcome = async (command: string) => {
        try { return { Ok: await read(command) }; } catch (Err) { return { Err }; }
      };
      const saved = await outcome("picker_preferences");
      if ("Ok" in saved) preferences = { ...preferences, ...saved.Ok as api.PickerPreferences };
      const library = { revision: "1", source_generation: "1", source_is_requested: true, ...await read("library_status") as object };
      lastLibrary = library as api.LibraryStatus;
      const destination = await outcome("destination");
      if ("Ok" in destination) destination.Ok = fixtureDestination(destination.Ok as api.PickerDestination);
      const recents = (library as api.LibraryStatus).has_snapshot ? await outcome("recent_references")
        : { Ok: [{ revision: library.revision, results: [] }, false, false] };
      if ("Ok" in recents) {
        const value = recents.Ok;
        const tuple = Array.isArray(value) && typeof value[1] === "boolean";
        const rows = tuple ? value[0] : value;
        const reply = Array.isArray(rows) ? { revision: rows[0]?.revision ?? library.revision, results: rows } : rows;
        recents.Ok = tuple ? [reply, value[1], value[2]] : [reply, Boolean((reply as { results?: unknown[] }).results?.length), false];
      }
      return { presentation: 0, selection: null, library, destination, recents, preferences: "Ok" in saved ? { Ok: preferences } : saved };
    },
    picker_rendered: () => null,
    picker_preferences: () => preferences,
    format_settings: () => ({ apps: [], key_output_visible: Boolean(preferences.key_output), writings: [] }),
    save_picker_preferences: ({ preferences: patch }) => { const receipt = ++revision; submitted.set(receipt, patch as Partial<api.PickerPreferences>); return receipt; },
    picker_preferences_saved: () => null,
    // Opened from no app Ibis knows: the default format applies.
    "destination": async () => {
      await Promise.resolve();
      return { app: null, app_id: null, app_identity: null, format: formatAnswer(preferences.key_output ?? preferences.output, "yours"), unreadable: false, can_paste: false, paste: null, paste_question: false, can_choose_paste: false };
    },
    remember_app_output: () => null,
    login_item: () => ({ name: "Ibis", enabled: false, problem: null }),
    set_login_item: ({ enabled }) => ({ name: "Ibis", enabled, problem: null }),
    // Builds without an update key have no update controls.
    update_checks: () => null,
    attachment_text_outside_library: () => attachmentTextOutsideLibrary,
    set_attachment_text_outside_library: ({ on }) => { attachmentTextOutsideLibrary = Boolean(on); return attachmentTextOutsideLibrary; },
    update_available: () => false,
    update_picker_activity: ({ ticket }: Record<string, unknown>) => ticket === null ? ++activityTicket : ticket === activityTicket ? ticket : null,
    find_sources: () => ({ zotero: "absent", list: 1, files: [], cut_short: false }),
    library_source: ({ zoteroOnly }) => ({ kind: zoteroOnly || (lastLibrary.snapshot_zotero ?? lastLibrary.zotero) ? "zotero" : "file", name: zoteroOnly ? "Zotero" : lastLibrary.snapshot_name ?? lastLibrary.name ?? "library", icon: null, status_icon: null }),
    library_status: () => ({ state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, problem: null, note: null }),
  };
  presentation = 0;
  openingSnapshot = async () => (overrides.picker_snapshot ?? defaults.picker_snapshot!)({} as Parameters<Handler>[0]) as Promise<api.PickerSnapshot | null>;
  mockWindows("main");
  mockIPC(
    (cmd, args) => {
      const contextKind = cmd === "app_context" ? (args as { request: import("../bindings").ContextRequest }).request.kind : null;
      if (contextKind) { cmd = contextKind; args = (args as { request: unknown }).request as typeof args; }
      calls.push(cmd);
      if (cmd === "preview_file_citation" && (args as PreviewRequest).intent.cancel) {
        return Promise.reject({ code: "cancelled" });
      }
      const handler = overrides[cmd] ?? defaults[cmd];
      if (!handler) throw new Error(`unexpected command ${cmd}`);
      // A test's save handler sees the choice the receipt stands for.
      const receipt = cmd === "picker_preferences_saved" ? (args as { receipt: number }).receipt : 0;
      const value = handler((receipt ? { ...args as object, preferences: submitted.get(receipt) } : args ?? {}) as Parameters<Handler>[0]);
      if (contextKind) return Promise.resolve(value).then((answer) => ({
        destination: fixtureDestination(cmd === "destination" ? answer as api.PickerDestination : {}),
        writings: cmd === "writings" || cmd === "edit-writing" ? answer : { writings: [], selected: null, session_only: true, context: "invented" },
        project: cmd === "project-status" ? answer : null,
      }));
      if (cmd === "picker_snapshot") return Promise.resolve(value).then((value) => {
        const snapshot = value as api.PickerSnapshot | null;
        if (snapshot && "Ok" in snapshot.recents) {
          for (const row of snapshot.recents.Ok[0].results) citationKeys.set(JSON.stringify(row.id), row.key);
        }
        return snapshot;
      });
      if (cmd === "picker_preferences") {
        if (value && typeof value === "object" && !("then" in value)) preferences = { ...preferences, ...value as api.PickerPreferences };
        return Promise.resolve(value).then((loaded) => { preferences = { ...preferences, ...loaded as api.PickerPreferences }; return preferences; });
      }
      if (cmd === "picker_preferences_saved") return Promise.resolve(value).then(() => {
        // The owner applies choices in the order they were submitted: a field
        // a later choice already saved keeps that later value.
        const patch = submitted.get(receipt);
        if (!patch) throw "preference change was not submitted";
        submitted.delete(receipt);
        const kept = Object.fromEntries(Object.entries(patch).filter(([field]) => receipt > (written.get(field) ?? 0)));
        preferences = { ...preferences, ...kept };
        for (const field of Object.keys(kept)) written.set(field, receipt);
        return { preferences };
      });
      if (cmd === "preview_file_citation") return Promise.resolve(value).then((preview) => {
        const result = preview as Preview;
        result.effective_output ??= (args as PreviewRequest).output;
        receipts.set(result.ticket, { request: args as PreviewRequest, preview: result });
        return result;
      });
      if (cmd === "library_status") return Promise.resolve(value).then((status) => { lastLibrary = status as api.LibraryStatus; return { revision: "1", source_generation: "1", source_is_requested: true, ...status as object }; });
      if (cmd === "search" || cmd === "recent_references") return Promise.resolve(value).then((reply) => {
        const found = cmd === "recent_references" && Array.isArray(reply) && typeof reply[1] === "boolean" ? reply[0] : reply;
        const records: api.SearchResult[] = Array.isArray(found) ? found : (found as { results?: api.SearchResult[] })?.results ?? [];
        for (const record of records) citationKeys.set(JSON.stringify(record.id), record.key);
        if (cmd === "recent_references" && Array.isArray(reply) && typeof reply[1] === "boolean") return reply;
        const rows = Array.isArray(reply) ? { revision: reply[0]?.revision ?? lastLibrary.revision ?? "1", results: reply } : reply;
        return cmd === "recent_references" ? [rows, Boolean((rows as { results?: unknown[] }).results?.length), false] : rows;
      });
      return value;
    },
    { shouldMockEvents: true },
  );
  void listen<api.LibraryStatus>("library:changed", (event) => { lastLibrary = event.payload; });
  return calls;
}

/** The token of a reference in the citation being built, by its key; null when it is not in it. */
export function inCitation(key: string): HTMLButtonElement | null {
  return document.querySelector<HTMLButtonElement>(`.citation-token[data-built][data-reference-key="${CSS.escape(key)}"] > button`);
}

/** Waits for a reference's token to be in the citation being built. */
export function findInCitation(key: string): Promise<HTMLButtonElement> {
  return waitFor(() => { const token = inCitation(key); if (!token) throw new Error(`${key} is not in the citation`); return token; });
}

/** Removes a reference from the citation as a person does: ⌫ on its token. */
export function removeFromCitation(token: HTMLElement | null) {
  if (!token) throw new Error("That reference is not in the citation");
  act(() => token.focus());
  fireEvent.keyDown(token, { key: "Backspace" });
}

/** A native delivery refusal; its worker never rejects across IPC. */
export const changedDelivery = { outcome: "refused", problem: {
  code: "snapshot_changed", message: { id: "picker-snapshot-changed", args: {} },
  recovery: { action: "select-references" }, focus: "results",
} } satisfies import("../bindings").DeliveryOutcome;
