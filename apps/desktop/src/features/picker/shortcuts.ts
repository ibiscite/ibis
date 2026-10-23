// Which picker command a key press means. Pure; no React state.
import type React from "react";
import { commandKey, keysFor, matchesAccelerator } from "../../keys";
import { isMac } from "../../platform";

export { composing } from "../../composition";

/** Typing a character there enters text, so a bare `?` is not a command. */
export function typesText(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable || target instanceof HTMLTextAreaElement) return true;
  return target instanceof HTMLInputElement && !["button", "checkbox", "radio", "range", "color", "file", "submit", "reset"].includes(target.type);
}

/** `?` outside a text field, or Ctrl/⌘ with / (or ?) anywhere. Hebrew types /
 * on the Q key, so a press that is a letter shortcut is not this one. */
export function isKeysShortcut(e: React.KeyboardEvent, mac = isMac): boolean {
  if (e.altKey || isQuitShortcut(e, mac) || isMenuShortcut(e, mac) || isPinShortcut(e, mac) || isPreviewShortcut(e, mac)) return false;
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

/** Keep-open is an in-picker command; no system shortcut is registered. */
export function isPinShortcut(e: React.KeyboardEvent, mac = isMac): boolean {
  return matchesAccelerator(e, keysFor("pin"), mac);
}

/** Logical I with the platform command modifier; punctuation on the physical
 * I key must not become a command on another keyboard layout. */
export function isPreviewShortcut(e: React.KeyboardEvent, mac = isMac): boolean {
  return e.key.toLowerCase() === "i" && matchesAccelerator(e, keysFor("preview"), mac);
}
