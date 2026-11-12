import { useEffect, useState } from "react";
import { currentPlatform } from "../../platform";

export function useAltHeld() {
  // Option on macOS; Shift over the footer elsewhere, so capitals in Search
  // never change its actions. Any other key ends the alternate display.
  const [altHeld, setAltHeld] = useState(false);
  useEffect(() => {
    const mac = currentPlatform() === "macos", modifier = mac ? "Alt" : "Shift";
    let held = false, hovering = false;
    const show = () => setAltHeld(held && (mac || hovering));
    const down = (e: KeyboardEvent) => {
      if (e.repeat) return;
      held = e.key === modifier && !e.ctrlKey && !e.metaKey && (mac ? !e.shiftKey : !e.altKey);
      show();
    };
    const up = (e: KeyboardEvent) => { if (e.key === modifier || !(mac ? e.altKey : e.shiftKey)) { held = false; show(); } };
    const over = (e: PointerEvent) => {
      // A key released in another window may never send us keyup.
      held &&= mac ? e.altKey : e.shiftKey;
      hovering = e.target instanceof Element && e.target.closest("footer") !== null;
      show();
    };
    const out = (e: PointerEvent) => { hovering = e.relatedTarget instanceof Element && e.relatedTarget.closest("footer") !== null; show(); };
    const reset = () => { held = false; hovering = false; show(); };
    // Capture: a key the layer stack uses stops there, and still ends the hold.
    window.addEventListener("keydown", down, true);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", reset);
    window.addEventListener("pointerover", over);
    window.addEventListener("pointerout", out);
    return () => {
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", reset);
      window.removeEventListener("pointerover", over);
      window.removeEventListener("pointerout", out);
    };
  }, []);

  return { altHeld, setAltHeld };
}
