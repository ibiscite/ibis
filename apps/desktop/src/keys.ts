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
  { region: "picker", action: "pin", keys: ["CmdOrCtrl+Shift+P"] },
  { region: "picker", action: "preview", keys: ["CmdOrCtrl+I"] },
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

/** A shortcut as people read it: `⌃⌘R` on macOS, `Ctrl+Shift+Y` elsewhere. */
export function shortcutLabel(shortcut: string, mac: boolean): string {
  // Settings may spell keys in any case and with the aliases the shortcut
  // parser accepts, such as `CmdOrCtrl` or `KeyR`.
  const names: Record<string, [mac: string, other: string]> = {
    control: ["⌃", "Ctrl"], ctrl: ["⌃", "Ctrl"],
    alt: ["⌥", "Alt"], option: ["⌥", "Alt"],
    shift: ["⇧", "Shift"],
    command: ["⌘", "Super"], cmd: ["⌘", "Super"], super: ["⌘", "Super"], meta: ["⌘", "Super"],
    commandorcontrol: ["⌘", "Ctrl"], cmdorctrl: ["⌘", "Ctrl"], commandorctrl: ["⌘", "Ctrl"], cmdorcontrol: ["⌘", "Ctrl"],
    space: ["Space", "Space"],
  };
  const keys = shortcut.split("+").map((key) => key.trim()).filter(Boolean).map((key) => {
    const known = names[key.toLowerCase()];
    if (known) return mac ? known[0] : known[1];
    const plain = /^(?:Key|Digit)(.)$/i.exec(key)?.[1] ?? key;
    return plain.length === 1 ? plain.toUpperCase() : plain;
  });
  return keys.join(mac ? "" : "+");
}

/** The parts of a key press a shortcut depends on. */
export type KeyPress = Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">;

/** The command modifier on its own: ⌘ on macOS, Ctrl elsewhere. Ctrl+K is
 * "delete to end of line" in macOS text fields, so there only ⌘ counts. */
export function commandKey(e: KeyPress, mac: boolean): boolean {
  return mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
}

/** Whether a key press is an accelerator from the key table, such as `CmdOrCtrl+Q`.
 * Letters match in any case (Caps Lock). When the key types anything but a
 * Latin letter (Cyrillic й, Greek ;, Hebrew /), the physical key decides
 * instead; a Latin layout matches what it types, so AZERTY's Q key is A. */
export function matchesAccelerator(e: KeyPress, accelerator: string, mac: boolean): boolean {
  const parts = accelerator === "+" ? ["+"] : accelerator.split(/\+(?!$)/);
  const key = parts[parts.length - 1] ?? "";
  const modifiers = new Set(parts.slice(0, -1).map((part) => part.toLowerCase()));
  const command = modifiers.has("cmdorctrl") ? commandKey(e, mac) : !e.ctrlKey && !e.metaKey;
  if (!command || e.altKey !== modifiers.has("alt") || e.shiftKey !== modifiers.has("shift")) return false;
  if (e.key.toLowerCase() === key.toLowerCase()) return true;
  const physical = /^[a-z]$/i.test(key) ? `Key${key.toUpperCase()}` : /^\d$/.test(key) ? `Digit${key}` : null;
  return physical !== null && e.code === physical && e.key.length === 1 && !/^[a-z]$/i.test(e.key);
}
