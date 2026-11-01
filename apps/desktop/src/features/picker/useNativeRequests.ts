import { useEffect, useRef } from "react";
import type { PickerApi } from "../../App";

/** What the Mac asks of the picker: text from Search with Ibis, collected once
 * so a launch cannot lose it, and the trackpad's tick as a citation grows. */
export function useNativeRequests(api: PickerApi, setQuery: (query: string) => void, shelf: number) {
  useEffect(() => {
    let current = true;
    const take = () => void api.takeSearchRequest?.().then((text) => { if (current && text) setQuery(text); });
    take();
    const subscription = api.onSearchRequested?.(take);
    return () => {
      current = false;
      void subscription?.then((unlisten) => unlisten());
    };
  }, [api, setQuery]);
  const previous = useRef(shelf);
  useEffect(() => {
    if (shelf > previous.current) void api.alignmentTick?.();
    previous.current = shelf;
  }, [api, shelf]);
}
