// What waits in the interface. Each timer belongs to the component that set
// it: a new start replaces it, and unmounting clears it, so none fires late.
import { useCallback, useEffect, useEffectEvent, useState } from "react";

/** Runs `run` once, `ms` after it is armed or `key` changes; null disarms it. */
export function useTimeout(run: () => void, ms: number | null, key?: unknown): void {
  const fire = useEffectEvent(run);
  // biome-ignore lint/correctness/useExhaustiveDependencies: key restarts the wait by design.
  useEffect(() => {
    if (ms === null) return;
    const timer = window.setTimeout(() => fire(), ms);
    return () => window.clearTimeout(timer);
  }, [ms, key]);
}

/** True once `on` has held for `ms` since it began or `key` last changed, such
 * as busy feedback that a fast answer never shows. */
export function useDelayed(on: boolean, ms: number, key?: unknown): boolean {
  const [done, setDone] = useState<{ key: unknown } | null>(null);
  if (!on && done) setDone(null);
  useTimeout(() => setDone({ key }), on ? ms : null, key);
  return on && done !== null && Object.is(done.key, key);
}

/** A value shown for `ms`, then cleared; showing it again restarts the time. */
export function useFlash<T = true>(ms: number): readonly [T | null, (value: T) => void, () => void] {
  const [shown, setShown] = useState<{ value: T } | null>(null);
  useTimeout(() => setShown(null), shown ? ms : null, shown);
  const show = useCallback((value: T) => setShown({ value }), []);
  const clear = useCallback(() => setShown(null), []);
  return [shown?.value ?? null, show, clear] as const;
}
