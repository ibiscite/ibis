// UI conveniences around the generated Rust command contract.
import { copyFormatLabels } from "./messages";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { commands, type CommandError, type KeyFormat, type LibraryStatus, type PickerPreferences, type ShortcutStatus } from "./bindings";

export type { Sources } from "./bindings";
export type {
  AppOutput, Copied, Density, LibraryStatus, Locator, LocatorQuery, LoginItem, LoginItemProblem, PastedList, PickerDestination, PickerPreferences, SearchQuery, SearchResult, ShortcutStatus, ZoteroGroup, ZoteroLibrary,
} from "./bindings";
export type CopyFormat = KeyFormat;
/** What the notice after a copy says: pasted into an app, or copied and why. */
export type NoticeMessage = NonNullable<Awaited<ReturnType<typeof commands.copiedNotice>>>;

export const {
  chooseLibrary,
  useZotero,
  useZoteroLibraries,
  zoteroGroups,
  reloadLibrary,
  libraryStatus,
  hidePicker,
  setPinned,
  fitPicker,
  quit,
  shortcutStatus,
  previewFileCitation,
  copyFileCitation,
  copyFormat,
  pickerPreferences,
  pickerDestination,
  rememberAppOutput,
  forgetAppOutputs,
  loginItem,
  setLoginItem,
  findSources,
  openFoundLibrary,
  showCopiedNotice,
  rowContext,
  updateChecks,
  setUpdateChecks,
  checkForUpdates,
  openSettings,
} = commands;

/** UI edits omit unchanged fields; null is meaningful only for citation_style. */
export function setPickerPreferences(preferences: Partial<PickerPreferences>, copyFormat: CopyFormat | null) {
  return commands.setPickerPreferences(preferences, copyFormat);
}

// The limit default, page and request number are not part of the UI's API:
// each search is numbered so the app can stop searches for outdated queries,
// and the page is named because a reloaded page counts from the start again.
const searchPage = crypto.getRandomValues(new Uint32Array(1))[0] ?? 0;
let searchRequest = 0;

export function search(query: string, limit = 20, scope: string[] | null = null, interpretLocators = true) {
  searchRequest += 1;
  return commands.search({ text: query, interpret_locators: interpretLocators }, limit, searchPage, searchRequest, scope);
}

/** Read visibility after subscribing, including hidden startup without an event. */
export function pickerVisible(): Promise<boolean> {
  return getCurrentWindow().isVisible();
}

/** Called whenever the picker is shown by the global shortcut or a second launch. */
export function onPickerShown(handler: () => void): Promise<UnlistenFn> {
  return listen("picker:shown", handler);
}

/** Called when the shortcut's status changes, such as after a keyboard layout switch on macOS. */
export function onShortcutStatus(handler: (status: ShortcutStatus) => void): Promise<UnlistenFn> {
  return listen<ShortcutStatus>("shortcut:status", (event) => handler(event.payload));
}

/** Called when Settings changes a picker preference, with the saved preferences. */
export function onPreferencesChanged(handler: () => void): Promise<UnlistenFn> {
  return listen("preferences:changed", handler);
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

/** The citation key another record also has, when that is why citing by key failed. */
export function sharedCitationKey(error: unknown): string | null {
  const failed = typeof error === "object" && error !== null ? error as { code?: unknown; key?: unknown } : null;
  return failed?.code === "shared_citation_key" && typeof failed.key === "string" ? failed.key : null;
}

/** Only the backend's snapshot rejection code becomes a recovery explanation. */
export function isSnapshotChanged(error: unknown): boolean {
  return typeof error === "object" && error !== null && (error as Partial<CommandError>).code === "snapshot_changed";
}

export type UpdateProgress = { attempt: number; revision: number; received: number; total: number | null; phase: "downloading" | "installing" | "failed" };

export function newerUpdate(old: UpdateProgress | null, next: UpdateProgress): UpdateProgress {
  const distance = old ? (next.revision - old.revision) >>> 0 : 1;
  return !old || next.attempt > old.attempt ||
    (next.attempt === old.attempt && distance > 0 && distance < 0x80000000) ? next : old;
}

export function onUpdateProgress(handler: (progress: UpdateProgress) => void): Promise<UnlistenFn> {
  return listen<UpdateProgress>("update:progress", (event) => handler(event.payload));
}

/** Whether update checks run on their own, sent after each change is saved. */
export function onUpdateChecks(handler: (automatic: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("update:automatic", (event) => handler(event.payload));
}
