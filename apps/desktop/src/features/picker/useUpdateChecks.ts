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
  // What the last click asked for, so quick clicks each flip the one before.
  const wanted = useRef<boolean | null>(null);
  const show = (on: boolean | null) => { wanted.current = on; setAutomatic(on); };
  // Calls run one after another, so a read waits for the saves before it, and
  // only the newest one's answer is shown.
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const request = useRef(0);
  const ask = (operation: Operation, call: () => Promise<boolean | null>, failed?: () => void) => {
    const id = ++request.current;
    const run = queue.current.catch(() => undefined).then(call).then(
      (on) => { if (id === request.current) show(on); },
      (error: unknown) => { if (id === request.current) failed?.(); throw error; },
    );
    queue.current = run;
    void perform(operation, () => run);
  };
  const read = useRef(false);
  // biome-ignore lint/correctness/useExhaustiveDependencies: reads at start and each time More opens; ask is rebuilt every render.
  useEffect(() => {
    if (!updateChecks || (read.current && !menuOpen)) return;
    read.current = true;
    ask("updateChecks", updateChecks);
  }, [menuOpen]);
  const toggle = () => {
    const before = wanted.current;
    if (!setUpdateChecks || before === null) return;
    // Shown only once saved, so the menu never shows a choice that a quit
    // could still lose.
    wanted.current = !before;
    ask("updateChecksChange", () => setUpdateChecks(!before), () => { wanted.current = before; });
  };
  // The More menu's items, given how "Check for Updates…" runs there.
  return (check: (() => void) | undefined) =>
    setUpdateChecks && check && automatic !== null ? { automatic, toggle, check } : null;
}
