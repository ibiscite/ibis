import { useCallback, useEffect, useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { SessionState } from "./session";
import { readHistory, recentIds, rememberCitation } from "./recentHistory";

const storage = { getItem: (key: string) => window.localStorage.getItem(key), setItem: (key: string, value: string) => window.localStorage.setItem(key, value) };
export function useRecentReferences(recentReferences: typeof nativeApi.recentReferences | undefined, app: string | null, compact: boolean, copy: SessionState["recentCopy"], opening: number) {
  const [history, setHistory] = useState(() => readHistory(storage));
  const latest = useRef(history);
  latest.current = history;
  const [forgetFailed, setForgetFailed] = useState(false);
  const [forgotten, setForgotten] = useState(0);
  useEffect(() => {
    if (copy?.ids.length) setHistory((old) => rememberCitation(old, copy.app, copy.ids, storage));
  }, [copy]);
  // A receipt doesn't replace rows under a kept-open picker. Read the newest
  // identities on the next empty search/opening; forgetting refreshes now.
  // biome-ignore lint/correctness/useExhaustiveDependencies: opening and forgotten refresh the empty search while latest supplies its immutable request input.
  const load = useCallback(() => recentReferences!(recentIds(latest.current, app), !compact && latest.current.length > 0), [recentReferences, app, compact, opening, forgotten]);
  const forget = useCallback(() => {
    setHistory([]);
    latest.current = [];
    setForgotten((value) => value + 1);
    try { storage.setItem("ibis-citation-history", "[]"); setForgetFailed(false); return true; } catch { setForgetFailed(true); return false; }
  }, []);
  return { load: recentReferences ? load : undefined, forget: history.length || forgetFailed ? forget : null };
}
