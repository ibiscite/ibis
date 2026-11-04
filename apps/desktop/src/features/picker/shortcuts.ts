// Which picker command a key press means. Pure; no React state.
import { commandKey, keysFor, matchesAccelerator, type KeyPress } from "../../keys";
import { isMac } from "../../platform";

/** Everything the picker does by name. A key and its menu item run the same one. */
export type PickerCommand = "format" | "more" | "pin" | "source" | "revealSource" | "chooseSource" | "keys" | "settings" | "quit";

type Press = KeyPress & Pick<KeyboardEvent, "target" | "getModifierState">;

/** Typing a character there enters text, so a bare `?` is not a command. */
export function typesText(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable || target instanceof HTMLTextAreaElement) return true;
  return target instanceof HTMLInputElement && !["button", "checkbox", "radio", "range", "color", "file", "submit", "reset"].includes(target.type);
}

/** `?` outside a text field, or Ctrl/⌘ with / (or ?) anywhere. Hebrew types /
 * on the Q key, so a press that is a letter shortcut is not this one. */
export function isKeysShortcut(e: Press, mac = isMac, focused: EventTarget | null = e.target): boolean {
  if (e.altKey || isQuitShortcut(e, mac) || isMenuShortcut(e, mac) || isPinShortcut(e, mac) || isSettingsShortcut(e, mac)) return false;
  if ((e.key === "/" || e.key === "?") && commandKey(e, mac)) return true;
  return e.key === "?" && !e.ctrlKey && !e.metaKey && !typesText(focused);
}

/** The More menu shortcut from the key table: ⌘K on macOS, Ctrl+K elsewhere. */
export function isMenuShortcut(e: Press, mac = isMac): boolean {
  return matchesAccelerator(e, keysFor("more"), mac);
}

/** The quit shortcut from the key table: ⌘Q on macOS, Ctrl+Q elsewhere. */
export function isQuitShortcut(e: Press, mac = isMac): boolean {
  return matchesAccelerator(e, keysFor("quit"), mac);
}

/** Keep-open is an in-picker command; no system shortcut is registered. */
export function isPinShortcut(e: Press, mac = isMac): boolean {
  return matchesAccelerator(e, keysFor("pin"), mac);
}

/** Logical I with the platform command modifier; punctuation on the physical
 * I key must not become a command on another keyboard layout. */

/** Settings, as in every Mac app: ⌘, (Ctrl+, elsewhere). */
export function isSettingsShortcut(e: Press, mac = isMac): boolean {
  return matchesAccelerator(e, keysFor("settings"), mac);
}

/** The output chooser has one mnemonic in-picker shortcut. */
export function isFormatShortcut(e: Press, mac = isMac): boolean {
  return matchesAccelerator(e, keysFor("format"), mac);
}

export function isSourceShortcut(e: Press, mac = isMac): boolean {
  if (e.getModifierState("AltGraph")) return false;
  return matchesAccelerator(e, e.altKey ? "Alt+CmdOrCtrl+O" : keysFor("source"), mac);
}

export function isChooseSourceShortcut(e: Press, mac = isMac): boolean {
  return matchesAccelerator(e, keysFor("chooseSource"), mac);
}

/** The command a key press means, wherever focus is. `focused` decides whether
 * a bare `?` is text; a key pressed with focus lost goes to Search. */
export function commandFor(e: Press, focused: EventTarget | null, mac = isMac): PickerCommand | null {
  if (isChooseSourceShortcut(e, mac)) return "chooseSource";
  if (isSourceShortcut(e, mac)) return e.altKey ? "revealSource" : "source";
  if (isFormatShortcut(e, mac)) return "format";
  if (isPinShortcut(e, mac)) return "pin";
  if (isKeysShortcut(e, mac, focused)) return "keys";
  if (isMenuShortcut(e, mac)) return "more";
  if (isSettingsShortcut(e, mac)) return "settings";
  return isQuitShortcut(e, mac) ? "quit" : null;
}
