import { useCallback, useEffect, useRef, useState } from "react";
import type { PickerApi } from "../../App";
import type { LibraryStatus } from "../../api";
import type { Operation } from "./Notices";

/** Events and reads share one request generation. Hiding retires pending
 * reads; reopening reads once. There is no library-status polling. */
export function useLibraryStatus(api: PickerApi, report: (operation: Operation, failed?: boolean) => void, cancelCopy: () => void) {
  const [library, setLibrary] = useState<LibraryStatus | null>();
  const [libraryVersion, setLibraryVersion] = useState(0);
  const generation = useRef(0);
  const mounted = useRef(false);
  const current = useRef({ api, report, cancelCopy });
  current.current = { api, report, cancelCopy };
  const refreshSnapshot = useCallback(() => {
    const request = ++generation.current;
    void current.current.api.libraryStatus().then((status) => {
      if (!mounted.current || request !== generation.current) return;
      setLibrary(status);
      setLibraryVersion((version) => version + 1);
      current.current.report("status", false);
    }).catch(() => {
      if (!mounted.current || request !== generation.current) return;
      setLibrary((old) => old === undefined ? null : old);
      current.current.cancelCopy();
      current.current.report("status");
    });
  }, []);
  const { onLibraryChanged, onPickerHidden, onPickerShown } = api;
  useEffect(() => {
    let alive = true;
    mounted.current = true;
    const subscribe = (operation: Operation) => () => { if (alive) current.current.report(operation); return () => {}; };
    const changed = onLibraryChanged((status) => {
      if (!alive) return;
      generation.current += 1;
      setLibrary(status);
      setLibraryVersion((version) => version + 1);
      current.current.report("status", false);
    }).catch(subscribe("libraryEvents"));
    const hidden = onPickerHidden(() => { if (alive) generation.current += 1; }).catch(subscribe("pickerEvents"));
    const shown = onPickerShown(() => { if (alive) refreshSnapshot(); }).catch(subscribe("pickerEvents"));
    refreshSnapshot();
    return () => {
      mounted.current = false;
      alive = false;
      generation.current += 1;
      void changed.then((stop) => stop()).catch(() => console.error("Library event cleanup failed."));
      for (const subscription of [hidden, shown]) void subscription.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
    };
  }, [onLibraryChanged, onPickerHidden, onPickerShown, refreshSnapshot]);
  return { library, libraryVersion, refreshSnapshot };
}
