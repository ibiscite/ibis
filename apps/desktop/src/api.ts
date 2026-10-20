// UI conveniences around the generated Rust command contract.
import { copyFormatLabels } from "./messages";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { commands, type KeyFormat, type LibraryStatus } from "./bindings";

export type { Copied, Density, LibraryStatus, PickerPreferences, RecordId, SearchResult, ShortcutStatus } from "./bindings";
export type CopyFormat = KeyFormat;

export const {
  chooseLibrary,
  useZotero,
  reloadLibrary,
  libraryStatus,
  hidePicker,
  setPinned,
  quit,
  shortcutStatus,
  copyCitation,
  copyFormat,
  cslItems,
  copyFormatted,
  pickerPreferences,
  setPickerPreferences,
} = commands;

// This UI default is not part of the Rust command signature.
export function search(query: string, limit = 20) {
  return commands.search(query, limit);
}

/** Called whenever the picker is shown by the global shortcut or a second launch. */
export function onPickerShown(handler: () => void): Promise<UnlistenFn> {
  return listen("picker:shown", handler);
}

/** Called whenever the picker is hidden, by any route. */
export function onPickerHidden(handler: () => void): Promise<UnlistenFn> {
  return listen("picker:hidden", handler);
}

/** Called when a library finishes opening, or fails to open. */
export function onLibraryChanged(handler: (status: LibraryStatus) => void): Promise<UnlistenFn> {
  return listen<LibraryStatus>("library:changed", (event) => handler(event.payload));
}

export const COPY_FORMATS = Object.entries(copyFormatLabels).map(([value, label]) => ({
  value: value as KeyFormat,
  label,
}));
