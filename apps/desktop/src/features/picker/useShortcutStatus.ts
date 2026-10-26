// The global shortcut and its note. It can change while the picker is hidden
// (macOS follows the keyboard layout), so the picker reads it again each time it opens.
import { useCallback, useEffect, useRef, useState } from "react";
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
  // biome-ignore lint/correctness/useExhaustiveDependencies: subscribes once, like the picker's other events.
  useEffect(() => {
    mounted.current = true;
    // This setup's own flag: a subscription cleaned up by a remount must not
    // report or show anything once a newer one is live.
    let active = true;
    const unlisten = onShortcutStatus?.((status) => {
      reads.current += 1;
      if (active) setShortcut(status);
    }).catch(() => {
      if (active) report("pickerEvents");
      else console.error("Picker event subscription failed.");
      return () => undefined;
    });
    // Read once the change listener is in place, so a change in between is not lost.
    void (unlisten ?? Promise.resolve()).then(readShortcut);
    return () => {
      active = false;
      mounted.current = false;
      void unlisten?.then((stop) => stop()).catch(() => console.error("Picker event cleanup failed."));
    };
  }, []);
  return { shortcut, readShortcut };
}
