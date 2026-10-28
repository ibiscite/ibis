// The tray's update controls in More, for desktops without a tray (#1130).
// Builds without updates answer null and offer neither.
import { useEffect, useState } from "react";
import type * as nativeApi from "../../api";
import type { useOperations } from "./useOperations";

export function useUpdateChecks({ updateChecks, setUpdateChecks, onUpdateChecks, updateAvailable, onUpdateAvailable, perform, report }: {
  updateChecks?: typeof nativeApi.updateChecks;
  setUpdateChecks?: typeof nativeApi.setUpdateChecks;
  onUpdateChecks?: typeof nativeApi.onUpdateChecks;
  updateAvailable?: typeof nativeApi.updateAvailable;
  onUpdateAvailable?: typeof nativeApi.onUpdateAvailable;
  perform: ReturnType<typeof useOperations>["perform"];
  report: ReturnType<typeof useOperations>["report"];
}) {
  // Only saved choices are shown. A click is queued natively with the tray's
  // clicks, and the saved answer arrives as an event, whichever menu asked.
  const [automatic, setAutomatic] = useState<boolean | null>(null);
  const [available, setAvailable] = useState(false);
  useEffect(() => {
    if (!updateAvailable || !onUpdateAvailable) return;
    let active = true, heard = false;
    const stop = onUpdateAvailable((pending) => { if (active) { heard = true; setAvailable(pending); } });
    stop.then(updateAvailable).then(
      (pending) => { if (active && !heard) setAvailable(pending); },
      () => { if (active) report("updateChecks"); },
    );
    return () => {
      active = false;
      void stop.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, [updateAvailable, onUpdateAvailable, report]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: reads once at start; later changes arrive as events.
  useEffect(() => {
    if (!updateChecks || !onUpdateChecks) return;
    let active = true, heard = false;
    const stop = onUpdateChecks((on) => { if (active) { heard = true; setAutomatic(on); } });
    // Read after listening, so no change is missed; an event is newer than
    // any read answer.
    stop.then(updateChecks).then(
      (on) => { if (active && !heard) setAutomatic(on); },
      () => { if (active) report("updateChecks"); },
    );
    return () => {
      active = false;
      void stop.then((unlisten) => unlisten()).catch(() => undefined); // Subscription failures are handled above; teardown has no active view for cleanup errors.
    };
  }, []);
  // Asks for the opposite of what is shown, never a flip of whatever the
  // native side holds by then, which a tray change may have overtaken.
  const toggle = () => {
    if (setUpdateChecks && automatic !== null) void perform("updateChecksChange", () => setUpdateChecks(!automatic));
  };
  // The More menu's items, given how "Check for Updates…" runs there.
  return (check: (() => void) | undefined) =>
    setUpdateChecks && check && automatic !== null ? { automatic, available, toggle, check } : null;
}
