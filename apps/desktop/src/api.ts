// Typed wrappers around the commands exposed by the Rust side
// (`src-tauri/src/main.rs`). The UI never calls `invoke` directly.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface LibraryStatus {
  path: string | null;
  references: number;
  warnings: number;
  /** Why the last attempt to open a library failed, if it did. */
  error: string | null;
}

/** Identity of a record: the source it came from and its key there. */
export interface RecordId {
  source: string;
  native_key: string;
}

export interface SearchResult {
  id: RecordId;
  key: string;
  title: string;
  authors: string;
  year: number | null;
  matched: string[];
}

/** Start opening a library; the outcome arrives through `onLibraryChanged`. */
export function openLibrary(path: string): Promise<void> {
  return invoke("open_library", { path });
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

/** Called when a library finishes opening, or fails to open. */
export function onLibraryChanged(handler: (status: LibraryStatus) => void): Promise<UnlistenFn> {
  return listen<LibraryStatus>("library:changed", (event) => handler(event.payload));
}

/** Key-based citation syntaxes, as named by `ibis_core::cite::KeyFormat`. */
export const COPY_FORMATS = [
  { value: "pandoc", label: "Pandoc / Markdown  [@key]" },
  { value: "autocite", label: "BibLaTeX  \\autocite{key}" },
  { value: "parencite", label: "BibLaTeX  \\parencite{key}" },
  { value: "textcite", label: "BibLaTeX  \\textcite{key}" },
  { value: "cite", label: "LaTeX  \\cite{key}" },
  { value: "typst", label: "Typst  @key" },
  { value: "keys", label: "Keys only" },
] as const;

export type CopyFormat = (typeof COPY_FORMATS)[number]["value"];

export interface Copied {
  text: string;
  warnings: string[];
}

/** Copy the records, in this order, as one citation. */
export function copyCitation(ids: RecordId[], format: CopyFormat): Promise<Copied> {
  return invoke<Copied>("copy_citation", { ids, format });
}

/** The format used last, or the default. */
export function copyFormat(): Promise<CopyFormat> {
  return invoke<CopyFormat>("copy_format");
}
