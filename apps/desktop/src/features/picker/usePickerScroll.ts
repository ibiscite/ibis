import { useLayoutEffect, useRef, type RefObject } from "react";

/** Preserve a person's browsing place across notices and scroll-owner changes.
 * Geometry here only anchors the current DOM rows; it never chooses a window
 * size, changes styles to measure content, or calls native sizing. */
export function usePickerScroll(main: RefObject<HTMLElement | null>) {
  const preservePlace = useRef(() => {});
  useLayoutEffect(() => {
    const node = main.current;
    if (!node) return;
    let current = true;
    let anchor: { row: HTMLElement; owner: HTMLElement; top: number; noticeHeight: number } | null = null;
    const update = () => {
      if (!current || main.current !== node || !node.isConnected) return;
      const area = node.querySelector<HTMLElement>(".results-area");
      // The first row anchors the whole unchanged list, even when offscreen.
      // Keep its DOM identity: an earlier search cannot move replacement rows.
      const row = area?.querySelector<HTMLElement>('[role="option"]');
      const owner = [area, area?.parentElement, node].find((part) => part && /auto|scroll/.test(getComputedStyle(part).overflowY));
      if (!row || !owner) { anchor = null; return; }
      const noticeHeight = [...node.querySelectorAll(":scope > .picker-content > .notices")].reduce((height, part) => height + part.getBoundingClientRect().height, 0);
      if (anchor?.row === row && (anchor.owner !== owner || anchor.noticeHeight !== noticeHeight)) {
        // CSS can clamp the former owner's offset as soon as notices change.
        // Transfer the visual position before paint, independently of selection.
        if (anchor.owner !== owner) anchor.owner.scrollTop = 0;
        owner.scrollTop += row.getBoundingClientRect().top - anchor.top;
      }
      anchor = { row, owner, top: row.getBoundingClientRect().top, noticeHeight };
    };
    const scrolled = (event: Event) => {
      if (event.target === anchor?.owner) update();
    };
    preservePlace.current = update;
    const resized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(update);
    const middle = node.querySelector(":scope > .picker-content");
    if (middle) resized?.observe(middle);
    window.addEventListener("resize", update);
    node.addEventListener("scroll", scrolled, true);
    return () => {
      current = false;
      resized?.disconnect();
      window.removeEventListener("resize", update);
      node.removeEventListener("scroll", scrolled, true);
      if (preservePlace.current === update) preservePlace.current = () => {};
    };
  }, [main]);
  useLayoutEffect(() => { preservePlace.current(); });
}
