// The search for the current query in the open library, and which of its
// results may be acted on. The picker session owns the search (decision 0035);
// this hook tells it what to search in and reads its answer.
import { useLayoutEffect } from "react";
import type * as nativeApi from "../../api";
import type { LibraryStatus } from "../../api";
import type { SessionState } from "./session";
import type { createSessionController } from "./sessionController";
import type { SearchInputs } from "./search";
import { searchStatus } from "./view";

export function useLibrarySearch({ session, state, search, emptySearch, app = null, query, interpretLocators, library, statusFailed, libraryVersion }: {
  session: ReturnType<typeof createSessionController>;
  state: SessionState;
  search: typeof nativeApi.search;
  emptySearch?: () => ReturnType<typeof nativeApi.search>;
  /** The app in front: an empty query shows its recent citations. */
  app?: string | null;
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
  const inputs: SearchInputs = { queryKey, query, interpretLocators, library, version: libraryVersion, recent: recentSearch, app: recentSearch ? app : null };
  useLayoutEffect(() => { session.connect({ search, recent: recentSearch }); });
  // biome-ignore lint/correctness/useExhaustiveDependencies: the inputs are the query (queryKey covers query and locator mode), the library and the opening.
  useLayoutEffect(() => { session.dispatch({ type: "search-inputs", inputs }); }, [queryKey, library, libraryVersion, state.session, recentSearch, inputs.app]);

  const { results, interpretation, list, unfiltered, resultsQuery, error, announcement } = state.search;
  const { sameSnapshot, ready, pending } = searchStatus(state.search, inputs, state.session, statusFailed);
  // The query text the shown results answer; "" before the first reply.
  const answered = resultsQuery === null ? "" : (JSON.parse(resultsQuery) as [string, boolean])[0];
  return { queryKey, results, sameSnapshot, ready, answered, interpretation, list, unfiltered, announcement, resultsQuery, error, pending };
}
