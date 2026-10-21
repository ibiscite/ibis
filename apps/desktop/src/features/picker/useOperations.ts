// Which picker operations last failed, for the notices; each succeeds or fails as a whole.
import { useState } from "react";
import type { Operation } from "./Notices";

export function useOperations() {
  const [failures, setFailures] = useState<Partial<Record<Operation, boolean>>>({});
  const report = (operation: Operation, failed = true) => {
    setFailures((previous) => ({ ...previous, [operation]: failed }));
  };
  const perform = async (operation: Operation, action: () => Promise<unknown>) => {
    try { await action(); report(operation, false); }
    catch { report(operation); }
  };
  return { failures, setFailures, report, perform };
}
