import { shortcutLabel } from "./LibrarySetup";
import { keyText } from "./messages";

export type KeyRegion = keyof typeof keyText.regions;
export type KeyAction = keyof typeof keyText.actions;
/** One key binding the picker really handles, in accelerator syntax. */
export type KeyBinding = { region: KeyRegion; action: KeyAction; keys: readonly string[] };

/** Every key the picker handles. The keys overlay, the footer hints and the
 * More menu read their labels from here, so the list cannot drift from them.
 * Add a row only together with the code that handles it. */
export const KEY_TABLE: readonly KeyBinding[] = [
  { region: "search", action: "copy", keys: ["Enter"] },
  { region: "search", action: "add", keys: ["Shift+Enter"] },
  { region: "search", action: "move", keys: ["ArrowDown", "ArrowUp"] },
  { region: "picker", action: "escape", keys: ["Escape"] },
  { region: "picker", action: "more", keys: ["CmdOrCtrl+K"] },
  { region: "picker", action: "keys", keys: ["CmdOrCtrl+/", "?"] },
  { region: "picker", action: "quit", keys: ["CmdOrCtrl+Q"] },
  { region: "menu", action: "menuMove", keys: ["ArrowDown", "ArrowUp"] },
  { region: "menu", action: "menuEnds", keys: ["Home", "End"] },
  { region: "menu", action: "menuChoose", keys: ["Enter", "Space"] },
  { region: "menu", action: "menuLeave", keys: ["Tab"] },
];

/** The accelerator a table row lists first for an action. */
export function keysFor(action: KeyAction): string {
  return KEY_TABLE.find((binding) => binding.action === action)?.keys[0] ?? "";
}

const NAMED: Record<string, [mac: string, other: string]> = {
  enter: ["↵", "Enter"], escape: ["Esc", "Esc"], arrowdown: ["↓", "↓"], arrowup: ["↑", "↑"],
  home: ["Home", "Home"], end: ["End", "End"], tab: ["Tab", "Tab"], space: ["Space", "Space"],
};

/** A key label in the platform's own words: ⌘ ⌥ ⇧ on macOS, Ctrl Alt Shift elsewhere. */
export function keyLabel(accelerator: string, mac: boolean): string {
  // "+" separates keys, so a lone "+" or a trailing one is the key itself.
  const parts = accelerator === "+" ? ["+"] : accelerator.split(/\+(?!$)/);
  return parts.map((part) => {
    const named = NAMED[part.toLowerCase()];
    if (named) return mac ? named[0] : named[1];
    return part.length === 1 ? part.toUpperCase() : shortcutLabel(part, mac);
  }).join(mac ? "" : "+");
}

/** The aria-keyshortcuts value for an accelerator. */
export function ariaKeys(accelerator: string, mac: boolean): string {
  return accelerator.split(/\+(?!$)/).map((part) =>
    part === "CmdOrCtrl" ? (mac ? "Meta" : "Control") : part.length === 1 ? part.toUpperCase() : part).join("+");
}
