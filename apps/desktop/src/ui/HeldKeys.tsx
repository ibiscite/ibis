import { useEffect } from "react";
import { currentPlatform } from "../platform";
import "./HeldKeys.css";

export const HOLD_MS = 1000;

/** A row's digit alone in its lead column while ⌘ (Ctrl elsewhere) is held.
 * Controls keep their words and glyphs; their keys are already printed. */
export function KeyMark({ keys }: { keys?: string }) {
  if (!keys?.split("+").includes("CmdOrCtrl")) return null;
  return <span className="ui-key-mark" aria-hidden="true"><bdi dir="ltr">{keys.split("+").at(-1)}</bdi></span>;
}

/** Holding ⌘ (Ctrl elsewhere) alone for a second shows every KeyMark. Any other
 * key, a click, letting go or leaving the window ends it. */
export function HeldKeys() {
  useEffect(() => {
    const held = currentPlatform() === "macos" ? "Meta" : "Control";
    let timer: number | undefined;
    const end = () => { window.clearTimeout(timer); timer = undefined; delete document.documentElement.dataset.keysHeld; };
    const down = (event: KeyboardEvent) => {
      if (event.repeat) return;
      end();
      if (event.key === held) timer = window.setTimeout(() => { document.documentElement.dataset.keysHeld = ""; }, HOLD_MS);
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
  return null;
}
