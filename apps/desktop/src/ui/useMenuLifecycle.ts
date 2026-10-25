import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

/** Share retained-collection restart and the owned focus-retry frame. Each
 * menu keeps its own pending-target, action and command validity checks. */
export function useMenuLifecycle(open: boolean) {
  const list = useRef<HTMLDivElement | null>(null);
  const closingList = useRef<HTMLDivElement | null>(null);
  const [opening, setOpening] = useState(0);
  const focusFrame = useRef(0);
  const listRef = useCallback((node: HTMLDivElement | null) => {
    list.current = node;
    if (!node) closingList.current = null;
  }, []);
  const retryFocus = useCallback((restore: () => void) => { focusFrame.current = requestAnimationFrame(restore); }, []);
  const cancelFocusRetry = useCallback(() => { cancelAnimationFrame(focusFrame.current); }, []);
  useLayoutEffect(() => {
    if (!open) closingList.current = list.current;
    else {
      // Reused content already autofocused; restart it for the new opening's
      // pointer/keyboard strategy, including clearing the previous row/filter.
      if (closingList.current && closingList.current === list.current) setOpening((value) => value + 1);
      closingList.current = null;
      cancelFocusRetry();
    }
  }, [open, cancelFocusRetry]);
  useEffect(() => cancelFocusRetry, [cancelFocusRetry]);
  return { list, listRef, opening, retryFocus, cancelFocusRetry };
}
