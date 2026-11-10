import { useLayoutEffect, useRef, type RefObject } from "react";

/** The selection glides: when the highlight moves within the same rows, the new
 * row's fill wipes in from the side it came from, in --dur-1 (ui/Option.css).
 * New rows never animate (nothing moves while typing), and under Reduce Motion
 * the stylesheet turns it off. */
export function useSelectionGlide(list: RefObject<HTMLElement | null>, active: number, rows: unknown) {
  const last = useRef<{ active: number; rows: unknown; top: number } | null>(null);
  const gliding = useRef<HTMLElement | null>(null);
  useLayoutEffect(() => {
    const row = list.current?.querySelector<HTMLElement>("[data-active]") ?? null;
    const before = last.current;
    last.current = row ? { active, rows, top: row.getBoundingClientRect().top } : null;
    if (gliding.current && gliding.current !== row) delete gliding.current.dataset.glide;
    if (!row || !before || before.rows !== rows || before.active === active) return;
    // Down: the fill comes from above, so it starts one row higher.
    row.style.setProperty("--glide-from", `${Math.sign(before.top - last.current!.top) * row.offsetHeight}px`);
    delete row.dataset.glide;
    void row.offsetWidth;
    row.dataset.glide = "";
    gliding.current = row;
  }, [list, active, rows]);
}
