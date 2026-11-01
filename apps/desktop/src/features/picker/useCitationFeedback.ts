import { useEffect } from "react";
import type { Copied } from "../../api";
import type { PickerPreview } from "../../App";
import type { SessionEvent } from "./session";

export function useCitationFeedback(copied: Copied | null, preview: PickerPreview | undefined, send: (event: SessionEvent) => void) {
  // Success is brief; warnings stay available until explicitly dismissed.
  useEffect(() => {
    if (preview || !copied || copied.warnings.length > 0) return;
    const timer = window.setTimeout(() => send({ type: "dismiss", copied }), 2400);
    return () => window.clearTimeout(timer);
  }, [copied, preview, send]);
}
