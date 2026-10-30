// Which picker operations last failed, for the notices; each succeeds or fails as a whole.
import { useCallback, useState } from "react";
import type { Operation } from "./Notices";

export function useOperations() {
  const [failures, setFailures] = useState<Partial<Record<Operation, boolean>>>({});
  // Stable, so effects can list them without running again on every render.
  const report = useCallback((operation: Operation, failed = true) => {
    setFailures((previous) => ({ ...previous, [operation]: failed }));
  }, []);
  const perform = useCallback(async (operation: Operation, action: () => Promise<unknown>, isCurrent?: () => boolean) => {
    let failed = false;
    try { await action(); }
    catch { failed = true; }
    // A completion left behind reports neither success nor failure.
    if (!isCurrent || isCurrent()) report(operation, failed);
  }, [report]);
  return { failures, setFailures, report, perform };
}
