/**
 * Records whether the last input was the keyboard or a pointer on
 * `<html data-input>`. Chromium (WebView2) matches `:focus-visible` on a
 * `<select>` opened with the mouse, unlike buttons, so the format control
 * uses this to keep its focus ring for keyboard use only (#533).
 */
const MODIFIERS = new Set(["Shift", "Control", "Alt", "Meta", "AltGraph", "CapsLock", "OS"]);

export function trackInputModality(root: HTMLElement = document.documentElement): () => void {
  root.dataset.input = "keyboard";
  const pointer = () => { root.dataset.input = "pointer"; };
  const keyboard = (event: KeyboardEvent) => { if (!MODIFIERS.has(event.key)) root.dataset.input = "keyboard"; };
  document.addEventListener("pointerdown", pointer, true);
  document.addEventListener("keydown", keyboard, true);
  return () => {
    document.removeEventListener("pointerdown", pointer, true);
    document.removeEventListener("keydown", keyboard, true);
  };
}
