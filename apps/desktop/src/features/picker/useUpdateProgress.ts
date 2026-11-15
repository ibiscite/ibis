import { useEffect, useState } from "react";
import * as nativeApi from "../../api";
import { useNativeEvent } from "../../nativeEvents";
import type { useOperations } from "./useOperations";

export function useUpdateProgress(api: typeof nativeApi, setFailures: ReturnType<typeof useOperations>["setFailures"]) {
  const [updateProgress, setUpdateProgress] = useState<nativeApi.UpdateProgress | null>(null);
  useEffect(() => {
    setFailures((old) => old.updateEvents ? { ...old, updateEvents: false } : old);
  }, [setFailures]);
  useNativeEvent(api.onUpdateProgress, (next) => setUpdateProgress((old) => nativeApi.newerUpdate(old, next)), {
    failed: () => setFailures((old) => ({ ...old, updateEvents: true })), subject: "Update",
  });
  return updateProgress;
}
