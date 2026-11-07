import { useCallback, useEffect, useRef, useState } from "react";
import type * as nativeApi from "../../api";

function clearLegacy() {
  try { localStorage.removeItem("ibis-citation-history"); return false; } catch { return true; }
}

/** The engine records native writes. The webview only requests current rows. */
export function useRecentReferences(recentReferences: typeof nativeApi.recentReferences | undefined, app: string | null, opening: number,
  onPreferencesChanged?: typeof nativeApi.onPreferencesChanged, onForget?: () => void) {
  const forgetting = useRef(onForget);
  forgetting.current = onForget;
  const epoch = useRef(0);
  const clearing = useRef<Promise<void> | null>(null);
  const [{ hasHistory, forgetFailed, revision }, setState] = useState({ hasHistory: false, forgetFailed: false, revision: 0 });
  const [legacyFailed, setLegacyFailed] = useState(clearLegacy);
  const retire = useCallback(() => {
    setLegacyFailed(clearLegacy());
    epoch.current++;
    forgetting.current?.();
    setState((old) => ({ ...old, hasHistory: false, revision: old.revision + 1 }));
  }, []);
  // biome-ignore lint/correctness/useExhaustiveDependencies: destination and opening refresh native history.
  const load = useCallback(async () => {
    const current = epoch.current;
    await clearing.current;
    const [reply, hasHistory, failed] = await recentReferences!(false);
    if (current !== epoch.current) return { ...reply, results: [] };
    setState((old) => ({ ...old, hasHistory, forgetFailed: failed }));
    return reply;
  }, [recentReferences, app, opening, revision]);
  const forget = useCallback(() => {
    retire();
    const current = epoch.current;
    if (!recentReferences) return;
    clearing.current = recentReferences(true).then(() => {
      if (current === epoch.current) setState((old) => ({ ...old, forgetFailed: false }));
    }).catch(() => { if (current === epoch.current) setState((old) => ({ ...old, forgetFailed: true })); });
  }, [recentReferences, retire]);
  useEffect(() => {
    if (!onPreferencesChanged) return;
    const stop = onPreferencesChanged((change) => {
      // Only a save that set the switch: the rest of a snapshot may be older.
      if (change?.recent_citations === false && change.fields?.includes("recent_citations")) retire();
    });
    return () => { void stop.then((unlisten) => unlisten()).catch(() => undefined); };
  }, [onPreferencesChanged, retire]);
  return { load: recentReferences ? load : undefined, forget: forgetFailed || legacyFailed || hasHistory ? forget : null, forgetFailed: forgetFailed || legacyFailed };
}
