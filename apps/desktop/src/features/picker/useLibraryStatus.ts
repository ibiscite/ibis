import type * as nativeApi from "../../api";
import { useCallback, useEffect, useRef, useState } from "react";
import type { LibraryStatus } from "../../api";
import type { Operation } from "./Notices";
import { useNativeEvent } from "../../nativeEvents";

/** Events and reads share one request generation. Hiding retires pending
 * reads. Opening supplies a complete picker snapshot. */
export function useLibraryStatus(api: typeof nativeApi, report: (operation: Operation, failed?: boolean) => void, cancelCopy: () => void) {
  const [library, setLibrary] = useState<LibraryStatus | null>();
  // A reload is not a new library: search and citations keep this one, and
  // the version stays, until it ends. Only the footer follows the live status.
  const [settledLibrary, setSettledLibrary] = useState<LibraryStatus | null>();
  const [libraryVersion, setLibraryVersion] = useState(0);
  const publish = useCallback((status: LibraryStatus) => {
    setLibrary(status);
    if (status.state === "reloading") {
      setSettledLibrary((settled) => settled ?? status);
      return;
    }
    setSettledLibrary(status);
    setLibraryVersion((version) => version + 1);
  }, []);
  const generation = useRef(0);
  const mounted = useRef(false);
  const current = useRef({ api, report, cancelCopy });
  current.current = { api, report, cancelCopy };
  const refreshSnapshot = useCallback(() => {
    const request = ++generation.current;
    void current.current.api.libraryStatus().then((status) => {
      if (!mounted.current || request !== generation.current) return;
      publish(status);
      current.current.report("status", false);
    }).catch(() => {
      if (!mounted.current || request !== generation.current) return;
      setLibrary((old) => old === undefined ? null : old);
      setSettledLibrary((old) => old === undefined ? null : old);
      current.current.cancelCopy();
      current.current.report("status");
    });
  }, [publish]);
  const { onLibraryChanged, onPickerHidden } = api;
  useNativeEvent(onLibraryChanged, (status) => {
    generation.current += 1;
    publish(status);
    report("status", false);
  }, { failed: () => report("libraryEvents"), subject: "Library" });
  useNativeEvent(onPickerHidden, () => { generation.current += 1; }, { failed: () => report("pickerEvents"), subject: "Picker" });
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      generation.current += 1;
    };
  }, []);
  const acceptSnapshot = (status: LibraryStatus) => {
    generation.current += 1;
    setLibrary(status);
    setSettledLibrary(status);
    setLibraryVersion((version) => version + 1);
    report("status", false);
  };
  const snapshotReceiver = () => {
    const request = generation.current;
    return (status: LibraryStatus) => { if (request === generation.current) acceptSnapshot(status); };
  };
  return { library, settledLibrary, libraryVersion, refreshSnapshot, snapshotReceiver };
}
