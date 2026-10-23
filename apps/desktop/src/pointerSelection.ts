import { useCallback, useRef, useState, type MouseEvent } from "react";

/** A pointer that has not moved for this long is treated as still again. */
const STILL_AFTER_MS = 1000;

/**
 * Moving the mouse selects the row under it; a row sliding under a still
 * pointer (typing, scrolling, the picker reopening) does not (docs/design.md,
 * "Hover"). Browsers repeat a mouse move at the same position when content
 * moves under the pointer, so only a change of screen position soon after an
 * earlier move counts. The wheel never selects.
 */
export function usePointerSelection(active: number, setActive: (index: number) => void) {
  const last = useRef<{ x: number; y: number; at: number } | null>(null);
  const [hoverAllowed, setHoverAllowed] = useState(false);
  const resetHover = useCallback(() => setHoverAllowed(false), []);
  const resetPointer = useCallback(() => { last.current = null; setHoverAllowed(false); }, []);
  // Set when the pointer changed the active row, so the list is not
  // scrolled under the pointer to reveal it.
  const fromPointer = useRef(false);
  const onMouseMove = (event: MouseEvent<HTMLElement>) => {
    const previous = last.current;
    const now = { x: event.screenX, y: event.screenY, at: event.timeStamp };
    last.current = now;
    const moved = previous !== null && now.at - previous.at <= STILL_AFTER_MS
      && (previous.x !== now.x || previous.y !== now.y);
    if (!moved || !(event.target instanceof Element)) return;
    const row = event.target.closest('[role="option"][id^="result-"]');
    const index = row ? Number(row.id.slice("result-".length)) : Number.NaN;
    if (!Number.isInteger(index) || index < 0) return;
    setHoverAllowed(true);
    if (index === active) return;
    fromPointer.current = true;
    setActive(index);
  };
  return { onMouseMove, fromPointer, hoverAllowed, resetHover, resetPointer };
}
