import type * as nativeApi from "../../api";
import { useCallback, useEffect, useLayoutEffect, useRef, type RefObject } from "react";
import { useNativeEvent } from "../../nativeEvents";

/** What the Mac asks of the picker: text from Search with Ibis, collected once
 * so a launch cannot lose it, and the trackpad's tick as a citation grows. */
export function useNativeRequests(api: typeof nativeApi, query: string, setQuery: (query: string) => void, input: RefObject<HTMLInputElement | null>, shelf: number) {
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
  const newest = useRef(0);
  const take = useCallback(() => {
    const request = ++newest.current, asked = typed.current;
    void api.takeSearchRequest?.().then((text) => {
      // Only the newest reply counts, and never over what was typed meanwhile.
      if (!text || request !== newest.current || typed.current !== asked) return;
      incoming.current = text;
      latest.current(text);
    }, () => undefined);
  }, [api]);
  useEffect(() => {
    take();
    return () => { newest.current += 1; };
  }, [take]);
  useNativeEvent(api.onSearchRequested, take);
  const previous = useRef(shelf);
  useEffect(() => {
    if (shelf > previous.current) void api.alignmentTick?.().catch(() => undefined);
    previous.current = shelf;
  }, [api, shelf]);
}
