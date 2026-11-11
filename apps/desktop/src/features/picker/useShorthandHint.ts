import { useCallback, useEffect, useRef, useState } from "react";
import type { PickerApi } from "../../App";
import { useInterfaceMessages } from "../../i18n";

/** Teaches what can be typed with the search ("see vale 417") when a person
 * reaches for a citation's details by hand: on focusing a token, or opening
 * details a second time, in one opening. Native code allows a few on a computer. */
export function useShorthandHint(api: Pick<PickerApi, "shorthandHint">, presentation: number | null, detailsOpen: boolean) {
  const { text } = useInterfaceMessages();
  const [hint, setHint] = useState<string | null>(null);
  const asked = useRef<number | null>(null);
  const opened = useRef(0);
  const ask = useCallback(() => {
    if (presentation === null || asked.current === presentation || !api.shorthandHint) return;
    asked.current = presentation;
    void api.shorthandHint().then((show) => { if (show) setHint(text("picker-shorthand-hint")); }, () => undefined);
  }, [api, presentation, text]);
  // Each opening starts afresh; a hint never outlives the opening it was for.
  // biome-ignore lint/correctness/useExhaustiveDependencies: presentation is the trigger.
  useEffect(() => { setHint(null); opened.current = 0; }, [presentation]);
  useEffect(() => { if (detailsOpen && ++opened.current >= 2) ask(); }, [detailsOpen, ask]);
  useEffect(() => {
    const focused = (event: FocusEvent) => { if (event.target instanceof Element && event.target.closest(".citation-token")) ask(); };
    document.addEventListener("focusin", focused);
    return () => document.removeEventListener("focusin", focused);
  }, [ask]);
  return { hint, dismiss: useCallback(() => setHint(null), []) };
}
