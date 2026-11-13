import { useLayoutEffect, type RefObject } from "react";
import { settingControl } from "./settingFocus";

/** A search jump owns scroll/focus until presented or the person moves on.
 * Child controls may finish reading later; DOM changes wake this, never a poll. */
export function useRowReveal(target: { id: string; ticket: number } | null, scroller: RefObject<HTMLElement | null>) {
  useLayoutEffect(() => {
    const page = scroller.current;
    if (!target || !page) return;
    let stopped = false;
    const stop = () => { stopped = true; observer.disconnect(); };
    const reveal = () => {
      if (stopped) return;
      const label = document.getElementById(target.id);
      const row = label?.closest<HTMLElement>(".ui-row, tr, .ui-about-check") ?? label;
      if (!row || !page.contains(row) || row.closest("[hidden], [inert]") || row.getAttribute("aria-busy") === "true") return;
      const control = settingControl(row);
      // A disabled control is still loading or unavailable; it must not hand
      // search focus to its reset button instead.
      if (!control && row.querySelector("input, button, select")) return;
      const titleHeight = page.querySelector<HTMLElement>(".ui-settings-page-title")?.offsetHeight ?? 0;
      page.scrollTop += row.getBoundingClientRect().top - page.getBoundingClientRect().top - titleHeight;
      stop();
      if (control) control.focus({ preventScroll: true });
      else { row.tabIndex = -1; row.focus({ preventScroll: true }); }
    };
    const observer = new MutationObserver(reveal);
    observer.observe(page, { subtree: true, childList: true, attributes: true, attributeFilter: ["disabled", "hidden", "checked", "aria-busy"] });
    const key = (event: KeyboardEvent) => { if (!event.repeat) stop(); };
    document.addEventListener("focusin", stop);
    document.addEventListener("pointerdown", stop);
    document.addEventListener("keydown", key);
    reveal();
    return () => {
      stop();
      document.removeEventListener("focusin", stop);
      document.removeEventListener("pointerdown", stop);
      document.removeEventListener("keydown", key);
    };
  }, [target, scroller]);
}
