import { useMemo, useRef, useState } from "react";
import type * as nativeApi from "../../api";

/** Observe the chooser's existing destination reads, without another IPC call.
 * The native policy supplies the intended action; final delivery may still refuse. */
export function useDeliveryTarget<T extends Partial<Pick<typeof nativeApi, "pickerDestination">>>(api: T) {
  const [canPaste, setCanPaste] = useState(false);
  const reads = useRef(0);
  const observed = useMemo(() => {
    const read = api.pickerDestination;
    return read ? { ...api, pickerDestination: async () => {
      const request = ++reads.current;
      setCanPaste(false);
      try {
        const target = await read();
        if (request === reads.current) setCanPaste(target.can_paste);
        return target;
      } catch (error) {
        if (request === reads.current) setCanPaste(false);
        throw error;
      }
    } } : api;
  }, [api]);
  return { api: observed, canPaste };
}
