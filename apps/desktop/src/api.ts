// UI conveniences around the generated Rust command contract.
import { copyFormatLabels } from "./messages";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { commands, type CommandError, type KeyFormat, type LibraryStatus } from "./bindings";

export type { Sources } from "./bindings";
export type {
  Copied, Density, LibraryStatus, LoginItem, LoginItemProblem, PickerPreferences, RecordId, SearchResult, ShortcutStatus, ZoteroGroup, ZoteroLibrary,
} from "./bindings";
export type CopyFormat = KeyFormat;

export const {
  chooseLibrary,
  useZotero,
  useZoteroLibraries,
  zoteroGroups,
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
  citationText,
  pickerPreferences,
  setPickerPreferences,
  loginItem,
  setLoginItem,
  findSources,
  openFoundLibrary,
  showCopiedNotice,
  rowContext,
} = commands;

// The limit default, page and request number are not part of the UI's API:
// each search is numbered so the app can stop searches for outdated queries,
// and the page is named because a reloaded page counts from the start again.
const searchPage = crypto.getRandomValues(new Uint32Array(1))[0] ?? 0;
let searchRequest = 0;

export function search(query: string, limit = 20, scope: string[] | null = null) {
  searchRequest += 1;
  return commands.search(query, limit, searchPage, searchRequest, scope);
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

/** Only the backend's snapshot rejection code becomes a recovery explanation. */
export function isSnapshotChanged(error: unknown): boolean {
  return typeof error === "object" && error !== null && (error as Partial<CommandError>).code === "snapshot_changed";
}
