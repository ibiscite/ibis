// What Enter would act on now, worked out from the session itself (decision
// 0035): the shown choice, its preview, the destination and whether all of
// them answer the current query. Only preferences and the library's lifecycle
// come from outside, as PickerInputs.
import type { LibraryStatus } from "../../api";
import { styleOf, type OutputId } from "../../format/outputs";
import { choiceKey } from "./choice";
import { COMMAND_PREFIX } from "./commands";
import { destinationFormat, destinationReady, projectVersion, shouldAsk } from "./destination";
import { ownedPreview, previewKey } from "./preview";
import { sameInputs, type SearchInputs, type SearchState } from "./search";
import type { SessionState, SessionView } from "./session";

/** What the session hears from outside it, whenever it changes. */
export type PickerInputs = {
  /** The default format, when the app and project name none. */
  fallback: OutputId;
  remembersLast: boolean;
  preferencesReady: boolean;
  /** The library now, and as searches see it. Undefined while loading; null means the status read failed. */
  library: LibraryStatus | null | undefined;
  settledLibrary: LibraryStatus | null | undefined;
  /** A failed status read ends a reconciliation wait; a later update needs fresh Enter. */
  statusFailed: boolean;
  /** False while the library is chosen or set up: no citation is offered. */
  showCitations: boolean;
  /** Changes whenever a library, an editor request or a project bibliography asks again. */
  version: number;
};

/** Which shown results may be acted on, for the search `inputs`. */
export function searchStatus(search: SearchState, inputs: SearchInputs, session: number, statusFailed: boolean) {
  const { results, resultsQuery, resultsRevision, error, settled } = search;
  const { library, queryKey } = inputs;
  // The screen keeps showing the last answer while a newer search runs, so
  // typing never blanks rows, the shelf or the footer. Only actions wait:
  // rows are acted on, and a pasted list or its missing keys are trusted,
  // only when they answer the current query on the current snapshot.
  // Rows of an older snapshot are never formatted or offered; the next reply replaces them.
  const sameSnapshot = resultsRevision === library?.revision && results.every((record) => record.revision === resultsRevision);
  const answersThis = Boolean(settled) && sameInputs(settled!.inputs, inputs) && settled!.session === session;
  const ready = sameSnapshot && resultsQuery === queryKey && settled?.inputs.recent === inputs.recent;
  // A reply ahead of status is not actionable yet, even though its request
  // finished. Keep Enter waiting through the status read and refreshed search.
  const reconciling = !error && Boolean(library?.has_snapshot) && resultsQuery === queryKey && resultsRevision !== library?.revision;
  const pending = !statusFailed && (library === undefined || library?.state === "loading" || !answersThis || reconciling);
  return { sameSnapshot, ready, pending };
}

/** The query the search runs: ">" asks for commands, never the library. */
export const searchQuery = (s: SessionState) => s.query.startsWith(COMMAND_PREFIX) ? "" : s.query;

/** The view Enter acts on; null until the outside inputs are known. */
export function deriveView(s: SessionState): SessionView | null {
  const o = s.inputs;
  if (!o) return null;
  const { search, destination: d } = s;
  const queryKey = JSON.stringify([searchQuery(s), s.interpretLocators]);
  // The search the session last heard of must be this query's, in this opening and library.
  const heard = search.inputs !== null && search.session === s.session && search.inputs.queryKey === queryKey
    && search.inputs.library === o.settledLibrary && search.inputs.version === o.version
    // Recent citations are the app's: a new app in front is searched again first.
    && (search.inputs.recent === undefined || search.inputs.app === (d.destination?.app_id ?? null));
  const status = heard ? searchStatus(search, search.inputs!, s.session, o.statusFailed) : { sameSnapshot: false, ready: false, pending: true };
  const { results, list, interpretation } = search;
  const highlighted = results.slice(s.active, s.active + 1).map((r) => ({ ...r, locator: interpretation?.locator }));
  // What the screen shows: the selection, otherwise the shown pasted list or highlighted result.
  const shownChoice = !o.showCitations ? [] : s.selected.size > 0 ? [...s.selected.values()] : !status.sameSnapshot ? [] : list ? results : highlighted;
  // What may be copied: the same, once the shown results answer the current query.
  const chosen = s.selected.size > 0 || status.ready ? shownChoice : [];
  const format = destinationFormat(d, o.fallback);
  const output = styleOf(format) ?? format;
  const key = previewKey(output, o.version, choiceKey(shownChoice), d.destination?.context_version ?? null, s.keepOrder);
  const preview = ownedPreview(s.preview, key, s.session, s.opened);
  return {
    appId: d.destination?.app_id ?? null, writingProjectId: d.destination?.writing_project?.id ?? null, projectVersion: projectVersion(d),
    queryKey: sessionQueryKeyOf(s), format, snapshotRevision: o.library?.revision,
    defaultChoice: list && status.ready ? choiceKey(results) : choiceKey(highlighted),
    chosen, shown: preview ? preview.value : null, searchPending: status.pending,
    // The citation stays in view through a reload, but Enter waits for its end.
    ready: destinationReady(d) && o.preferencesReady && o.library?.state !== "reloading",
    saving: d.saves > 0, shouldAsk: shouldAsk(d), output: d.output, remembersLast: o.remembersLast,
  };
}
const sessionQueryKeyOf = (s: SessionState) => JSON.stringify([s.query, s.interpretLocators]);
