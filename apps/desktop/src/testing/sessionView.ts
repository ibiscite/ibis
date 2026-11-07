// Session tests describe what Enter would act on as a view. This builds the
// session parts that make that view: the search, preview, destination and the
// outside inputs, with invented data.
import type { PickerDestination, SearchResult } from "../api";
import { styleOf } from "../format/outputs";
import { choiceKey, type CitationReference } from "../features/picker/choice";
import { initialDestination } from "../features/picker/destination";
import { initialPreview, previewKey } from "../features/picker/preview";
import { initialSearch, type SearchInputs } from "../features/picker/search";
import type { SessionEvent, SessionState, SessionView } from "../features/picker/session";
import { searchQuery, type PickerInputs } from "../features/picker/view";
import { testStates } from "../test-fixtures";

/** Arranges `state` so that the session's view is `v`; send the event it returns to apply it. */
export function arrangeView(state: SessionState, v: SessionView): [SessionState, SessionEvent] {
  const library = { ...testStates[0]!.library, revision: v.snapshotRevision ?? "synthetic" };
  const inputs: PickerInputs = { fallback: v.format, remembersLast: v.remembersLast, preferencesReady: v.ready, library, settledLibrary: library,
    statusFailed: false, showCitations: true, version: 0 };
  const selected = state.selected.size > 0;
  // The shown results; a selection, when there is one, is what Enter copies.
  const chosen = v.chosen as SearchResult[];
  // The highlighted row is the one at the session's active index.
  const filler = { ...testStates[1]!.results[2]!, revision: v.snapshotRevision ?? "synthetic" };
  const rows = chosen.length === 1 ? [...Array.from({ length: state.active }, () => filler), ...chosen] : chosen;
  const queryKey = JSON.stringify([searchQuery(state), state.interpretLocators]);
  const heard: SearchInputs = { queryKey, query: searchQuery(state), interpretLocators: state.interpretLocators, library, version: 0, recent: undefined, app: null };
  const locator = chosen.length === 1 ? (v.chosen[0] as CitationReference).locator : undefined;
  const search = { ...initialSearch, inputs: heard, session: state.session, seq: state.search.seq,
    // A search still out: its rows answer an earlier request.
    settled: { inputs: v.searchPending ? { ...heard, version: -1 } : heard, session: state.session },
    results: rows, interpretation: locator ? { locator, search: "", typed: "" } : null, list: chosen.length > 1 ? { missing: [] } : null,
    resultsQuery: queryKey, resultsRevision: library.revision };
  const version = v.projectVersion ? v.projectVersion.split(":").map(Number) as [number, number] : null;
  const app = v.appId ?? null;
  const destination: PickerDestination | null = app === null && !v.writingProjectId && !version ? null : {
    app, app_id: app, app_identity: null, automatic: null, remembered: null, unreadable: false, can_paste: false,
    output_question: v.shouldAsk ? "browser" : null, paste: null, paste_question: false, can_choose_paste: false,
    writing_project: v.writingProjectId ? { id: v.writingProjectId, name: v.writingProjectId, output: null, bibliography: "references.bib" } : null,
    writing_context: null, context_version: version,
  };
  const shownChoice = selected ? [...state.selected.values()] : chosen.length > 1 ? chosen : chosen.map((r) => ({ ...r, locator }));
  const key = previewKey(styleOf(v.format) ?? v.format, 0, choiceKey(shownChoice), version);
  const ask = shownChoice.length ? { selection: shownChoice.map((r) => ({ id: r.id, cite: r.cite ?? "", locator: (r as CitationReference).locator ?? null })), output: styleOf(v.format) ?? v.format, revision: library.revision } : null;
  const previewInputs = { key, retryKey: key, ask, contextReady: true };
  const preview = v.shown === null ? { ...initialPreview, session: state.session }
    : { ...initialPreview, inputs: previewInputs, session: state.session, request: 1, key, result: { key, request: 1, session: state.session, opened: state.opened, value: v.shown } };
  const arranged: SessionState = { ...state, inputs, search, preview,
    destination: { ...initialDestination, reader: true, remembers: true, destination, ready: true, saves: v.saving ? 1 : 0, output: v.output, observedVersion: version } };
  return [arranged, { type: "picker-inputs", inputs }];
}
