import { useEffect } from "react";
import { currentPlatform } from "../platform";
import "./HeldKeys.css";

export const HOLD_MS = 1000;

/** A control's key, in the control's own place while ⌘ (Ctrl elsewhere) is held:
 * it stands in for the control's words or glyph, as a numbered list's marker
 * does, so nothing moves and nothing else is covered. The control that owns the
 * shortcut declares it (Button's `keys`, a row's number); only a key that uses
 * the held key has a mark, so a token's ↓ shows none. A glyph-only button has no
 * room for words, so its key stays in its tooltip. */
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
