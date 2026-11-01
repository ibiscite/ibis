import { useEffect, useRef, useState } from "react";
import { keyFromAria } from "../../keys";
import { currentPlatform } from "../../platform";
import { KeyCaps } from "../../ui/KeyCaps";
import "./HeldKeys.css";

export const HOLD_MS = 1000;

type Shown = { key: string; x: number; y: number };

/** Controls that show no key of their own, read when the hold completes. */
function keyedControls(): Shown[] {
  return [...document.querySelectorAll<HTMLElement>("[aria-keyshortcuts]")].flatMap((control) => {
    const box = control.getBoundingClientRect(), shortcut = control.getAttribute("aria-keyshortcuts")!.split(" ")[0]!;
    if (control.querySelector("kbd") || !box.width || !box.height) return [];
    return [{ key: keyFromAria(shortcut), x: box.left + box.width / 2, y: box.top + box.height / 2 }];
  });
}

/** Holding ⌘ (Ctrl elsewhere) alone for a second shows every visible action's
 * key. Any other key, a click, letting go or leaving the window ends it. It
 * never takes focus or a click. */
export function HeldKeys() {
  const [shown, setShown] = useState<Shown[]>([]);
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
  return <>{shown.map(({ key, x, y }) => (
    <span className="held-key" key={`${key}-${x}-${y}`} aria-hidden="true" style={{ insetInlineStart: x, insetBlockStart: y }}><KeyCaps accelerator={key} /></span>
  ))}</>;
}
