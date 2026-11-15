// UI conveniences around the generated Rust command contract.
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { OutputId } from "./format/outputs";
import { commands, type CommandError, type PreferenceChange } from "./bindings";
import { events, type Source } from "./nativeEvents";

export type { Sources } from "./bindings";
export type {
  AppPaste, Copied, Density, EntryCopy, LibraryStatus, Locator, LocatorQuery, LoginItem, LoginItemProblem, PastedList, PickerDestination, PickerPreferences, ProjectStatus, SearchQuery, SearchResult, SessionBibliography, SessionEdit, SessionReferencesView, ShortcutStatus, ZoteroGroup, ZoteroLibrary, ZoteroProfiles,
  WritingView, WritingsView, WritingEdit,
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
  writings,
  editWriting,
  editorRequest,
  sessionReferences,
  editSessionReferences,
  openLibrarySource,
  librarySource,
  hidePicker,
  pickerPresentation,
  pickerMoving,
  setPinned,
  quit,
  shortcutStatus,
  recentReferences,
  pickerPreferences,
  pickerDestination,
  rememberAppOutput,
  forgetAppOutput,
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
export const onWindowSizeSaved: Source<(saved: boolean) => void> = (handler) => {
  let latest = 0;
  return events.onWindowSaved((payload) => {
    if (payload.revision <= latest) return;
    latest = payload.revision;
    handler(payload.saved);
  });
};

export const { takeSearchRequest, alignmentTick, resetWindowPosition, readWindowPositionUndo, undoWindowPosition, dismissWindowPositionUndo } = commands;

/** UI edits omit unchanged fields; the owner saves them in the order chosen. */
export const { savePickerPreferences, pickerPreferencesSaved } = commands;

// The limit default, page and request number are not part of the UI's API:
// each search is numbered so the app can stop searches for outdated queries,
// and the page is named because a reloaded page counts from the start again.
const searchPage = crypto.getRandomValues(new Uint32Array(1))[0] ?? 0;
let searchRequest = 0;
let previewRequest = 0;
let formattingRequest = 0;

/** Closing an auxiliary view stops its formatter work, keeping its receipt. */
function formatting<T>(run: (intent: Parameters<typeof commands.citationDetails>[3]) => Promise<T>, signal?: AbortSignal) {
  const intent = { page: searchPage, request: ++formattingRequest, cancel: false, keep_order: false, context: null };
  const pending = run(intent);
  const cancel = () => { void run({ ...intent, cancel: true }).catch(() => {}); };
  signal?.addEventListener("abort", cancel, { once: true });
  if (signal?.aborted) cancel();
  return pending.finally(() => signal?.removeEventListener("abort", cancel));
}

export function citationDetails(ticket: string, contentHash: string, item: number, signal?: AbortSignal) {
  return formatting((intent) => commands.citationDetails(ticket, contentHash, item, intent), signal);
}

export function sessionBibliography(style: string, signal?: AbortSignal) {
  return formatting((intent) => commands.sessionBibliography(style, intent), signal);
}

export function copySessionBibliography(style: string, contentHash: string, signal?: AbortSignal) {
  return formatting((intent) => commands.copySessionBibliography(style, contentHash, intent), signal);
}

export function exportSessionBibliography(style: string, contentHash: string, signal?: AbortSignal) {
  return formatting((intent) => commands.exportSessionBibliography(style, contentHash, intent), signal);
}

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

// The picker's native events; `events` names each once (nativeEvents.ts).
export const {
  onPickerShown, onPickerPreparing, onPickerHidden, onSearchRequested, onWindowReset, onShortcutStatus, onWritingsChanged,
  onProjectNotSaved, onEditorRequest, onLibraryChanged, onFileDrag, onLibraryFileDropped, onLibraryFileRefused,
  onUpdateProgress, onUpdateChecks,
} = events;

/** Settings changed a picker preference; tests may announce one without its fields. */
export const onPreferencesChanged: Source<(change: Partial<PreferenceChange> | null) => void> = events.onPreferencesChanged;

/** The bibliography's choice or its read changed. */
export const onProjectChanged: Source<(version: [number, number] | null) => void> = (handler) => events.onProjectChanged((version) => handler(version ?? null));

/** The citation key another record also has, when that is why citing by key failed. */
export function sharedCitationKey(error: unknown): string | null {
  const failed = typeof error === "object" && error !== null ? error as { code?: unknown; message?: { args?: { key?: unknown } } } : null;
  const key = failed?.message?.args?.key;
  return failed?.code === "ambiguous-key" && typeof key === "string" ? key : null;
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
  return typeof error === "object" && error !== null && ["snapshot_changed", "changed-item", "missing-item"].includes(String((error as Partial<CommandError>).code));
}

export type UpdateProgress = { attempt: number; revision: number; received: number; total: number | null; phase: "downloading" | "installing" | "failed" };

export function newerUpdate(old: UpdateProgress | null, next: UpdateProgress): UpdateProgress {
  const distance = old ? (next.revision - old.revision) >>> 0 : 1;
  return !old || next.attempt > old.attempt ||
    (next.attempt === old.attempt && distance > 0 && distance < 0x80000000) ? next : old;
}

/** Delivery still receives the full canonical text; only the receipt is shortened. */
export function showCopiedNotice(text: string, delivery: number | null, summary: import("./bindings").ReceiptSummary | null = null) {
  return commands.showCopiedNotice(text, delivery, summary);
}

