import { useEffect, useState } from "react";
import * as nativeApi from "../../api";
import type { PickerApi } from "../../App";
import type { useOperations } from "./useOperations";

export function useUpdateProgress(api: PickerApi, setFailures: ReturnType<typeof useOperations>["setFailures"]) {
  const [updateProgress, setUpdateProgress] = useState<nativeApi.UpdateProgress | null>(null);
  useEffect(() => {
    let mounted = true;
    setFailures((old) => old.updateEvents ? { ...old, updateEvents: false } : old);
    const subscription = api.onUpdateProgress?.((next) => {
      if (mounted) setUpdateProgress((old) => nativeApi.newerUpdate(old, next));
    }).catch(() => {
      if (mounted) setFailures((old) => ({ ...old, updateEvents: true }));
      return null;
    });
    return () => {
      mounted = false;
      void subscription?.then((stop) => stop?.()).catch(() => console.error("Update event cleanup failed."));
    };
  }, [api, setFailures]);
  return updateProgress;
}
