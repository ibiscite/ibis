import type { PickerCommand, ShortcutRefusal } from "../bindings";
import type { KeyAction, KeyRegion } from "../keys";
import type { MessageId } from ".";

export const SHORTCUT_ACTIONS = {
  chooseSource: "shortcuts-action-choose-source", revealSource: "shortcuts-action-reveal-source", source: "shortcuts-action-source", open: "shortcuts-open", preview: "shortcuts-action-preview", copy: "shortcuts-action-copy",
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

export const SETTINGS_IDS = [
  "citations-key-formats", "citations-style-formats", "citations-filter-formats",
  "look-sample-title-2", "look-sample-title-3",
  "settings-read-only",
  "general-menu-bar-unavailable",
  "settings-undo", "settings-shortcut-restored",
  "citations-default-description",
  "citations-format-pandoc",
  "citations-format-autocite",
  "citations-format-parencite",
  "citations-format-textcite",
  "citations-format-cite",
  "citations-format-typst",
  "citations-format-keys",
  "citations-format-apa",
  "citations-format-chicago-author-date",
  "citations-format-harvard-cite-them-right",
  "citations-format-ieee",
  "citations-format-modern-language-association",

  "settings-look",
  "settings-libraries",
  "settings-citations",
  "settings-advanced",
  "settings-search",
  "settings-no-results",
  "settings-coming-soon",
  "settings-read-failed",
  "settings-choice-failed",
  "settings-startup-words",
  "settings-paste-words",
  "settings-appearance-words",
  "general-menu-bar",
  "general-menu-bar-description",
  "general-welcome",
  "look-appearance",
  "look-appearance-description",
  "look-system",
  "look-light",
  "look-dark",
  "look-accent",
  "look-accent-description",
  "look-accent-system",
  "look-accent-blue",
  "look-accent-purple",
  "look-accent-pink",
  "look-accent-red",
  "look-accent-orange",
  "look-accent-yellow",
  "look-accent-green",
  "look-accent-graphite",
  "look-window-size",
  "look-small",
  "look-medium",
  "look-large",
  "look-compact-window",
  "look-custom",
  "look-search-icon",
  "look-ibis",
  "look-magnifier",
  "look-none",
  "look-text-size",
  "look-default-text",
  "look-larger-text",
  "look-largest-text",
  "look-sample-title",
  "look-sample-authors",
  "look-sample-journal",
  "libraries-sources",
  "libraries-search-in",
  "libraries-add",
  "libraries-remove",
  "citations-default",
  "citations-per-app",
  "citations-printed",
  "citations-forget",
  "citations-styles",
  "shortcuts-choose",
  "advanced-placement",
  "advanced-reset-position",
  "advanced-file",
  "advanced-command",
  "advanced-report",
  "advanced-reset",
  "about-report",
  "about-suggest",
  "about-licences",
] as const;
