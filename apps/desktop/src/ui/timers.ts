// What waits in the interface. Each timer belongs to the component that set
// it: a new start replaces it, and unmounting clears it, so none fires late.
import { useCallback, useEffect, useEffectEvent, useRef, useState } from "react";

/** Runs once after `ms` active milliseconds. A new duration or identity starts
 * fresh; null disarms it. Pausing keeps this identity's remaining time. */
export function useTimeout(run: () => void, ms: number | null, key?: unknown, paused = false): void {
  const fire = useEffectEvent(run);
  const budget = useRef<{ ms: number; key: unknown; remaining: number; fired: boolean } | null>(null);
  useEffect(() => {
    if (ms === null) { budget.current = null; return; }
    if (!budget.current || budget.current.ms !== ms || !Object.is(budget.current.key, key)) budget.current = { ms, key, remaining: ms, fired: false };
    const current = budget.current;
    if (paused || current.fired) return;
    const start = performance.now();
    const timer = window.setTimeout(() => { current.fired = true; fire(); }, current.remaining);
    return () => {
      window.clearTimeout(timer);
      current.remaining = Math.max(0, current.remaining - (performance.now() - start));
    };
  }, [ms, key, paused]);
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
