import { createContext, useContext, useSyncExternalStore } from "react";

/** Whether the picker window is on screen, from its session (decision 0035).
 * Outside the picker, such as in Settings or a test of one part, it counts as shown. */
export const PickerShown = createContext(true);

const subscribe = (changed: () => void) => {
  document.addEventListener("visibilitychange", changed);
  return () => document.removeEventListener("visibilitychange", changed);
};
const documentVisible = () => document.visibilityState !== "hidden";

/** On screen and not hidden by the system: the one test for work that runs
 * only while shown. Parts below the picker read the session through context. */
export function usePickerShown(): boolean {
  return useShown(useContext(PickerShown));
}

/** The same test for the picker's own hooks, which sit above the context. */
export function useShown(shown: boolean): boolean {
  return useSyncExternalStore(subscribe, documentVisible) && shown;
}
