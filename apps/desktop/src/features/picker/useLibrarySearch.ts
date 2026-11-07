// The search for the current query in the open library, and which of its
// results may be acted on. The picker session owns the search (decision 0035);
// this hook tells it what to search in and reads its answer.
import { useEffect, useLayoutEffect } from "react";
import type * as nativeApi from "../../api";
import type { LibraryStatus } from "../../api";
import type { SessionState } from "./session";
import type { createSessionController } from "./sessionController";
import { sameInputs, type SearchInputs } from "./search";

export function useLibrarySearch({ session, state, search, emptySearch, query, interpretLocators, library, statusFailed, libraryVersion }: {
  session: ReturnType<typeof createSessionController>;
  state: SessionState;
  search: typeof nativeApi.search;
  emptySearch?: () => ReturnType<typeof nativeApi.search>;
  query: string;
  interpretLocators: boolean;
  /** Undefined while loading; null means the status request itself failed. */
  library: LibraryStatus | null | undefined;
  /** A failed status read ends a reconciliation wait; a later update needs fresh Enter. */
  statusFailed: boolean;
  /** Changes whenever a library opens, so the current query is searched again. */
  libraryVersion: number;
}) {
  const queryKey = JSON.stringify([query, interpretLocators]);
  const recentSearch = query.trim() ? undefined : emptySearch;
  const inputs: SearchInputs = { queryKey, query, interpretLocators, library, version: libraryVersion, recent: recentSearch };
  useLayoutEffect(() => { session.connect({ search, recent: recentSearch }); });
  // biome-ignore lint/correctness/useExhaustiveDependencies: the inputs are the query (queryKey covers query and locator mode), the library and the opening.
  useEffect(() => { session.dispatch({ type: "search-inputs", inputs }); }, [queryKey, library, libraryVersion, state.session, recentSearch]);

  const { results, interpretation, list, unfiltered, resultsQuery, resultsRevision, error, announcement, settled } = state.search;
  // The screen keeps showing the last answer while a newer search runs, so
  // typing never blanks rows, the shelf or the footer. Only actions wait:
  // rows are acted on, and a pasted list or its missing keys are trusted,
  // only when they answer the current query on the current snapshot.
  // Rows of an older snapshot are never formatted or offered; the next reply replaces them.
  const sameSnapshot = resultsRevision === library?.revision && results.every((record) => record.revision === resultsRevision);
  const answersThis = Boolean(settled) && sameInputs(settled!.inputs, inputs) && settled!.session === state.session;
  const ready = sameSnapshot && resultsQuery === queryKey && settled?.inputs.recent === recentSearch;
  // A reply ahead of status is not actionable yet, even though its request
  // finished. Keep Enter waiting through the status read and refreshed search.
  const reconciling = !error && Boolean(library?.has_snapshot) && resultsQuery === queryKey && resultsRevision !== library?.revision;
  const pending = !statusFailed && (library === undefined || library?.state === "loading" || !answersThis || reconciling);
  // The query text the shown results answer; "" before the first reply.
  const answered = resultsQuery === null ? "" : (JSON.parse(resultsQuery) as [string, boolean])[0];
  return { queryKey, results, sameSnapshot, ready, answered, interpretation, list, unfiltered, announcement, resultsQuery, error, pending };
}
