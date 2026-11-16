import { useLayoutEffect } from "react";
import type { PickerPreferences } from "../bindings";

type Appearance = Pick<PickerPreferences, "appearance" | "accent">;

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
