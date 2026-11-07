// The search for the current query, as part of the picker session (decision 0035):
// one numbered request at a time, and only the newest may answer.
import type * as nativeApi from "../../api";
import type { LibraryStatus, LocatorQuery, PastedList, SearchResult } from "../../api";
import { say } from "../../i18n";
import { choiceKey } from "./choice";

export type SearchReply = Awaited<ReturnType<typeof nativeApi.search>>;

/** What one search asks: the query and what it is searched in. `recent` is the
 * recent-citations reader for an empty query, whose app may change. */
export type SearchInputs = {
  queryKey: string; query: string; interpretLocators: boolean;
  /** Undefined while loading; null means the status read itself failed. */
  library: LibraryStatus | null | undefined;
  /** Changes whenever a library opens, so the current query is searched again. */
  version: number;
  recent: unknown;
};

export type SearchState = {
  /** The newest request, the opening it belongs to and its number. */
  inputs: SearchInputs | null; session: number; seq: number;
  /** The request whose answer is shown, and the opening that may act on it. */
  settled: { inputs: SearchInputs; session: number } | null;
  results: SearchResult[]; interpretation: LocatorQuery | null; list: PastedList | null; unfiltered: string | null;
  /** The query and snapshot the shown results answer; they stay while a newer search runs. */
  resultsQuery: string | null; resultsRevision: string | null;
  error: string | null; announcement: string;
};

export const initialSearch: SearchState = {
  inputs: null, session: -1, seq: 0, settled: null,
  results: [], interpretation: null, list: null, unfiltered: null,
  resultsQuery: null, resultsRevision: null, error: null, announcement: "",
};

export type SearchEvent =
  | { type: "search-inputs"; inputs: SearchInputs }
  | { type: "searched"; seq: number; reply: SearchReply }
  | { type: "search-failed"; seq: number }
  | { type: "announce"; seq: number };

export type SearchEffect =
  | { type: "search"; seq: number; query: string; interpretLocators: boolean; recent: boolean }
  /** Say how many results there are once typing pauses. */
  | { type: "announce-after"; seq: number; ms: number };

export const ANNOUNCE_AFTER = 300;

export const sameInputs = (a: SearchInputs | null | undefined, b: SearchInputs) =>
  Boolean(a) && a!.queryKey === b.queryKey && a!.library === b.library && a!.version === b.version && a!.recent === b.recent;

/** Typing before a library opens is normal: nothing is searched and nothing fails. */
const searchable = (library: LibraryStatus | null | undefined) =>
  !(library === undefined || library?.state === "none" || library?.state === "loading" || (library?.state === "error" && !library.has_snapshot));

export type SearchStep = { search: SearchState; firstRow: boolean; effects: (SearchEffect | { type: "refresh" })[] };

/** One search event for the opening `session`; null when nothing changes. */
export function reduceSearch(s: SearchState, event: SearchEvent, session: number): SearchStep | null {
  switch (event.type) {
    case "search-inputs": {
      const { inputs } = event;
      if (sameInputs(s.inputs, inputs) && s.session === session) return null;
      // Settled results can be adopted by a new opening; an unfinished request
      // is asked again, and its old answer cannot land in the new one.
      // Adopting retires any newer request still out, so its answer cannot land over this one.
      if (s.settled && sameInputs(s.settled.inputs, inputs)) {
        return { search: { ...s, inputs, session, seq: s.seq + 1, settled: { inputs, session } }, firstRow: false, effects: [] };
      }
      const seq = s.seq + 1;
      if (!searchable(inputs.library)) {
        return { search: { ...s, inputs, session, seq, settled: { inputs, session }, results: [], interpretation: null, list: null, unfiltered: null,
          resultsQuery: inputs.queryKey, error: null, announcement: "" }, firstRow: true, effects: [] };
      }
      return { search: { ...s, inputs, session, seq, announcement: "" }, firstRow: false,
        effects: [{ type: "search", seq, query: inputs.query, interpretLocators: inputs.interpretLocators, recent: inputs.recent !== undefined }] };
    }
    case "searched": {
      if (event.seq !== s.seq || s.session !== session || !s.inputs) return null;
      const { inputs } = s, { revision, results: found } = event.reply;
      // An unchanged reply must not undo the person's newer row choice.
      const same = s.resultsQuery === inputs.queryKey && s.resultsRevision === revision && choiceKey(s.results) === choiceKey(found);
      return {
        search: { ...s, settled: { inputs, session }, results: found, interpretation: event.reply.interpretation ?? null, list: event.reply.list ?? null,
          unfiltered: event.reply.unfiltered ?? null, resultsQuery: inputs.queryKey, resultsRevision: revision, error: null },
        firstRow: !same,
        // A reply from a newer snapshot reads the library status again.
        effects: [...revision !== inputs.library?.revision ? [{ type: "refresh" as const }] : [], { type: "announce-after", seq: s.seq, ms: ANNOUNCE_AFTER }],
      };
    }
    case "search-failed": {
      if (event.seq !== s.seq || s.session !== session || !s.inputs) return null;
      // Results of an earlier query must not sit beside this query's error.
      return { search: { ...s, settled: { inputs: s.inputs, session }, results: [], interpretation: null, list: null, unfiltered: null,
        resultsQuery: s.inputs.queryKey, error: say("picker-search-error") }, firstRow: false, effects: [] };
    }
    case "announce": {
      if (event.seq !== s.seq || s.session !== session || s.error !== null) return null;
      const count = say("picker-result-count", { count: s.results.length });
      return { search: { ...s, announcement: s.results.some((r) => r.partial) ? `${say("picker-closest-matches")} · ${count}` : count }, firstRow: false, effects: [] };
    }
  }
}
