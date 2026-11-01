import { useEffect, useLayoutEffect, useRef, type RefObject } from "react";
import type { PickerApi } from "../../App";

/** What the Mac asks of the picker: text from Search with Ibis, collected once
 * so a launch cannot lose it, and the trackpad's tick as a citation grows. */
export function useNativeRequests(api: PickerApi, query: string, setQuery: (query: string) => void, input: RefObject<HTMLInputElement | null>, shelf: number) {
  const latest = useRef(setQuery);
  const incoming = useRef<string | null>(null);
  useLayoutEffect(() => {
    latest.current = setQuery;
    if (incoming.current !== query) return;
    // Selected, like any query on opening: typing replaces it.
    incoming.current = null;
    input.current?.select();
  });
  useEffect(() => {
    let current = true;
    const take = () => void api.takeSearchRequest?.().then((text) => {
      if (!current || !text) return;
      incoming.current = text;
      latest.current(text);
    }, () => undefined);
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
