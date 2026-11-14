import { useCallback, useLayoutEffect, useState } from "react";

type Register<Slot extends string> = (slot: Slot, on: boolean) => void;

/** Layout belongs to the owner; mounted slots report their presence before paint. */
export function usePresence<Slot extends string>(initial: Record<Slot, number>) {
  const [counts, setCounts] = useState(initial);
  const register = useCallback<Register<Slot>>((slot, on) => {
    setCounts((counts) => ({ ...counts, [slot]: counts[slot] + (on ? 1 : -1) }));
  }, []);
  return { counts, register };
}

export function usePresenceSlot<Slot extends string>(register: Register<Slot> | undefined, slot: Slot, present = true) {
  useLayoutEffect(() => {
    if (!register || !present) return;
    register(slot, true);
    return () => register(slot, false);
  }, [register, slot, present]);
}
