import { useCallback, useEffect, useRef, useState } from "react";
import type { PickerApi } from "../../App";
import { pickerText } from "../../messages";

/** The native geometry owner owns the snapshot. This hook owns only which
 * immutable ticket is visible; old reads and actions cannot consume a new one. */
export function useWindowResetUndo(api: PickerApi) {
  const { readWindowPositionUndo, undoWindowPosition, dismissWindowPositionUndo, onWindowReset } = api;
  const [ticket, setTicket] = useState<number | null>(null);
  const current = useRef<number | null>(null);
  const retired = useRef(0);
  const latest = useRef(0);
  const publish = useCallback((value: number | null) => {
    if (value !== null && value <= retired.current) return;
    current.current = value;
    setTicket(value);
  }, []);
  useEffect(() => {
    if (!onWindowReset || !readWindowPositionUndo) return;
    let mounted = true, events = 0;
    const subscription = onWindowReset((state) => {
      events += 1;
      if (!mounted || state.revision < latest.current) return;
      latest.current = state.revision;
      if (state.undo === null && current.current !== null) retired.current = Math.max(retired.current, current.current);
      publish(state.undo);
    });
    void subscription.then(async () => {
      const before = events;
      const value = await readWindowPositionUndo();
      if (mounted && before === events && (value === null || value >= latest.current)) {
        if (value !== null) latest.current = value;
        publish(value);
      }
    }).catch(() => {});
    return () => { mounted = false; void subscription.then((stop) => stop()).catch(() => {}); };
  }, [onWindowReset, readWindowPositionUndo, publish]);
  const expire = useCallback((value: number | null) => {
    if (value === null || current.current !== value) return;
    retired.current = Math.max(retired.current, value);
    publish(null);
    void dismissWindowPositionUndo?.(value).catch(() => {});
  }, [dismissWindowPositionUndo, publish]);
  const undo = useCallback(async (value: number | null) => {
    if (value === null || current.current !== value || !undoWindowPosition) return;
    const restored = await undoWindowPosition(value);
    if (current.current !== value) return;
    if (!restored) throw new Error(pickerText.undoFailed);
    retired.current = Math.max(retired.current, value);
    publish(null);
  }, [undoWindowPosition, publish]);
  return { ticket, undo: useCallback(() => undo(ticket), [undo, ticket]), expire: useCallback(() => expire(ticket), [expire, ticket]) };
}
