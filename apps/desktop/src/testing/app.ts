// The lightweight mocked Rust side and shared steps for whole-picker tests.
// For clipboard/document outcomes and reordered workers, use ./delivery instead.
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import type * as api from "../api";
import type { Density, FilePreview, FileSelection, PreviewIntent } from "../bindings";
import { isStyle, listedOutput } from "../format/outputs";

export async function resetApp() {
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

/** Opens the More menu and picks an action from it, as a user would. */
export async function chooseFromMore(name: string, role: "menuitem" | "menuitemcheckbox" = "menuitem") {
  if (!screen.queryByRole("menu")) fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  const item = await screen.findByRole(role, { name });
  // fireEvent.click omits the focus a real pointer/keyboard activation gives
  // the row; the shortcuts dialog uses it to choose its return destination.
  act(() => item.focus());
  fireEvent.click(item);
}

/** Choose a named row density from More's submenu, without cycling values. */
export async function chooseRows(density: Density) {
  if (!screen.queryByRole("menu")) fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  fireEvent.click(await screen.findByRole("menuitem", { name: /^Rows: / }));
  const item = await screen.findByRole("menuitemradio", { name: density.charAt(0).toUpperCase() + density.slice(1) });
  act(() => item.focus());
  fireEvent.click(item);
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

/** Picks a form from the details' Show as pop-up. */
export async function chooseForm(name: string) {
  fireEvent.click(await waitFor(() => within(citationDetails()!).getByRole("button", { name: /^Show as / })));
  const menu = await waitFor(() => { const list = document.querySelector<HTMLElement>('[role="menu"]'); if (!list) throw new Error("The Form menu is not open"); return list; });
  fireEvent.click(within(menu).getByRole("menuitemradio", { name }));
}

/** Esc from inside the details, as a person closes them. */
export function closeCitationDetails() {
  const details = citationDetails();
  if (details) fireEvent.keyDown(details.contains(document.activeElement) ? document.activeElement! : details, { key: "Escape" });
}

export function result(key: string, title: string, source = "refs.bib") {
  return {
    revision: "1", id: { library: source, key }, cite: `synthetic-cite-${key}`, source, key, title, authors: "Smith", year: "2020", matched: ["title" as const],
    first_author: "Smith", author_count: 1, container: null as string | null, item_type: "article-journal", partial: false,
  };
}

export type SyntheticId = { library: string; key: string };
export type PreviewRequest = {
  items: (Omit<FileSelection, "id"> & { id: SyntheticId })[];
  /** The engine output id the picker asked for. */
  output: string;
  expectedRevision: string;
  intent: PreviewIntent;
};
type Prepared = { request: PreviewRequest; preview: FilePreview };
const receipts = new Map<string, Prepared>();
let receiptNumber = 0;

/** A synthetic engine result. Tests choose output; no renderer runs in the browser. */
export function receipt(text: string, html: string | null = null, warnings: string[] = []): FilePreview {
  const number = ++receiptNumber;
  return { ticket: `synthetic-ticket-${number}`, content_hash: `synthetic-hash-${number}`, text, html, warnings, spans: [{ item: 0, text }] };
}

/** A synthetic result with each reference's own words as its own token, joined by "; ". */
export function lineReceipt(words: readonly string[], warnings: string[] = []): FilePreview {
  const spans = words.flatMap((text, item) => [...(item ? [{ item: null, text: "; " }] : []), { item, text }]);
  return { ...receipt(spans.map((span) => span.text).join(""), null, warnings), spans };
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

export type Handler = (args: Record<string, unknown> & PreviewRequest & { query: api.SearchQuery; ticket: string; contentHash: string }) => unknown;

/** Mock the Rust commands; returns the list of commands called. */
export function backend(overrides: Record<string, Handler> = {}): string[] {
  const calls: string[] = [];
  let lastLibrary: Partial<api.LibraryStatus> = {};
  let activityTicket = 0;
  let preferences = { output: "pandoc", density: "comfortable", larger_text: false, paste: true, menu_bar: true } as api.PickerPreferences;
  let revision = 0;
  const written = new Map<string, number>();
  const submitted = new Map<number, Partial<api.PickerPreferences>>();
  receipts.clear();
  const defaults: Record<string, Handler> = {
    "plugin:window|is_visible": () => true,
    shortcut_status: () => ({ shortcut: "CommandOrControl+Shift+Y", registered: true, error: null }),
    search: () => [],
    project_status: () => null,
    editor_request: () => null,
    session_references: () => ({ revision: 0, rows: [], absent: 0, overflowed: false, undo: null }),
    session_bibliography: () => null,
    hide_picker: () => true,
    picker_presentation: () => 0,
    picker_moving: () => null,
    delight_view: () => ({ status: null, peck: false, anniversary: false, build: "0.0.0 · invented · 2026-11-01" }),
    show_copied_notice: () => null,
    set_pinned: () => null,
    read_window_position_undo: () => null,
    undo_window_position: () => false,
    dismiss_window_position_undo: () => null,
    preview_file_citation: ({ items, output }) => {
      const style = isStyle(listedOutput(output as string) ?? "pandoc");
      return lineReceipt(items.map(({ id }) => style ? "(Smith, 2020)" : `@${id.key}`));
    },
    recent_references: (args) => overrides.search?.({ ...args, query: { text: "", interpret_locators: true } }) ?? [],
    citation_details: (args) => ({ current_form: prepared(args).request.items[Number(args.item)]?.form ?? "normal", forms: [
      { form: "normal", example: "(Smith, 2020)", reason: null },
      { form: "narrative", example: "Smith (2020)", reason: null },
      { form: "year-only", example: "(2020)", reason: null },
    ] }),
    copy_file_citation: (args) => {
      const { preview } = prepared(args);
      return { text: preview.text, warnings: preview.warnings };
    },
    picker_preferences: () => preferences,
    save_picker_preferences: ({ preferences: patch }) => { const receipt = ++revision; submitted.set(receipt, patch as Partial<api.PickerPreferences>); return receipt; },
    picker_preferences_saved: () => null,
    // Opened from no app Ibis knows: the default format applies.
    picker_destination: () => ({ app: null, app_id: null, app_identity: null, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null, paste: null, paste_question: false, can_choose_paste: false }),
    remember_app_output: () => null,
    login_item: () => ({ name: "Ibis", enabled: false, problem: null }),
    set_login_item: ({ enabled }) => ({ name: "Ibis", enabled, problem: null }),
    // Builds without an update key have no update controls.
    update_checks: () => null,
    update_available: () => false,
    update_picker_activity: ({ ticket }: Record<string, unknown>) => ticket === null ? ++activityTicket : ticket === activityTicket ? ticket : null,
    find_sources: () => ({ zotero: "absent", list: 1, files: [], cut_short: false }),
    library_source: ({ zoteroOnly }) => ({ kind: zoteroOnly || (lastLibrary.snapshot_zotero ?? lastLibrary.zotero) ? "zotero" : "file", name: zoteroOnly ? "Zotero" : lastLibrary.snapshot_name ?? lastLibrary.name ?? "library", icon: null, status_icon: null }),
    library_status: () => ({ state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, problem: null, note: null }),
  };
  mockWindows("main");
  mockIPC(
    (cmd, args) => {
      calls.push(cmd);
      if (cmd === "preview_file_citation" && (args as PreviewRequest).intent.cancel) {
        return Promise.reject({ code: "cancelled" });
      }
      const handler = overrides[cmd] ?? defaults[cmd];
      if (!handler) throw new Error(`unexpected command ${cmd}`);
      // A test's save handler sees the choice the receipt stands for.
      const receipt = cmd === "picker_preferences_saved" ? (args as { receipt: number }).receipt : 0;
      const value = handler((receipt ? { ...args as object, preferences: submitted.get(receipt) } : args ?? {}) as Parameters<Handler>[0]);
      if (cmd === "picker_preferences") return Promise.resolve(value).then((loaded) => { preferences = { ...preferences, ...loaded as api.PickerPreferences }; return preferences; });
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
        const result = preview as FilePreview;
        receipts.set(result.ticket, { request: args as PreviewRequest, preview: result });
        return result;
      });
      if (cmd === "library_status") return Promise.resolve(value).then((status) => { lastLibrary = status as api.LibraryStatus; return { revision: "1", source_generation: "1", source_is_requested: true, ...status as object }; });
      if (cmd === "search" || cmd === "recent_references") return Promise.resolve(value).then((reply) => {
        if (cmd === "recent_references" && Array.isArray(reply) && typeof reply[1] === "boolean") return reply;
        const rows = Array.isArray(reply) ? { revision: reply[0]?.revision ?? "1", results: reply } : reply;
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
