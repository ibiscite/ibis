import { useCallback, useEffect, useEffectEvent, useRef } from "react";

/** Reads cannot overtake changes; keyed changes may finish independently. */
export function useSettingsRequests<T>(load: () => Promise<T>, loaded: (value: T) => void, failed: () => void) {
  const sequence = useRef(0);
  const pending = useRef(new Map<string, number>());
  const live = useRef(false);
  const accept = useEffectEvent(loaded);
  const reject = useEffectEvent(failed);
  useEffect(() => {
    live.current = true;
    return () => { live.current = false; sequence.current += 1; pending.current.clear(); };
  }, []);
  const isPending = useCallback(() => pending.current.size > 0, []);
  const begin = useCallback((key?: string) => {
    const ticket = ++sequence.current;
    if (key !== undefined) pending.current.set(key, ticket);
    const current = () => live.current && (key === undefined ? sequence.current === ticket : pending.current.get(key) === ticket);
    return { current, finish: () => {
      if (key === undefined || !current()) return;
      pending.current.delete(key);
    } };
  }, []);
  const read = useCallback((): void => {
    if (!live.current || isPending()) return;
    const request = begin();
    void load().then((value) => { if (request.current()) accept(value); }, () => { if (request.current()) reject(); });
  }, [load, begin, isPending]);
  return { read, begin, isPending };
}
