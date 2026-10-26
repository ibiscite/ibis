import { useCallback, useRef, useState, type MouseEvent } from "react";

/** A pointer that has not moved for this long is treated as still again. */
const STILL_AFTER_MS = 1000;

/**
 * Moving the mouse only shows hover; a click selects [C5], so reaching across
 * rows for Copy never changes what is copied (docs/design.md, "Hover"). A row
 * sliding under a still pointer (typing, scrolling, the picker reopening) does
 * not even show hover. Browsers repeat a mouse move at the same position when
 * content moves under the pointer, so only a change of screen position soon
 * after an earlier move counts.
 */
export function usePointerSelection(active: number, setActive: (index: number) => void) {
  const last = useRef<{ x: number; y: number; at: number } | null>(null);
  const [hoverAllowed, setHoverAllowed] = useState(false);
  const resetHover = useCallback(() => setHoverAllowed(false), []);
  const resetPointer = useCallback(() => { last.current = null; setHoverAllowed(false); }, []);
  // The row a click made active, until that row renders, so the list is not
  // scrolled under the pointer to reveal it. Other highlight changes that
  // render first, such as new results, must not use it up.
  const clicked = useRef<number | null>(null);
  const choose = (index: number) => {
    if (index === active) return;
    clicked.current = index;
    setActive(index);
  };
  /** Whether the highlight at `index` was clicked there, or a click is still
   * on its way to another row. Answers "click" only once per click. */
  const cameFrom = useCallback((index: number): "click" | "waiting" | "other" => {
    if (clicked.current === null) return "other";
    if (clicked.current !== index) return "waiting";
    clicked.current = null;
    return "click";
  }, []);
  const forgetClick = useCallback(() => { clicked.current = null; }, []);
  const onMouseMove = (event: MouseEvent<HTMLElement>) => {
    const previous = last.current;
    const now = { x: event.screenX, y: event.screenY, at: event.timeStamp };
    last.current = now;
    const moved = previous !== null && now.at - previous.at <= STILL_AFTER_MS
      && (previous.x !== now.x || previous.y !== now.y);
    if (!moved || !(event.target instanceof Element)) return;
    const row = event.target.closest('[role="option"][id^="result-"]');
    const index = row ? Number(row.id.slice("result-".length)) : Number.NaN;
    if (Number.isInteger(index) && index >= 0) setHoverAllowed(true);
  };
  return { onMouseMove, choose, cameFrom, forgetClick, hoverAllowed, resetHover, resetPointer };
}
