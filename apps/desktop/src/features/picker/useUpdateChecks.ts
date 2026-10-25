// The tray's update controls in More, for desktops without a tray (#1130).
// Builds without updates answer null and offer neither.
import { useEffect, useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { Operation } from "./Notices";
import type { useOperations } from "./useOperations";

export function useUpdateChecks({ updateChecks, setUpdateChecks, perform, menuOpen }: {
  updateChecks?: typeof nativeApi.updateChecks;
  setUpdateChecks?: typeof nativeApi.setUpdateChecks;
  perform: ReturnType<typeof useOperations>["perform"];
  /** Read at start, so the items are there when More opens, and again each
   * time it opens, since the tray item changes it too. */
  menuOpen: boolean;
}) {
  const [automatic, setAutomatic] = useState<boolean | null>(null);
  // Only the newest answer is shown, so a slow read cannot undo a click.
  const request = useRef(0);
  const ask = (operation: Operation, call: () => Promise<boolean | null>) => {
    const id = ++request.current;
    void perform(operation, async () => {
      const on = await call();
      if (id === request.current) setAutomatic(on);
    });
  };
  const read = useRef(false);
  // biome-ignore lint/correctness/useExhaustiveDependencies: reads at start and each time More opens; ask is rebuilt every render.
  useEffect(() => {
    if (!updateChecks || (read.current && !menuOpen)) return;
    read.current = true;
    ask("updateChecks", updateChecks);
  }, [menuOpen]);
  const toggle = () => {
    if (setUpdateChecks && automatic !== null) ask("updateChecksChange", () => setUpdateChecks(!automatic));
  };
  return { automatic: automatic ?? false, toggle, offered: Boolean(setUpdateChecks) && automatic !== null };
}
