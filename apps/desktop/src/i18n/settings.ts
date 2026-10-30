import type { PickerCommand, ShortcutRefusal } from "../bindings";
import type { KeyAction, KeyRegion } from "../keys";
import type { MessageId } from ".";

export const SHORTCUT_ACTIONS = {
  source: "shortcuts-action-source", open: "shortcuts-open", preview: "shortcuts-action-preview", copy: "shortcuts-action-copy",
  add: "shortcuts-action-add", move: "shortcuts-action-move", escape: "shortcuts-action-escape",
  format: "shortcuts-action-format", more: "shortcuts-action-more", pin: "shortcuts-action-pin",
  keys: "shortcuts-action-keys", settings: "shortcuts-action-settings", quit: "shortcuts-action-quit",
  menuMove: "shortcuts-action-menu-move", menuEnds: "shortcuts-action-menu-ends",
  menuChoose: "shortcuts-action-menu-choose", menuLeave: "shortcuts-action-menu-leave",
} as const satisfies Record<KeyAction, MessageId>;
export const SHORTCUT_REGIONS = {
  search: "shortcuts-region-search", picker: "shortcuts-region-picker",
  menu: "shortcuts-region-menu", system: "shortcuts-region-system",
} as const satisfies Record<KeyRegion, MessageId>;
export const SHORTCUT_COMMANDS = {
  format: "shortcuts-command-format", pin: "shortcuts-command-pin", keys: "shortcuts-command-keys",
  source: "shortcuts-command-source", more: "shortcuts-command-more", preview: "shortcuts-command-preview",
  settings: "shortcuts-command-settings", quit: "shortcuts-command-quit",
} as const satisfies Record<PickerCommand, MessageId>;
const REFUSALS = {
  invalid: "shortcuts-refusal-invalid", "needs-modifier": "shortcuts-refusal-needs-modifier",
  "app-shortcut": "shortcuts-refusal-app-shortcut", "alt-gr": "shortcuts-refusal-alt-gr",
  "in-use": "shortcuts-refusal-in-use", unavailable: "shortcuts-refusal-unavailable",
  "not-saved": "shortcuts-refusal-not-saved",
} as const satisfies Record<Extract<ShortcutRefusal, string>, MessageId>;
export type ShortcutProblem = Readonly<{ id: MessageId; command?: PickerCommand }>;

/** The existing code-to-copy mapping only; native intent and text stay native. */
export function shortcutRefusalMessage(refusal: unknown): ShortcutProblem {
  if (typeof refusal === "string" && Object.hasOwn(REFUSALS, refusal)) {
    return { id: REFUSALS[refusal as Extract<ShortcutRefusal, string>] };
  }
  if (typeof refusal === "object" && refusal !== null && "picker-shortcut" in refusal) {
    const command = refusal["picker-shortcut"];
    if (typeof command === "string" && Object.hasOwn(SHORTCUT_COMMANDS, command)) {
      return { id: "shortcuts-picker-conflict", command: command as PickerCommand };
    }
  }
  return { id: "shortcuts-failed" };
}
