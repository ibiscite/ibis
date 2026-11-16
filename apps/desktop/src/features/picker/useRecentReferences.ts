import { useCallback, useRef, useState } from "react";
import { useNativeEvent } from "../../nativeEvents";
import type * as nativeApi from "../../api";

function eraseWebviewHistory() {
  try { localStorage.removeItem("ibis-citation-history"); return false; } catch { return true; }
}

/** The engine records native writes. The webview only requests current rows. */
export function useRecentReferences(recentReferences: typeof nativeApi.recentReferences | undefined, app: string | null, opening: number,
  onPreferencesChanged?: typeof nativeApi.onPreferencesChanged, onForget?: () => void, destination?: nativeApi.PickerDestination | null) {
  const [snapshot, setSnapshot] = useState<{ revision: string; reply: Awaited<ReturnType<typeof nativeApi.search>> | null; destination: nativeApi.PickerDestination | null }>();
  const acceptSnapshot = (value: nativeApi.PickerSnapshot["recents"], revision: string, destination: nativeApi.PickerDestination | null) => {
    epoch.current++;
    const next = "Ok" in value ? value.Ok : null;
    setSnapshot({ revision, reply: next?.[0] ?? null, destination });
    setState((old) => ({ ...old, hasHistory: next?.[1] ?? false, forgetFailed: next?.[2] ?? false, revision: old.revision + 1 }));
  };
  const forgetting = useRef(onForget);
  forgetting.current = onForget;
  const epoch = useRef(0);
  const clearing = useRef<Promise<void> | null>(null);
  const [{ hasHistory, forgetFailed, revision }, setState] = useState({ hasHistory: false, forgetFailed: false, revision: 0 });
  const [erasureFailed, setErasureFailed] = useState(eraseWebviewHistory);
  const retire = useCallback(() => {
    setErasureFailed(eraseWebviewHistory());
    epoch.current++;
    setSnapshot(undefined);
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
  useNativeEvent(onPreferencesChanged, (change) => {
    // Only a save that set the switch: the rest of a snapshot may be older.
    if (change?.recent_citations === false && change.fields?.includes("recent_citations")) retire();
  });
  const snapshotReceiver = () => {
    const request = epoch.current;
    return (...value: Parameters<typeof acceptSnapshot>) => { if (request === epoch.current) acceptSnapshot(...value); };
  };
  return { snapshot: snapshot?.destination === destination ? snapshot : undefined, snapshotReceiver, load: recentReferences ? load : undefined, forget: forgetFailed || erasureFailed || hasHistory ? forget : null, forgetFailed: forgetFailed || erasureFailed };
}
