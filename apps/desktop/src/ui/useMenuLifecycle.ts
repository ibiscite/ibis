import { useCallback, useLayoutEffect, useRef, useState } from "react";

/** Restart a retained collection for each new opening. */
export function useMenuLifecycle(open: boolean) {
  const list = useRef<HTMLDivElement | null>(null);
  const closingList = useRef<HTMLDivElement | null>(null);
  const [opening, setOpening] = useState(0);
  const listRef = useCallback((node: HTMLDivElement | null) => {
    list.current = node;
    if (!node) closingList.current = null;
  }, []);
  useLayoutEffect(() => {
    if (!open) closingList.current = list.current;
    else {
      // Reused content already autofocused; restart it for the new opening's
      // pointer/keyboard strategy, including clearing the previous row/filter.
      if (closingList.current && closingList.current === list.current) setOpening((value) => value + 1);
      closingList.current = null;
    }
  }, [open]);
  return { list, listRef, opening };
}
