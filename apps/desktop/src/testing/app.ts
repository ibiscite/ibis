// The lightweight mocked Rust side and shared steps for whole-picker tests.
// For clipboard/document outcomes and reordered workers, use ./delivery instead.
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import { clearMocks, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import type * as api from "../api";
import type { FilePreview, FileSelection, PreviewIntent } from "../bindings";

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

export function result(key: string, title: string, source = "refs.bib") {
  return {
    revision: "1", id: { library: source, key }, cite: `synthetic-cite-${key}`, source, key, title, authors: "Smith", year: 2020, matched: ["title" as const],
    first_author: "Smith", author_count: 1, container: null as string | null, item_type: "article-journal", partial: false,
  };
}

export type SyntheticId = { library: string; key: string };
export type PreviewRequest = {
  items: (Omit<FileSelection, "id"> & { id: SyntheticId })[];
  format: api.CopyFormat;
  style: string | null;
  expectedRevision: string;
  intent: PreviewIntent;
};
type Prepared = { request: PreviewRequest; preview: FilePreview };
const receipts = new Map<string, Prepared>();
let receiptNumber = 0;

/** A synthetic engine result. Tests choose output; no renderer runs in the browser. */
export function receipt(text: string, html: string | null = null, warnings: string[] = []): FilePreview {
  const number = ++receiptNumber;
  return { ticket: `synthetic-ticket-${number}`, content_hash: `synthetic-hash-${number}`, text, html, warnings };
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
  let preferences = { citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true, search_mark: false, paste: true } as api.PickerPreferences;
  let copyFormat: api.CopyFormat = "pandoc";
  receipts.clear();
  const defaults: Record<string, Handler> = {
    "plugin:window|is_visible": () => true,
    shortcut_status: () => ({ shortcut: "CommandOrControl+Shift+Y", registered: true, error: null }),
    search: () => [],
    hide_picker: () => null,
    show_copied_notice: () => null,
    set_pinned: () => null,
    copy_format: () => "pandoc",
    preview_file_citation: ({ items, style }) => receipt(style ? "(Smith, 2020)" : items.map(({ id }) => `@${id.key}`).join("; ")),
    copy_file_citation: (args) => {
      const { preview } = prepared(args);
      return { text: preview.text, warnings: preview.warnings };
    },
    picker_preferences: () => ({ citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true }),
    set_picker_preferences: () => null,
    // Opened from no app Ibis knows: the default format applies.
    picker_destination: () => ({ app: null, app_id: null, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null }),
    remember_app_output: () => null,
    forget_app_outputs: () => null,
    login_item: () => ({ name: "Ibis", enabled: false, problem: null }),
    set_login_item: ({ enabled }) => ({ name: "Ibis", enabled, problem: null }),
    // Builds without an update key have no update controls.
    update_checks: () => null,
    update_available: () => false,
    find_sources: () => ({ zotero: "absent", list: 1, files: [], cut_short: false }),
    library_source: ({ zoteroOnly }) => ({ kind: zoteroOnly || (lastLibrary.snapshot_zotero ?? lastLibrary.zotero) ? "zotero" : "file", name: zoteroOnly ? "Zotero" : lastLibrary.snapshot_name ?? lastLibrary.name ?? "library", icon: null, status_icon: null }),
    library_status: () => ({ state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, problem: null, note: null }),
  };
  mockWindows("main");
  mockIPC(
    (cmd, args) => {
      calls.push(cmd);
      if (cmd === "preview_file_citation" && (args as PreviewRequest).intent.cancel) {
        return Promise.reject({ code: "failed", message: "A newer citation preview replaced this request." });
      }
      const handler = overrides[cmd] ?? defaults[cmd];
      if (!handler) throw new Error(`unexpected command ${cmd}`);
      const value = handler((args ?? {}) as Parameters<Handler>[0]);
      if (cmd === "picker_preferences") return Promise.resolve(value).then((loaded) => { preferences = { ...preferences, ...loaded as api.PickerPreferences }; return preferences; });
      if (cmd === "copy_format") return Promise.resolve(value).then((loaded) => { copyFormat = loaded as api.CopyFormat; return loaded; });
      if (cmd === "set_picker_preferences") return Promise.resolve(value).then(() => {
        preferences = { ...preferences, ...(args as { preferences: Partial<api.PickerPreferences> }).preferences };
        copyFormat = (args as { copyFormat: api.CopyFormat | null }).copyFormat ?? copyFormat;
        return { preferences, copy_format: copyFormat };
      });
      if (cmd === "preview_file_citation") return Promise.resolve(value).then((preview) => {
        const result = preview as FilePreview;
        receipts.set(result.ticket, { request: args as PreviewRequest, preview: result });
        return result;
      });
      if (cmd === "library_status") return Promise.resolve(value).then((status) => { lastLibrary = status as api.LibraryStatus; return { revision: "1", ...status as object }; });
      if (cmd === "search") return Promise.resolve(value).then((reply) => Array.isArray(reply) ? { revision: reply[0]?.revision ?? "1", results: reply } : reply);
      return value;
    },
    { shouldMockEvents: true },
  );
  void listen<api.LibraryStatus>("library:changed", (event) => { lastLibrary = event.payload; });
  return calls;
}
