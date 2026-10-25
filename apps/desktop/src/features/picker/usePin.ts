// Whether the picker stays open beside the document, with native calls kept in click order.
import { useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { useOperations } from "./useOperations";

export function usePin({ setPinned, perform }: {
  setPinned: typeof nativeApi.setPinned;
  perform: ReturnType<typeof useOperations>["perform"];
}) {
  const [pinned, setPinnedState] = useState(false);
  // Read by handlers that outlive a render, such as a copy finishing.
  const currentPin = useRef(pinned);
  currentPin.current = pinned;
  const confirmedPin = useRef(false);
  const pinRequest = useRef(0);
  const pinWork = useRef(Promise.resolve());
  const togglePin = () => {
    const on = !currentPin.current;
    currentPin.current = on;
    setPinnedState(on);
    const request = ++pinRequest.current;
    // Preserve native request order, and roll back only the latest request to
    // the last confirmed state. An older failure cannot undo a newer choice.
    pinWork.current = pinWork.current.then(() => perform("pin", async () => {
      try { await setPinned(on); confirmedPin.current = on; }
      catch (error: unknown) {
        if (request === pinRequest.current) {
          currentPin.current = confirmedPin.current;
          setPinnedState(confirmedPin.current);
        }
        throw error;
      }
    }));
  };
  return { pinned, currentPin, togglePin };
}
