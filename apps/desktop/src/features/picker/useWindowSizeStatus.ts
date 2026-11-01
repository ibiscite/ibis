import { useEffect } from "react";
import type { PickerApi } from "../../App";
import type { useOperations } from "./useOperations";

export function useWindowSizeStatus(onWindowSizeSaved: PickerApi["onWindowSizeSaved"], report: ReturnType<typeof useOperations>["report"]) {
  useEffect(() => {
    let current = true;
    const subscription = onWindowSizeSaved?.((saved) => { if (current) report("windowSize", !saved); });
    void subscription?.catch(() => { if (current) report("windowSize"); });
    return () => { current = false; void subscription?.then((stop) => stop()).catch(() => {}); };
  }, [onWindowSizeSaved, report]);
}
