import type * as nativeApi from "../../api";
import { useEffect, useLayoutEffect, useRef, type RefObject } from "react";

/** What the Mac asks of the picker: text from Search with Ibis, collected once
 * so a launch cannot lose it, and the trackpad's tick as a citation grows. */
export function useNativeRequests(api: typeof nativeApi, query: string, setQuery: (query: string) => void, input: RefObject<HTMLInputElement | null>, shelf: number) {
  const latest = useRef(setQuery);
  const typed = useRef(query);
  const edits = useRef(0);
  const incoming = useRef<string | null>(null);
  useLayoutEffect(() => {
    latest.current = setQuery;
    if (typed.current !== query) edits.current += 1;
    typed.current = query;
    if (incoming.current !== query) return;
    // Selected, like any query on opening: typing replaces it.
    incoming.current = null;
    input.current?.select();
  });
  useEffect(() => {
    let current = true, taking = false;
    let requested: number | null = edits.current;
    const take = async () => {
      if (taking) return;
      taking = true;
      // Notifications only wake this consumer; the native slot owns the query.
      while (current && requested !== null) {
        const asked = requested;
        requested = null;
        const text = await api.takeSearchRequest?.().catch(() => null);
        if (!current || !text || edits.current !== asked) continue;
        incoming.current = text;
        typed.current = text;
        latest.current(text);
      }
      taking = false;
    };
    const subscription = api.onSearchRequested?.(() => {
      requested = edits.current;
      void take();
    });
    // Subscribe before consuming so a request cannot fall between the two.
    void subscription?.then(() => { if (current) void take(); }, () => undefined);
    return () => {
      current = false;
      void subscription?.then((unlisten) => unlisten(), () => undefined);
    };
  }, [api]);
  const previous = useRef(shelf);
  useEffect(() => {
    if (shelf > previous.current) void api.alignmentTick?.().catch(() => undefined);
    previous.current = shelf;
  }, [api, shelf]);
}
