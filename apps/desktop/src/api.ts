// UI conveniences around the generated Rust command contract.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { commands, type KeyFormat, type LibraryStatus } from "./bindings";

export type { Copied, LibraryStatus, RecordId, SearchResult, ShortcutStatus } from "./bindings";
export type CopyFormat = KeyFormat;

export const {
  chooseLibrary,
  reloadLibrary,
  libraryStatus,
  hidePicker,
  quit,
  shortcutStatus,
  copyCitation,
  copyFormat,
} = commands;

// This UI default is not part of the Rust command signature.
export function search(query: string, limit = 20) {
  return commands.search(query, limit);
}

/** Called whenever the picker is shown by the global shortcut or a second launch. */
export function onPickerShown(handler: () => void): Promise<UnlistenFn> {
  return listen("picker:shown", handler);
}

/** Called when a library finishes opening, or fails to open. */
export function onLibraryChanged(handler: (status: LibraryStatus) => void): Promise<UnlistenFn> {
  return listen<LibraryStatus>("library:changed", (event) => handler(event.payload));
}

// Labels are UI copy; Record requires a label for every Rust format variant.
const COPY_FORMAT_LABELS: Record<KeyFormat, string> = {
  pandoc: "Pandoc / Markdown  [@key]",
  autocite: "BibLaTeX  \\autocite{key}",
  parencite: "BibLaTeX  \\parencite{key}",
  textcite: "BibLaTeX  \\textcite{key}",
  cite: "LaTeX  \\cite{key}",
  typst: "Typst  @key",
  keys: "Keys only",
};

export const COPY_FORMATS = Object.entries(COPY_FORMAT_LABELS).map(([value, label]) => ({
  value: value as KeyFormat,
  label,
}));
