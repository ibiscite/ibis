import { useEffect, useState } from "react";
import { currentPlatform } from "../../platform";

export function useAltHeld(mode: "picker" | "option" = "picker") {
  // Option on macOS; Shift over the footer elsewhere, so capitals in Search
  // never change its actions. About keeps Option/Alt on every system.
  // Any other key ends the alternate display.
  const [altHeld, setAltHeld] = useState(false);
  useEffect(() => {
    const option = mode === "option" || currentPlatform() === "macos", modifier = option ? "Alt" : "Shift";
    let held = false, hovering = false;
    const show = () => setAltHeld(held && (option || hovering));
    const down = (e: KeyboardEvent) => {
      if (e.repeat) return;
      held = e.key === modifier && !e.ctrlKey && !e.metaKey && (option ? !e.shiftKey : !e.altKey);
      show();
    };
    const up = (e: KeyboardEvent) => { if (e.key === modifier || !(option ? e.altKey : e.shiftKey)) { held = false; show(); } };
    const over = (e: PointerEvent) => {
      // A key released in another window may never send us keyup.
      held &&= option ? e.altKey : e.shiftKey;
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
  }, [mode]);

  return { altHeld, setAltHeld };
}
