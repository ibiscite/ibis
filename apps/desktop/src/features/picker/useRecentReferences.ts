import { useCallback, useEffect, useRef, useState } from "react";
import type * as nativeApi from "../../api";

/** The engine records native writes. The webview only requests current rows. */
export function useRecentReferences(recentReferences: typeof nativeApi.recentReferences | undefined, app: string | null, opening: number,
  onPreferencesChanged?: typeof nativeApi.onPreferencesChanged, onForget?: () => void) {
  const forgetting = useRef(onForget);
  forgetting.current = onForget;
  const epoch = useRef(0);
  const clearing = useRef<Promise<void> | null>(null);
  const [{ hasHistory, forgetFailed, revision }, setState] = useState({ hasHistory: false, forgetFailed: false, revision: 0 });
  const retire = useCallback(() => {
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
    if (!recentReferences) return true;
    clearing.current = recentReferences(true).then(() => {
      if (current === epoch.current) setState((old) => ({ ...old, forgetFailed: false }));
    }).catch(() => { if (current === epoch.current) setState((old) => ({ ...old, forgetFailed: true })); });
    return true;
  }, [recentReferences, retire]);
  useEffect(() => {
    if (!onPreferencesChanged) return;
    const stop = onPreferencesChanged((change) => {
      if (change?.recent_citations === false) retire();
    });
    return () => { void stop.then((unlisten) => unlisten()).catch(() => undefined); };
  }, [onPreferencesChanged, retire]);
  return { load: recentReferences ? load : undefined, forget: forgetFailed || hasHistory ? forget : null, forgetFailed };
}
