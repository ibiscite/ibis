import { useCallback, useRef, useState, type MouseEvent } from "react";

/** A pointer that has not moved for this long is treated as still again. */
const STILL_AFTER_MS = 1000;

/**
 * Qualified mouse movement enables hover styling only. A row sliding under a
 * still pointer (typing, scrolling or reopening) must not reclaim hover from
 * the keyboard. Selection belongs to explicit clicks and keyboard commands.
 */
export function usePointerSelection() {
  const last = useRef<{ x: number; y: number; at: number } | null>(null);
  const [hoverAllowed, setHoverAllowed] = useState(false);
  const resetHover = useCallback(() => setHoverAllowed(false), []);
  const resetPointer = useCallback(() => { last.current = null; setHoverAllowed(false); }, []);
  // Explicit row clicks mark their selection so scrolling does not move it
  // away from the pointer. Hover never marks or changes the selection.
  const fromPointer = useRef(false);
  const onMouseMove = (event: MouseEvent<HTMLElement>) => {
    const previous = last.current;
    const now = { x: event.screenX, y: event.screenY, at: event.timeStamp };
    last.current = now;
    const moved = previous !== null && now.at - previous.at <= STILL_AFTER_MS
      && (previous.x !== now.x || previous.y !== now.y);
    if (!moved || !(event.target instanceof Element)) return;
    if (!event.target.closest('[role="option"][id^="result-"]')) return;
    setHoverAllowed(true);
  };
  return { onMouseMove, fromPointer, hoverAllowed, resetHover, resetPointer };
}
