import { useCallback, useEffect, useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { SessionState } from "./session";
import { readHistory, recentIds, rememberCitation } from "./recentHistory";

const storageKey = "ibis-citation-history";
const storage = { getItem: (key: string) => window.localStorage.getItem(key), setItem: (key: string, value: string) => window.localStorage.setItem(key, value) };
/** `enabled` is the saved Recent citations choice, null until read. Only a
 * known "on" records; a known "off" forgets, so turning it back on starts empty.
 * Every saved "off" forgets as it arrives, even when a quick "on" follows it
 * before the picker reads the settings again. */
export function useRecentReferences(recentReferences: typeof nativeApi.recentReferences | undefined, app: string | null, compact: boolean, copy: SessionState["recentCopy"], opening: number, enabled: boolean | null = true,
  onPreferencesChanged?: typeof nativeApi.onPreferencesChanged) {
  const [history, setHistory] = useState(() => readHistory(storage));
  const latest = useRef(history);
  latest.current = history;
  const recording = useRef(enabled);
  recording.current = enabled;
  // The newest saved choice a change event carried. Events arrive in save
  // order, so an "off" holds until a newer saved "on" replaces it, even while
  // the re-read that follows is still on its way or fails.
  const announced = useRef<boolean | null>(null);
  const [forgetFailed, setForgetFailed] = useState(false);
  const [forgotten, setForgotten] = useState(0);
  // Only a new receipt records. Turning the switch on, or Forget, never
  // replays the receipt that was current before.
  useEffect(() => {
    if (copy?.ids.length && recording.current === true && announced.current !== false) setHistory((old) => rememberCitation(old, copy.app, copy.ids, storage));
  }, [copy]);
  // A receipt doesn't replace rows under a kept-open picker. Read the newest
  // identities on the next empty search/opening; forgetting refreshes now.
  // biome-ignore lint/correctness/useExhaustiveDependencies: opening and forgotten refresh the empty search while latest supplies its immutable request input.
  const load = useCallback(() => recentReferences!(recentIds(latest.current, app), !compact && latest.current.length > 0), [recentReferences, app, compact, opening, forgotten]);
  const forget = useCallback(() => {
    setHistory([]);
    latest.current = [];
    setForgotten((value) => value + 1);
    try { storage.setItem(storageKey, "[]"); setForgetFailed(false); return true; } catch { setForgetFailed(true); return false; }
  }, []);
  useEffect(() => {
    if (enabled === false) forget();
  }, [enabled, forget]);
  useEffect(() => {
    if (!onPreferencesChanged) return;
    const stop = onPreferencesChanged((change) => {
      if (typeof change?.recent_citations !== "boolean") return;
      announced.current = change.recent_citations;
      if (!change.recent_citations) forget();
    });
    return () => { void stop.then((unlisten) => unlisten()).catch(() => undefined); };
  }, [onPreferencesChanged, forget]);
  return { load: recentReferences ? load : undefined, forget: enabled !== false && (history.length || forgetFailed) ? forget : null };
}
