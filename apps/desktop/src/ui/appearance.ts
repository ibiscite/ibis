import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type * as nativeApi from "../api";
import type { PickerPreferences } from "../bindings";

type Appearance = Pick<PickerPreferences, "appearance" | "accent">;
type PreferencesApi = Pick<typeof nativeApi, "pickerPreferences"> & Partial<Pick<typeof nativeApi, "onPreferencesChanged">>;

/** One root-level application rule for both windows; native/system colours stay intact. */
export function useAppearance(preferences: Appearance | null) {
  const appearance = preferences?.appearance ?? "system", accent = preferences?.accent ?? "system";
  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.appearance = appearance;
    root.dataset.accent = accent;
    return () => { delete root.dataset.appearance; delete root.dataset.accent; };
  }, [appearance, accent]);
}

/** Observe the picker's existing preference I/O. No second read, event subscription
 * or polling owner: its queue still owns reading and saving. The picker cannot
 * change Appearance or Accent, so only successful reads publish them; an unrelated
 * save reply must never replace a newer Settings notification. */
export function usePickerAppearance<T extends PreferencesApi>(api: T): T {
  const [preferences, setPreferences] = useState<Appearance | null>(null);
  const operation = useRef(0), live = useRef(true);
  useAppearance(preferences);
  useEffect(() => {
    live.current = true;
    return () => { live.current = false; operation.current += 1; };
  }, []);
  return useMemo(() => ({
    ...api,
    ...(api.onPreferencesChanged ? { onPreferencesChanged: (handler: Parameters<typeof nativeApi.onPreferencesChanged>[0]) => api.onPreferencesChanged!((change) => {
      // The queue reserves this refresh before it waits for pending saves.
      // Retire older appearance reads at notification time, before queued I/O.
      operation.current += 1;
      handler(change);
    }) } : {}),
    pickerPreferences: async () => {
      const request = ++operation.current;
      const saved = await api.pickerPreferences();
      if (live.current && request === operation.current) setPreferences(saved);
      return saved;
    },
  }), [api]);
}
