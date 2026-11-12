import { useLayoutEffect, useRef } from "react";
import { flushSync } from "react-dom";
import { isBackShortcut } from "../keys";
import { composingKey, compositionInProgress, consumeKey } from "./layers";

/** Only the nearest view takes Back. A pop-up or an input method keeps its keys. */
export function useBackNavigation(onBack: () => void, ownsKeys: () => boolean) {
  const latest = useRef({ onBack, ownsKeys });
  useLayoutEffect(() => { latest.current = { onBack, ownsKeys }; });
  useLayoutEffect(() => {
    const down = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !latest.current.ownsKeys() || composingKey(event)) return;
      if (!isBackShortcut(event)) return;
      consumeKey(event);
      if (!event.repeat) flushSync(latest.current.onBack);
    };
    const mouse = (event: MouseEvent) => {
      if (event.button !== 3 || event.defaultPrevented || !latest.current.ownsKeys() || compositionInProgress()) return;
      event.preventDefault(); event.stopPropagation();
      flushSync(latest.current.onBack);
    };
    window.addEventListener("keydown", down, true);
    window.addEventListener("mousedown", mouse, true);
    return () => {
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("mousedown", mouse, true);
    };
  }, []);
}
