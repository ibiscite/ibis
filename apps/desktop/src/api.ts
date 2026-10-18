// Typed wrappers around the commands exposed by the Rust side
// (`src-tauri/src/main.rs`). The UI never calls `invoke` directly.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface LibraryStatus {
  path: string | null;
  references: number;
  warnings: number;
}

export interface SearchResult {
  key: string;
  title: string;
  authors: string;
  year: number | null;
  matched: string[];
}

export function openLibrary(path: string): Promise<LibraryStatus> {
  return invoke<LibraryStatus>("open_library", { path });
}

export function search(query: string, limit = 20): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("search", { query, limit });
}

export interface ShortcutStatus {
  shortcut: string;
  registered: boolean;
  error: string | null;
}

export function hidePicker(): Promise<void> {
  return invoke("hide_picker");
}

/** End the app. Closing the window only hides the picker. */
export function quit(): Promise<void> {
  return invoke("quit");
}

export function shortcutStatus(): Promise<ShortcutStatus> {
  return invoke<ShortcutStatus>("shortcut_status");
}

/** Called whenever the picker is shown by the global shortcut or a second launch. */
export function onPickerShown(handler: () => void): Promise<UnlistenFn> {
  return listen("picker:shown", handler);
}
