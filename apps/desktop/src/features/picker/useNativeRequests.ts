import { useEffect, useLayoutEffect, useRef, type RefObject } from "react";
import type { PickerApi } from "../../App";

/** What the Mac asks of the picker: text from Search with Ibis, collected once
 * so a launch cannot lose it, and the trackpad's tick as a citation grows. */
export function useNativeRequests(api: PickerApi, query: string, setQuery: (query: string) => void, input: RefObject<HTMLInputElement | null>, shelf: number) {
  const latest = useRef(setQuery);
  const typed = useRef(query);
  const incoming = useRef<string | null>(null);
  useLayoutEffect(() => {
    latest.current = setQuery;
    typed.current = query;
    if (incoming.current !== query) return;
    // Selected, like any query on opening: typing replaces it.
    incoming.current = null;
    input.current?.select();
  });
  useEffect(() => {
    let current = true, newest = 0;
    const take = () => {
      const request = ++newest, asked = typed.current;
      void api.takeSearchRequest?.().then((text) => {
        // Only the newest reply counts, and never over what was typed meanwhile.
        if (!current || !text || request !== newest || typed.current !== asked) return;
        incoming.current = text;
        latest.current(text);
      }, () => undefined);
    };
    take();
    const subscription = api.onSearchRequested?.(take);
    return () => {
      current = false;
      void subscription?.then((unlisten) => unlisten());
    };
  }, [api]);
  const previous = useRef(shelf);
  useEffect(() => {
    if (shelf > previous.current) void api.alignmentTick?.().catch(() => undefined);
    previous.current = shelf;
  }, [api, shelf]);
}
