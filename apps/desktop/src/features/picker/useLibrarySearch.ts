// The search for the current query in the open library, and which of its results may be acted on.
import { useEffect, useState } from "react";
import type * as nativeApi from "../../api";
import type { LibraryStatus, LocatorQuery, PastedList, SearchResult } from "../../api";
import { pickerText } from "../../messages";
import { choiceKey } from "./choice";

export function useLibrarySearch({ search, query, interpretLocators, library, libraryVersion, setActive, refreshSnapshot }: {
  search: typeof nativeApi.search;
  query: string;
  interpretLocators: boolean;
  /** Undefined while loading; null means the status request itself failed. */
  library: LibraryStatus | null | undefined;
  /** Changes whenever a library opens, so the current query is searched again. */
  libraryVersion: number;
  setActive: (index: number) => void;
  /** Reads the library status again when a reply comes from a newer snapshot. */
  refreshSnapshot: () => void;
}) {
  const queryKey = JSON.stringify([query, interpretLocators]);
  const [interpretation, setInterpretation] = useState<LocatorQuery | null>(null);
  const [list, setList] = useState<PastedList | null>(null);
  const [results, setResults] = useState<SearchResult[]>([]);
  const [announcement, setAnnouncement] = useState("");
  // The query the shown results belong to; differs while a search is pending.
  const [resultsQuery, setResultsQuery] = useState<string | null>(null);
  const [resultsRevision, setResultsRevision] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [settled, setSettled] = useState<{ queryKey: string; library: typeof library; version: number } | null>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: searches when the query (queryKey covers query and locator mode) or the library changes. The earlier results are read only to keep the user's row when a reply repeats them.
  useEffect(() => {
    const request = { queryKey, library, version: libraryVersion };
    setAnnouncement("");
    // Typing before choosing a library is normal; preserve the query and
    // chooser. Actual library/status failures still report search errors.
    if (library === undefined || library?.state === "none" || library?.state === "loading" || (library?.state === "error" && !library.has_snapshot)) {
      setResults([]);
      setInterpretation(null);
      setList(null);
      setResultsQuery(queryKey);
      setActive(0);
      setError(null);
      setSettled(request);
      return;
    }
    let current = true;
    let announcementTimer: number | undefined;
    search(query, undefined, undefined, interpretLocators)
      .then(({ revision, results: found, interpretation: interpreted, list: pasted }) => {
        if (current) {
          setResults(found);
          setInterpretation(interpreted ?? null);
          setList(pasted ?? null);
          setResultsRevision(revision);
          if (revision !== library?.revision) refreshSnapshot();
          setResultsQuery(queryKey);
          // An unchanged reply must not undo the user's newer row choice.
          const sameResults = resultsQuery === queryKey && resultsRevision === revision && choiceKey(results) === choiceKey(found);
          if (!sameResults) setActive(0);
          setError(null);
          setSettled(request);
          announcementTimer = window.setTimeout(() => {
            const count = pickerText.resultCount(found.length);
            setAnnouncement(found.some((r) => r.partial) ? `${pickerText.closestMatches} · ${count}` : count);
          }, 300);
        }
      })
      .catch(() => {
        // Results of an earlier query must not sit beside this query's error.
        if (current) {
          setResults([]);
          setInterpretation(null);
          setList(null);
          setResultsQuery(queryKey);
          setError(pickerText.searchError);
          setSettled(request);
        }
      });
    return () => {
      current = false;
      window.clearTimeout(announcementTimer);
    };
  }, [queryKey, library, libraryVersion]);

  // Results belong to the query that produced them. While a newer search is
  // pending, the rows still on screen are not acted on or offered by any key,
  // click or footer hint; only these current results are.
  const current = resultsQuery === queryKey && resultsRevision === library?.revision && results.every((record) => record.revision === resultsRevision) ? results : [];
  // A reply ahead of status is not actionable yet, even though its request
  // finished. Keep Enter waiting through the status read and refreshed search.
  const reconciling = !error && Boolean(library?.has_snapshot) && resultsQuery === queryKey && resultsRevision !== library?.revision;
  const pending = library === undefined || library?.state === "loading" || settled?.queryKey !== queryKey
    || settled.library !== library || settled.version !== libraryVersion || reconciling;
  return { queryKey, results, current, interpretation, list: current === results ? list : null, announcement, resultsQuery, error, pending };
}
