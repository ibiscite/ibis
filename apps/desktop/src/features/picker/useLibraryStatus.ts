import type * as nativeApi from "../../api";
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { LibraryStatus } from "../../api";
import { latest } from "../../latest";
import type { Operation } from "./Notices";
import { useNativeEvent } from "../../nativeEvents";

type Library = {
  library: LibraryStatus | null | undefined;
  /** A reload is not a new library: search and citations keep this one, and
   * the version stays, until it ends. Only the footer follows the live status. */
  settledLibrary: LibraryStatus | null | undefined;
  libraryVersion: number;
};

const shown = (status: LibraryStatus) => (old: Library): Library => status.state === "reloading"
  ? { ...old, library: status, settledLibrary: old.settledLibrary ?? status }
  : { library: status, settledLibrary: status, libraryVersion: old.libraryVersion + 1 };
/** A complete picker snapshot settles even a reload. */
const settled = (status: LibraryStatus) => (old: Library): Library => ({ library: status, settledLibrary: status, libraryVersion: old.libraryVersion + 1 });
const unread = (old: Library): Library => ({ ...old, library: old.library ?? null, settledLibrary: old.settledLibrary ?? null });

/** One owner for reads, events and picker snapshots: the newest begun wins.
 * Hiding and leaving retire pending reads. */
export function useLibraryStatus(api: typeof nativeApi, report: (operation: Operation, failed?: boolean) => void, cancelCopy: () => void) {
  const [owner] = useState(() => latest<Library>({ library: undefined, settledLibrary: undefined, libraryVersion: 0 }));
  const { library, settledLibrary, libraryVersion } = useSyncExternalStore(owner.subscribe, owner.getSnapshot);
  const current = useRef({ api, report, cancelCopy });
  current.current = { api, report, cancelCopy };
  const refreshSnapshot = useCallback(() => {
    void owner.run(() => current.current.api.libraryStatus(), (old, outcome) => outcome.ok ? shown(outcome.value)(old) : unread(old), (outcome) => {
      if (outcome.ok) return current.current.report("status", false);
      current.current.cancelCopy();
      current.current.report("status");
    });
  }, [owner]);
  const { onLibraryChanged, onPickerHidden } = api;
  useNativeEvent(onLibraryChanged, (status) => {
    owner.invalidate(shown(status));
    report("status", false);
  }, { failed: () => report("libraryEvents"), subject: "Library" });
  useNativeEvent(onPickerHidden, () => owner.invalidate(), { failed: () => report("pickerEvents"), subject: "Picker" });
  useEffect(() => () => owner.invalidate(), [owner]);
  const snapshotReceiver = () => {
    const stamp = owner.begin();
    return (status: LibraryStatus) => stamp.commit(settled(status), () => current.current.report("status", false));
  };
  return { library, settledLibrary, libraryVersion, refreshSnapshot, snapshotReceiver };
}
