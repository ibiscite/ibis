import { useEffect, type RefObject } from "react";

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
      // A character typed now lands in Search by itself; any other key is
      // handed to Search as if it had been pressed there.
      if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      search.dispatchEvent(new KeyboardEvent("keydown", {
        key: event.key, code: event.code, bubbles: true, cancelable: true,
        ctrlKey: event.ctrlKey, metaKey: event.metaKey, altKey: event.altKey, shiftKey: event.shiftKey, repeat: event.repeat,
      }));
    };
    document.addEventListener("keydown", keyDown, true);
    return () => {
      document.removeEventListener("keydown", keyDown, true);
    };
  }, [input]);
}
