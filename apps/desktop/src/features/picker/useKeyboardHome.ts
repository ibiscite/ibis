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
      // Text typed now, also with Option or AltGr or through an input method,
      // lands in Search by itself.
      const shortcut = (event.ctrlKey || event.metaKey) && !(event.ctrlKey && event.altKey);
      if (event.isComposing || event.key === "Process" || event.key.length === 1 && !shortcut) return;
      // Any other key is handed to Search as if pressed there. A native edit
      // (paste, undo, delete) still happens in Search, so it keeps its default.
      if (!shortcut && event.key !== "Backspace" && event.key !== "Delete") event.preventDefault();
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
