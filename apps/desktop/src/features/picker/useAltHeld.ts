import { useEffect, useState } from "react";

export function useAltHeld() {
  // While Alt (⌥) is held, the footer shows the other keys instead of Enter.
  const [altHeld, setAltHeld] = useState(false);
  useEffect(() => {
    // Only Alt on its own, or Alt still down from that, counts; any other key ends it.
    const down = (e: KeyboardEvent) => setAltHeld((held) => e.key === "Alt" || (e.altKey && held));
    const up = (e: KeyboardEvent) => { if (e.key === "Alt" || !e.altKey) setAltHeld(false); };
    const reset = () => setAltHeld(false);
    // Capture: a key the layer stack uses stops there, and still ends the hold.
    window.addEventListener("keydown", down, true);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", reset);
    return () => {
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", reset);
    };
  }, []);

  return { altHeld, setAltHeld };
}
