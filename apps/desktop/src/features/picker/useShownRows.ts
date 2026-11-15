import { useLayoutEffect, useState, type RefObject } from "react";

/** The result rows wholly in view: the first one's index and how many, at most
 * nine. ⌘1–9 (Ctrl elsewhere) numbers them from the top of the view, so a
 * number always names a row a person can see. */
export function useShownRows(area: RefObject<HTMLElement | null>, results: readonly unknown[]): { first: number; count: number } {
  const [shown, setShown] = useState({ first: 0, count: Math.min(results.length, 9) });
  // biome-ignore lint/correctness/useExhaustiveDependencies: new results are new rows to measure.
  useLayoutEffect(() => {
    const node = area.current;
    if (!node) return;
    const scroller = node.parentElement ?? node;
    let frame = 0;
    const measure = () => {
      frame = 0;
      const box = node.getBoundingClientRect(), outer = scroller.getBoundingClientRect();
      const top = Math.max(box.top, outer.top) - 1, bottom = Math.min(box.bottom, outer.bottom) + 1;
      const rows = [...node.querySelectorAll<HTMLElement>("#results [role='option']")];
      const first = rows.findIndex((row) => row.getBoundingClientRect().top >= top);
      const after = first < 0 ? -1 : rows.slice(first).findIndex((row) => row.getBoundingClientRect().bottom > bottom);
      const count = first < 0 ? 0 : Math.min(9, after < 0 ? rows.length - first : after);
      setShown((current) => current.first === first && current.count === count ? current : { first: Math.max(first, 0), count });
    };
    const later = () => { if (!frame) frame = requestAnimationFrame(measure); };
    measure();
    scroller.addEventListener("scroll", later, true);
    // Density, text size, the shelf and the window all change what is in view.
    const resized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(later);
    resized?.observe(node); resized?.observe(scroller);
    return () => { cancelAnimationFrame(frame); scroller.removeEventListener("scroll", later, true); resized?.disconnect(); };
  }, [area, results]);
  return shown;
}
