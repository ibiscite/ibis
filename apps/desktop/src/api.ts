// UI conveniences around the generated Rust command contract.
import { copyFormatLabels } from "./messages";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { commands, type CommandError, type KeyFormat, type LibraryStatus, type PickerPreferences, type ShortcutStatus } from "./bindings";

export type { Sources } from "./bindings";
export type {
  AppOutput, AppPaste, Copied, Density, LibraryStatus, Locator, LocatorQuery, LoginItem, LoginItemProblem, PastedList, PickerDestination, PickerPreferences, SearchQuery, SearchResult, ShortcutStatus, ZoteroGroup, ZoteroLibrary,
} from "./bindings";
export type CopyFormat = KeyFormat;
/** What the notice after a copy says: pasted into an app, or copied and why. */
export type NoticeMessage = NonNullable<Awaited<ReturnType<typeof commands.copiedNotice>>>;

export const {
  delightView,
  chooseLibrary,
  useZotero,
  useSampleLibrary,
  useZoteroLibraries,
  zoteroGroups,
  reloadLibrary,
  libraryStatus,
  openLibrarySource,
  librarySource,
  hidePicker,
  pickerPresentation,
  setPinned,
  quit,
  shortcutStatus,
  copyFileCitation,
  citationDetails,
  recentReferences,
  copyFormat,
  pickerPreferences,
  pickerDestination,
  rememberAppOutput,
  forgetAppOutputs,
  loginItem,
  setLoginItem,
  findSources,
  openFoundLibrary,
  rowContext,
  updateChecks,
  setUpdateChecks,
  checkForUpdates,
  updateAvailable,
  updatePickerActivity,
  openSettings,
} = commands;

/** Native saves are ordered by the settings owner before publishing a result. */
export function onWindowSizeSaved(handler: (saved: boolean) => void): Promise<UnlistenFn> {
  let latest = 0;
  return listen<{ revision: number; saved: boolean }>("picker:window-saved", ({ payload }) => {
    if (payload.revision <= latest) return;
    latest = payload.revision;
    handler(payload.saved);
  });
}

export const { takeSearchRequest, alignmentTick, resetWindowPosition, readWindowPositionUndo, undoWindowPosition, dismissWindowPositionUndo } = commands;
export function onWindowReset(handler: (state: { revision: number; undo: number | null }) => void): Promise<UnlistenFn> {
  return listen<{ revision: number; undo: number | null }>("picker:window-reset", ({ payload }) => handler(payload));
}

/** UI edits omit unchanged fields; null is meaningful only for citation_style. */
export function setPickerPreferences(preferences: Partial<PickerPreferences>, copyFormat: CopyFormat | null, revision: number) {
  return commands.setPickerPreferences(preferences, copyFormat, revision);
}

// The limit default, page and request number are not part of the UI's API:
// each search is numbered so the app can stop searches for outdated queries,
// and the page is named because a reloaded page counts from the start again.
const searchPage = crypto.getRandomValues(new Uint32Array(1))[0] ?? 0;
let searchRequest = 0;
let previewRequest = 0;

export function previewFileCitation(
  items: Parameters<typeof commands.previewFileCitation>[0],
  format: KeyFormat,
  style: Parameters<typeof commands.previewFileCitation>[2],
  expectedRevision: string,
  signal?: AbortSignal,
) {
  const intent = { page: searchPage, request: ++previewRequest, cancel: false };
  const pending = commands.previewFileCitation(items, format, style, expectedRevision, intent);
  const cancel = () => { void commands.previewFileCitation([], format, style, expectedRevision, { ...intent, cancel: true }).catch(() => {}); };
  signal?.addEventListener("abort", cancel, { once: true });
  if (signal?.aborted) cancel();
  return pending.finally(() => signal?.removeEventListener("abort", cancel));
}

export function search(query: string, limit = 20, scope: string[] | null = null, interpretLocators = true) {
  searchRequest += 1;
  return commands.search({ text: query, interpret_locators: interpretLocators }, limit, searchPage, searchRequest, scope);
}

/** Read visibility after subscribing, including hidden startup without an event. */
export function pickerVisible(): Promise<boolean> {
  return getCurrentWindow().isVisible();
}

/** Called whenever the picker is shown by the global shortcut or a second launch. */
export function onPickerShown(handler: (presentation: number) => void): Promise<UnlistenFn> {
  return listen<number>("picker:shown", (event) => handler(event.payload));
}

/** Search with Ibis has left text for the picker to collect. */
export function onSearchRequested(handler: () => void): Promise<UnlistenFn> {
  return listen("picker:search-requested", () => handler());
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
/** Fires before the window appears, so its first frame is already current. */
export function onPickerPreparing(handler: () => void): Promise<UnlistenFn> {
  return listen("picker:preparing", handler);
}

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

/** Quiet availability only; this event never shows or focuses the picker. */
export function onUpdateAvailable(handler: (available: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("update:available", (event) => handler(event.payload));
}

/** Delivery still receives the full canonical text; only the receipt is shortened. */
export function showCopiedNotice(text: string, delivery: number | null, summary: import("./bindings").ReceiptSummary | null = null) {
  return commands.showCopiedNotice(text, delivery, summary);
}

/** Presence only: native paths never enter the webview. */
export function onFileDrag(change: (active: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("library:file-drag", (event) => change(event.payload));
}
