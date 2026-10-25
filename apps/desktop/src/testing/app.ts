// The mocked Rust side and shared steps for tests that render the whole picker.
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import { clearMocks, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { vi } from "vitest";
import type * as api from "../api";

export async function resetApp() {
  vi.useRealTimers();
  cleanup();
  // Event listeners are removed asynchronously after unmount; let that
  // finish before the mocked Tauri internals are cleared.
  await new Promise((resolve) => setTimeout(resolve, 0));
  // Closing a React Aria scope also restores focus on the next frame.
  await act(async () => { await new Promise(requestAnimationFrame); });
  clearMocks();
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

export function result(key: string, title: string, source = "refs.bib") {
  return {
    revision: "1", id: { source, native_key: key }, key, title, authors: "Smith", year: 2020, matched: ["title"],
    first_author: "Smith", author_count: 1, container: null as string | null, item_type: "article-journal",
  };
}

export type Handler = (args: Record<string, unknown> & { query: api.SearchQuery; items: api.CitationSelection[]; ids: api.RecordId[] }) => unknown;

/** Mock the Rust commands; returns the list of commands called. */
export function backend(overrides: Record<string, Handler> = {}): string[] {
  const calls: string[] = [];
  const defaults: Record<string, Handler> = {
    "plugin:window|is_visible": () => true,
    shortcut_status: () => ({ shortcut: "Control+Command+R", registered: true, error: null }),
    search: () => [],
    hide_picker: () => null,
    show_copied_notice: () => null,
    set_pinned: () => null,
    copy_format: () => "pandoc",
    // The shelf previews key syntaxes without touching the clipboard.
    citation_text: ({ items }) => ({ text: items.map(({ id }) => `@${id.native_key}`).join("; "), warnings: [] }),
    picker_preferences: () => ({ citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true }),
    set_picker_preferences: () => null,
    login_item: () => ({ name: "Ibis", enabled: false, problem: null }),
    set_login_item: ({ enabled }) => ({ name: "Ibis", enabled, problem: null }),
    // Builds without an update key have no update controls.
    update_checks: () => null,
    find_sources: () => ({ zotero: "absent", list: 1, files: [], cut_short: false }),
    library_status: () => ({ state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, problem: null, note: null }),
  };
  mockWindows("main");
  mockIPC(
    (cmd, args) => {
      calls.push(cmd);
      const handler = overrides[cmd] ?? defaults[cmd];
      if (!handler) throw new Error(`unexpected command ${cmd}`);
      const value = handler((args ?? {}) as Parameters<Handler>[0]);
      if (cmd === "library_status") return Promise.resolve(value).then((status) => ({ revision: "1", ...status as object }));
      if (cmd === "search") return Promise.resolve(value).then((reply) => Array.isArray(reply) ? { revision: reply[0]?.revision ?? "1", results: reply } : reply);
      return value;
    },
    { shouldMockEvents: true },
  );
  return calls;
}
