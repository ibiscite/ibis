// UI conveniences around the generated Rust command contract.
import type { OutputId } from "./format/outputs";
import { commands, type CommandError, type PreferenceChange, type ProblemCode } from "./bindings";
import { events, type Source } from "./nativeEvents";

export type { Sources } from "./bindings";
export type {
  AppPaste, Copied, Density, EntryCopy, LibraryStatus, Locator, LocatorQuery, LoginItem, LoginItemProblem, PastedList, PickerDestination, PickerPreferences, PickerSnapshot, ProjectStatus, SearchQuery, SearchResult, SessionBibliography, SessionEdit, SessionReferencesView, ShortcutStatus, ZoteroGroup, ZoteroLibrary, ZoteroProfiles,
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
  chooseProject,
  copyProjectEntry,
  editorRequest,
  sessionReferences,
  editSessionReferences,
  openLibrarySource,
  librarySource,
  hidePicker,
  pickerSnapshot,
  pickerRendered,
  pickerMoving,
  setPinned,
  quit,
  shortcutStatus,
  recentReferences,
  pickerPreferences,
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

export const pickerDestination = () => commands.appContext({ kind: "destination" }).then((view) => view.destination);
export const writings = () => commands.appContext({ kind: "writings" }).then((view) => view.writings);
export const editWriting = (context: string, action: import("./bindings").WritingEdit) => commands.appContext({ kind: "edit-writing", context, action }).then((view) => view.writings);
export const projectStatus = () => commands.appContext({ kind: "project-status" }).then((view) => view.project);
export const stopProject = () => commands.appContext({ kind: "stop-project" }).then(() => {});

export function deliverCitation(ticket: string, contentHash: string, replaces: readonly string[] | null, presentation: number, closestMatches: import("./bindings").ClosestMatch[]) {
  return commands.deliverCitation(ticket, contentHash, [...replaces ?? []], presentation, closestMatches);
}

/** Retry refreshes evidence only; the person chooses insertion afresh. */
export function retryProject() {
  return commands.appContext({ kind: "retry-project" }).then(() => {});
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

/** The shown citation in another output, exactly as choosing that output would copy it. */
export function citationExample(ticket: string, contentHash: string, output: OutputId, signal?: AbortSignal) {
  return formatting((intent) => commands.citationExample(ticket, contentHash, output, intent), signal);
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

/** A thrown command error's code, from the closed set the bindings generate. */
export function failedCode(error: unknown): ProblemCode | null {
  const code = typeof error === "object" && error !== null ? (error as { code?: unknown }).code : null;
  return typeof code === "string" ? code as ProblemCode : null;
}

/** The citation key another record also has, when that is why citing by key failed. */
export function sharedCitationKey(error: unknown): string | null {
  const key = (error as Partial<CommandError> | null)?.message?.args?.key;
  return failedCode(error) === "ambiguous-key" && typeof key === "string" ? key : null;
}

/** Only the backend's snapshot rejection code becomes a recovery explanation. */
export function isLibraryLoading(error: unknown): boolean {
  return failedCode(error) === "library-loading";
}

/** Work stopped on purpose. It is never a failure and never reaches the person. */
export function isCancelled(error: unknown): boolean {
  return failedCode(error) === "cancelled";
}

/** The library or the chosen references changed under the request. */
export const STALE_CODES: readonly ProblemCode[] = ["snapshot-changed", "changed-item", "missing-item"];

export function isSnapshotChanged(error: unknown): boolean {
  const code = failedCode(error);
  return code !== null && STALE_CODES.includes(code);
}

export type UpdateProgress = { attempt: number; revision: number; received: number; total: number | null; phase: "downloading" | "installing" | "failed" };

export function newerUpdate(old: UpdateProgress | null, next: UpdateProgress): UpdateProgress {
  const distance = old ? (next.revision - old.revision) >>> 0 : 1;
  return !old || next.attempt > old.attempt ||
    (next.attempt === old.attempt && distance > 0 && distance < 0x80000000) ? next : old;
}

