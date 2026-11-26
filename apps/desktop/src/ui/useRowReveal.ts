import { useLayoutEffect, type RefObject } from "react";
import { settingControl } from "./settingFocus";

/** How long a revealed row is kept in view while what is above it finishes loading. */
export const REVEAL_SETTLE_MS = 1500;

/** A search jump owns scroll/focus until presented or the person moves on.
 * Child controls may finish reading later; DOM changes wake this, never a poll.
 * Once presented, the row stays where it landed while content above it still
 * loads (a list read late pushes it down), for a short while and only until the
 * person scrolls, points, types or moves focus. */
export function useRowReveal(target: { id: string; ticket: number } | null, scroller: RefObject<HTMLElement | null>) {
  useLayoutEffect(() => {
    const page = scroller.current;
    if (!target || !page) return;
    let phase: "waiting" | "settling" | "done" = "waiting";
    let focused: HTMLElement | null = null;
    let timer: number | undefined;
    const find = () => {
      const label = document.getElementById(target.id);
      const row = label?.closest<HTMLElement>(".ui-row, tr, .ui-about-check") ?? label;
      return row && page.contains(row) && !row.closest("[hidden], [inert]") ? row : null;
    };
    const align = (row: HTMLElement) => {
      const titleHeight = page.querySelector<HTMLElement>(".ui-settings-page-title")?.offsetHeight ?? 0;
      const offset = row.getBoundingClientRect().top - page.getBoundingClientRect().top - titleHeight;
      if (Math.abs(offset) >= 1) page.scrollTop += offset;
    };
    const take = (row: HTMLElement, control: HTMLElement | null) => {
      focused = control ?? row;
      if (!control) row.tabIndex = -1;
      focused.focus({ preventScroll: true });
    };
    const stop = () => { phase = "done"; observer.disconnect(); resized?.disconnect(); window.clearTimeout(timer); };
    const reveal = () => {
      if (phase === "done") return;
      const row = find();
      if (phase === "settling") {
        if (!row) return;
        align(row);
        // The row was drawn again (its list finished reading): the keyboard follows it.
        if (focused && !focused.isConnected) take(row, settingControl(row));
        return;
      }
      if (!row || row.getAttribute("aria-busy") === "true") return;
      const control = settingControl(row);
      // A disabled control is still loading or unavailable; it must not hand
      // search focus to its reset button instead.
      if (!control && row.querySelector("input, button, select")) return;
      align(row);
      phase = "settling";
      timer = window.setTimeout(stop, REVEAL_SETTLE_MS);
      take(row, control);
    };
    const observer = new MutationObserver(reveal);
    observer.observe(page, { subtree: true, childList: true, attributes: true, attributeFilter: ["disabled", "hidden", "checked", "aria-busy"] });
    // Sizes change without the DOM changing too (a font or picture arriving).
    const resized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(reveal);
    for (const child of page.children) resized?.observe(child);
    // Our own focus and scrolling never count as the person moving on.
    const moved = (event: Event) => { if (phase === "waiting" || event.target !== focused) stop(); };
    const key = (event: KeyboardEvent) => { if (!event.repeat) stop(); };
    const leave = ["pointerdown", "wheel", "touchmove"] as const;
    document.addEventListener("focusin", moved);
    for (const type of leave) document.addEventListener(type, stop, { passive: true });
    document.addEventListener("keydown", key);
    reveal();
    return () => {
      stop();
      document.removeEventListener("focusin", moved);
      for (const type of leave) document.removeEventListener(type, stop);
      document.removeEventListener("keydown", key);
    };
  }, [target, scroller]);
}
