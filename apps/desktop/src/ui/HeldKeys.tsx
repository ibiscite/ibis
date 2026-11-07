import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { keyFromAria } from "../keys";
import { currentPlatform } from "../platform";
import { KeyCaps } from "./KeyCaps";
import "./HeldKeys.css";

export const HOLD_MS = 1000;

/** A badge sits beside its control, never on it: above, or below where the window has no room above. */
type Shown = { key: string; x: number; top: number; bottom: number; side: "above" | "below" };

/** Controls that show no key of their own, read when the hold completes. */
function keyedControls(): Shown[] {
  return [...document.querySelectorAll<HTMLElement>("[aria-keyshortcuts]")].flatMap((control) => {
    const box = control.getBoundingClientRect(), shortcut = control.getAttribute("aria-keyshortcuts")!.split(" ")[0]!;
    if (control.querySelector("kbd") || !box.width || !box.height) return [];
    return [{ key: keyFromAria(shortcut), x: box.left + box.width / 2, top: box.top, bottom: box.bottom, side: "above" as const }];
  });
}

/** Holding ⌘ (Ctrl elsewhere) alone for a second shows every visible action's
 * key. Any other key, a click, letting go or leaving the window ends it. It
 * never takes focus or a click. */
export function HeldKeys() {
  const [shown, setShown] = useState<Shown[]>([]);
  const badges = useRef<(HTMLSpanElement | null)[]>([]);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => {
    const held = currentPlatform() === "macos" ? "Meta" : "Control";
    const end = () => { window.clearTimeout(timer.current); timer.current = undefined; setShown([]); };
    const down = (event: KeyboardEvent) => {
      if (event.repeat) return;
      end();
      if (event.key === held) timer.current = window.setTimeout(() => setShown(keyedControls()), HOLD_MS);
    };
    window.addEventListener("keydown", down, true);
    window.addEventListener("keyup", end, true);
    window.addEventListener("pointerdown", end, true);
    window.addEventListener("blur", end);
    return () => {
      end();
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("keyup", end, true);
      window.removeEventListener("pointerdown", end, true);
      window.removeEventListener("blur", end);
    };
  }, []);
  // A badge that would leave the window's top goes under its control instead, before it is painted.
  useLayoutEffect(() => {
    const flip = shown.map((badge, i) => badge.side === "above" && (badges.current[i]?.getBoundingClientRect().top ?? 0) < 0);
    if (flip.some(Boolean)) setShown(shown.map((badge, i) => flip[i] ? { ...badge, side: "below" } : badge));
  }, [shown]);
  return <>{shown.map(({ key, x, top, bottom, side }, i) => (
    <span className="ui-held-key" key={`${key}-${x}-${top}`} ref={(node) => { badges.current[i] = node; }} aria-hidden="true"
      data-side={side} style={{ left: x, top: side === "above" ? top : bottom }}><KeyCaps accelerator={key} /></span>
  ))}</>;
}
