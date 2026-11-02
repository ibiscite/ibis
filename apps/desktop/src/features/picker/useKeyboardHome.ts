import { useEffect, type RefObject } from "react";
import { composing } from "../../composition";

const lost = () => document.activeElement === null || document.activeElement === document.body;

/**
 * The keyboard never rests on the page itself. A click on a blank part of
 * the picker, or a menu that closes after its trigger went away, would leave
 * focus there, outside every handler: typing and Escape would do nothing.
 * The next key moves focus to Search and reaches it. Focus is not moved
 * before then: a closing menu may still be returning it to its trigger.
 */
export function useKeyboardHome(input: RefObject<HTMLInputElement | null>) {
  useEffect(() => {
    const keyDown = (event: KeyboardEvent) => {
      const search = input.current;
      if (!lost() || !search || event.defaultPrevented) return;
      search.focus();
      // An input method composes into Search by itself.
      if (composing(event) || event.key === "Process") return;
      // Search's handlers get the key as if it were pressed there. Its native
      // effect (text, paste, caret movement) then also happens in Search,
      // unless one of them consumes the key.
      event.stopImmediatePropagation();
      const delivered = search.dispatchEvent(new KeyboardEvent("keydown", {
        key: event.key, code: event.code, location: event.location, bubbles: true, cancelable: true,
        ctrlKey: event.ctrlKey, metaKey: event.metaKey, altKey: event.altKey, shiftKey: event.shiftKey, repeat: event.repeat,
        modifierAltGraph: event.getModifierState("AltGraph"),
      }));
      if (!delivered) event.preventDefault();
    };
    document.addEventListener("keydown", keyDown, true);
    return () => {
      document.removeEventListener("keydown", keyDown, true);
    };
  }, [input]);
}
