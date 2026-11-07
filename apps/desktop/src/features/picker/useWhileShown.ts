import { useEffect } from "react";
import { usePickerShown } from "./pickerShown";

/** Ask again every `interval` while this screen is shown: nothing runs while
 * the picker is hidden, and one request finishes before the next is timed. */
export function useWhileShown(ask: (() => Promise<unknown>) | undefined, interval: number) {
  const shown = usePickerShown();
  useEffect(() => {
    if (!ask || !shown) return;
    let timer: number | undefined;
    let stopped = false;
    const schedule = () => {
      if (stopped) return;
      timer = window.setTimeout(() => {
        // Checked again when it fires; showing the window again restarts this.
        if (document.visibilityState === "hidden") return;
        void ask().catch(() => {}).finally(schedule);
      }, interval);
    };
    schedule();
    return () => {
      stopped = true;
      window.clearTimeout(timer);
    };
  }, [ask, interval, shown]);
}
