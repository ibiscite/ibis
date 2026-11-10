// UI conveniences around the generated Rust command contract.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { OutputId } from "./format/outputs";
import { commands, type CommandError, type LibraryStatus, type PreferenceChange, type ShortcutStatus } from "./bindings";

export type { Sources } from "./bindings";
export type {
  AppPaste, Copied, Density, EntryCopy, LibraryStatus, Locator, LocatorQuery, LoginItem, LoginItemProblem, PastedList, PickerDestination, PickerPreferences, ProjectStatus, SearchQuery, SearchResult, SessionBibliography, SessionEdit, SessionReferencesView, ShortcutStatus, ZoteroGroup, ZoteroLibrary, ZoteroProfiles,
  WritingProjectView, WritingProjectsView, WritingProjectEdit,
} from "./bindings";
/** What the notice after a copy says: pasted into an app, or copied and why. */
export type NoticeMessage = NonNullable<Awaited<ReturnType<typeof commands.copiedNotice>>>;

export const {
  delightView,
  shorthandHint,
  chooseLibrary,
  useZotero,
  useSampleLibrary,
  useZoteroLibraries,
  zoteroGroups,
  zoteroProfiles,
  useZoteroProfile,
  checkZoteroSharing,
  reloadLibrary,
  libraryStatus,
  projectStatus,
  chooseProject,
  stopProject,
  copyProjectEntry,
  writingProjects,
  editWritingProject,
  editorRequest,
  sessionReferences,
  editSessionReferences,
  sessionBibliography,
  copySessionBibliography,
  exportSessionBibliography,
  openLibrarySource,
  librarySource,
  hidePicker,
  pickerPresentation,
  pickerMoving,
  setPinned,
  quit,
  shortcutStatus,
  citationDetails,
  recentReferences,
  pickerPreferences,
  pickerDestination,
  rememberAppOutput,
  loginItem,
  setLoginItem,
  findZotero,
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

export function copyFileCitation(ticket: string, contentHash: string, replaces: readonly string[] | null) {
  return commands.copyFileCitation(ticket, contentHash, [...replaces ?? []]);
}

/** Retry refreshes evidence only; the person chooses insertion afresh. */
export function retryProject() {
  return commands.retryProject();
}

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

/** UI edits omit unchanged fields; the owner saves them in the order chosen. */
export const { savePickerPreferences, pickerPreferencesSaved } = commands;

// The limit default, page and request number are not part of the UI's API:
// each search is numbered so the app can stop searches for outdated queries,
// and the page is named because a reloaded page counts from the start again.
const searchPage = crypto.getRandomValues(new Uint32Array(1))[0] ?? 0;
let searchRequest = 0;
let previewRequest = 0;

export function previewFileCitation(
  items: Parameters<typeof commands.previewFileCitation>[0],
  output: OutputId,
  expectedRevision: string,
  signal?: AbortSignal,
  context?: [number, number] | null,
  keepOrder = false,
) {
  const intent = { page: searchPage, request: ++previewRequest, cancel: false, context: context ?? null, keep_order: keepOrder };
  const pending = commands.previewFileCitation(items, output, expectedRevision, intent);
  const cancel = () => { void commands.previewFileCitation([], output, expectedRevision, { ...intent, cancel: true }).catch(() => {}); };
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
export function onPreferencesChanged(handler: (change: Partial<PreferenceChange> | null) => void): Promise<UnlistenFn> {
  return listen<Partial<PreferenceChange> | null>("preferences:changed", (event) => handler(event.payload));
}

/** Called whenever the picker is hidden, by any route. */
/** Fires before the window appears, so its first frame is already current. */
/** Before the window shows, with short text selected in the target app, if any. */
export function onPickerPreparing(handler: (selection: string | null) => void): Promise<UnlistenFn> {
  return listen<string | null>("picker:preparing", (event) => handler(event.payload));
}

export function onPickerHidden(handler: () => void): Promise<UnlistenFn> {
  return listen("picker:hidden", handler);
}

/** Called when a library finishes opening, or fails to open. */
/** A native-accepted drop supersedes the explicit source chooser. */
export function onLibraryFileDropped(handler: () => void): Promise<UnlistenFn> {
  return listen("library:file-dropped", handler);
}

/** Dropped files Ibis cannot use: only that it happened, never a path. */
export function onLibraryFileRefused(handler: () => void): Promise<UnlistenFn> {
  return listen("library:file-refused", handler);
}

export function onLibraryChanged(handler: (status: LibraryStatus) => void): Promise<UnlistenFn> {
  return listen<LibraryStatus>("library:changed", (event) => handler(event.payload));
}

/** The project bibliography's choice or its read changed. */
export function onProjectChanged(handler: (version: [number, number] | null) => void): Promise<UnlistenFn> {
  return listen<[number, number]>("project:changed", ({ payload }) => handler(payload ?? null));
}

/** Saved-definition metadata can change without retargeting the current task. */
export function onWritingProjectsChanged(handler: () => void): Promise<UnlistenFn> {
  return listen("writing-projects:changed", handler);
}

/** A chosen project bibliography could not be saved, so the old choice stands. */
export function onProjectNotSaved(handler: () => void): Promise<UnlistenFn> {
  return listen("project:not-saved", () => handler());
}

/** An editor began or stopped waiting for a citation. */
export function onEditorRequest(handler: () => void): Promise<UnlistenFn> {
  return listen("editor:request", () => handler());
}

/** The citation key another record also has, when that is why citing by key failed. */
export function sharedCitationKey(error: unknown): string | null {
  const failed = typeof error === "object" && error !== null ? error as { code?: unknown; key?: unknown } : null;
  return failed?.code === "shared_citation_key" && typeof failed.key === "string" ? failed.key : null;
}

/** Only the backend's snapshot rejection code becomes a recovery explanation. */
export function isLibraryLoading(error: unknown): boolean {
  return typeof error === "object" && error !== null && (error as Partial<CommandError>).code === "library_loading";
}

/** Work stopped on purpose. It is never a failure and never reaches the person. */
export function isCancelled(error: unknown): boolean {
  return typeof error === "object" && error !== null && (error as Partial<CommandError>).code === "cancelled";
}

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
