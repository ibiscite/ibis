import type * as nativeApi from "../../api";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useInterfaceMessages } from "../../i18n";

/** Teaches what can be typed with the search ("see vale 417") when a person
 * reaches for a citation's details by hand: on focusing a token, or opening
 * details a second time, in one opening. Native code allows a few on a computer. */
export function useShorthandHint(api: Pick<typeof nativeApi, "shorthandHint">, presentation: number | null, detailsOpen: boolean) {
  const { text } = useInterfaceMessages();
  const [hint, setHint] = useState<string | null>(null);
  const asked = useRef<number | null>(null);
  const opened = useRef(0);
  const opening = useRef<object | null>(null);
  useLayoutEffect(() => {
    opening.current = presentation === null ? null : {};
    setHint(null);
    opened.current = 0;
    return () => { opening.current = null; };
  }, [presentation]);
  const { shorthandHint } = api;
  const ask = useCallback(() => {
    if (presentation === null || asked.current === presentation) return;
    asked.current = presentation;
    const owner = opening.current;
    void shorthandHint().then((show) => {
      if (show && opening.current === owner) setHint(text("picker-shorthand-hint"));
    }, () => undefined);
  }, [shorthandHint, presentation, text]);
  useEffect(() => { if (detailsOpen && ++opened.current >= 2) ask(); }, [detailsOpen, ask]);
  useEffect(() => {
    const focused = (event: FocusEvent) => { if (event.target instanceof Element && event.target.closest(".citation-token")) ask(); };
    document.addEventListener("focusin", focused);
    return () => document.removeEventListener("focusin", focused);
  }, [ask]);
  return { hint, dismiss: useCallback(() => setHint(null), []) };
}
