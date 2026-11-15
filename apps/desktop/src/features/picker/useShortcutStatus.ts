// The global shortcut and its note. It can change while the picker is hidden
// (macOS follows the keyboard layout), so the picker reads it again each time it opens.
import { useCallback, useEffect, useRef, useState } from "react";
import { useNativeEvent } from "../../nativeEvents";
import type * as nativeApi from "../../api";
import type { ShortcutStatus } from "../../api";
import type { useOperations } from "./useOperations";

export function useShortcutStatus({ shortcutStatus, onShortcutStatus, report }: {
  shortcutStatus: typeof nativeApi.shortcutStatus;
  onShortcutStatus?: typeof nativeApi.onShortcutStatus;
  report: ReturnType<typeof useOperations>["report"];
}) {
  const [shortcut, setShortcut] = useState<ShortcutStatus | null>(null);
  const mounted = useRef(true);
  // Only the newest read or change is shown.
  const reads = useRef(0);
  const readShortcut = useCallback(() => {
    const read = ++reads.current;
    shortcutStatus()
      .then((status) => { if (mounted.current && read === reads.current) setShortcut(status); })
      .catch(() => { if (mounted.current && read === reads.current) { setShortcut(null); report("shortcut"); } });
  }, [shortcutStatus, report]);
  useEffect(() => {
    mounted.current = true;
    if (!onShortcutStatus) readShortcut();
    return () => { mounted.current = false; };
  }, [onShortcutStatus, readShortcut]);
  // Read once the change listener is in place, so a change in between is not lost.
  useNativeEvent(onShortcutStatus, (status) => {
    reads.current += 1;
    setShortcut(status);
  }, { ready: readShortcut, failed: () => { report("pickerEvents"); readShortcut(); }, subject: "Picker" });
  return { shortcut, readShortcut };
}
