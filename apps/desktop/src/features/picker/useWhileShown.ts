import { useEffect } from "react";

const pickerShown = () => document.visibilityState !== "hidden" && !document.documentElement.hasAttribute("data-picker-hidden");

/** Ask again every `interval` while this screen is shown: nothing runs while
 * the picker is hidden, and one request finishes before the next is timed. */
export function useWhileShown(ask: (() => Promise<unknown>) | undefined, interval: number) {
  useEffect(() => {
    if (!ask) return;
    let timer: number | undefined;
    let asking = false;
    let stopped = false;
    const schedule = () => {
      if (stopped || asking || timer !== undefined || !pickerShown()) return;
      timer = window.setTimeout(() => {
        timer = undefined;
        if (!pickerShown()) return;
        asking = true;
        void ask().catch(() => {}).finally(() => { asking = false; schedule(); });
      }, interval);
    };
    const sync = () => {
      if (pickerShown()) { schedule(); return; }
      window.clearTimeout(timer);
      timer = undefined;
    };
    const observer = new MutationObserver(sync);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-picker-hidden"] });
    document.addEventListener("visibilitychange", sync);
    sync();
    return () => {
      stopped = true;
      window.clearTimeout(timer);
      observer.disconnect();
      document.removeEventListener("visibilitychange", sync);
    };
  }, [ask, interval]);
}
