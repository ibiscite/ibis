import { useEffect, useState } from "react";

export function useAltHeld() {
  // While Alt (⌥) is held, the footer shows the other keys instead of Enter.
  const [altHeld, setAltHeld] = useState(false);
  useEffect(() => {
    // Only Alt on its own, or Alt still down from that, counts; any other key ends it.
    const down = (e: KeyboardEvent) => setAltHeld((held) => e.key === "Alt" || (e.altKey && held));
    const up = (e: KeyboardEvent) => { if (e.key === "Alt" || !e.altKey) setAltHeld(false); };
    const reset = () => setAltHeld(false);
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", reset);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", reset);
    };
  }, []);

  return { altHeld, setAltHeld };
}
