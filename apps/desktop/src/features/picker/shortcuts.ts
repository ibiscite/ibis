// Which picker command a key press means. Pure; no React state.
import type React from "react";
import { commandKey, keysFor, matchesAccelerator } from "../../keys";
import { isMac } from "../../platform";

/** True while an input method is composing text; Enter then confirms the text.
 * Some WebKit versions report the confirming Enter with key code 229 only. */
export function composing(e: React.KeyboardEvent): boolean {
  return e.nativeEvent.isComposing || e.keyCode === 229;
}

/** Typing a character there enters text, so a bare `?` is not a command. */
export function typesText(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable || target instanceof HTMLTextAreaElement) return true;
  return target instanceof HTMLInputElement && !["button", "checkbox", "radio", "range", "color", "file", "submit", "reset"].includes(target.type);
}

/** `?` outside a text field, or Ctrl/⌘ with / (or ?) anywhere. */
export function isKeysShortcut(e: React.KeyboardEvent, mac = isMac): boolean {
  if (e.altKey) return false;
  if ((e.key === "/" || e.key === "?") && commandKey(e, mac)) return true;
  return e.key === "?" && !e.ctrlKey && !e.metaKey && !typesText(e.target);
}

/** The More menu shortcut from the key table: ⌘K on macOS, Ctrl+K elsewhere. */
export function isMenuShortcut(e: React.KeyboardEvent, mac = isMac): boolean {
  return matchesAccelerator(e, keysFor("more"), mac);
}

/** The quit shortcut from the key table: ⌘Q on macOS, Ctrl+Q elsewhere. */
export function isQuitShortcut(e: React.KeyboardEvent, mac = isMac): boolean {
  return matchesAccelerator(e, keysFor("quit"), mac);
}
