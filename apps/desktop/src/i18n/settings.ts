import type { Problem } from "../bindings";
import type { MessageId } from ".";

export const SHORTCUT_ACTIONS = {
  copyCitation: "shortcuts-action-copy-only",
  chooseSource: "shortcuts-action-choose-source", revealSource: "shortcuts-action-reveal-source", source: "shortcuts-action-source", open: "shortcuts-action-open", changeLast: "shortcuts-action-change-last", copy: "shortcuts-action-copy",
  add: "shortcuts-action-add", move: "shortcuts-action-move", pick: "shortcuts-action-pick", peek: "shortcuts-action-peek", escape: "shortcuts-action-escape",
  format: "shortcuts-action-format", more: "shortcuts-action-more", pin: "shortcuts-action-pin",
  keys: "shortcuts-action-keys", settings: "shortcuts-action-settings", quit: "shortcuts-action-quit",
  undo: "shortcuts-action-undo", textSize: "shortcuts-action-text-size",
  citationMove: "shortcuts-action-citation-move", citationRemove: "shortcuts-action-citation-remove",
  citationReorder: "shortcuts-action-citation-reorder", citationOpen: "shortcuts-action-citation-open", citationFocus: "shortcuts-action-citation-focus",
  menuMove: "shortcuts-action-menu-move", menuEnds: "shortcuts-action-menu-ends",
  menuChoose: "shortcuts-action-menu-choose", menuLeave: "shortcuts-action-menu-leave",
} as const satisfies Record<string, MessageId>;
export const SHORTCUT_REGIONS = {
  search: "shortcuts-region-search", library: "shortcuts-region-library", picker: "shortcuts-region-picker",
  menu: "shortcuts-region-menu", system: "shortcuts-region-system", citation: "shortcuts-region-citation",
} as const satisfies Record<string, MessageId>;
export type ShortcutProblem = Readonly<{ id: MessageId; args?: Record<string, string>; failure?: Problem }>;
const REFUSAL_IDS = new Set<string>([
  "shortcuts-refusal-invalid", "shortcuts-refusal-needs-modifier", "shortcuts-refusal-app-shortcut",
  "shortcuts-refusal-alt-gr", "shortcuts-refusal-in-use", "shortcuts-refusal-unavailable",
  "shortcuts-refusal-not-saved", "shortcuts-picker-conflict",
]);

/** Native failures already name their catalogue message and arguments. */
export function shortcutRefusalMessage(refusal: unknown): ShortcutProblem {
  const problem = typeof refusal === "object" && refusal !== null ? refusal as Partial<Problem> : null;
  if (problem?.message && REFUSAL_IDS.has(problem.message.id)) {
    return { id: problem.message.id as MessageId, args: problem.message.args, failure: problem as Problem };
  }
  return { id: "shortcuts-failed" };
}

export const SETTINGS_IDS = [
  "citations-unlisted",
  "citations-documents", "citations-keys", "citations-apps", "citations-forget", "citations-writings", "settings-open-file", "settings-file-description",
  "settings-shortcut-words", "settings-details", "settings-use-default", "settings-default-restored", "settings-undo-default",
  "command-install", "command-remove", "command-checking", "command-missing", "command-matching", "command-different", "command-foreign", "command-profile", "command-copy", "command-copied", "command-failed",
  "backup-words", "backup-title", "backup-label", "backup-description", "backup-button", "backup-saved",
  "restore-label", "restore-description", "restore-button", "restore-from", "restore-same", "restore-confirm", "restore-cancel", "restore-restarting",
  "backup-change", "backup-note-library", "backup-note-shortcut", "backup-note-repairs",
  "backup-setting-library", "backup-setting-larger-text", "backup-setting-shortcut",
  "backup-value-default", "backup-value-on", "backup-value-off", "backup-value-zotero", "backup-value-zotero-many", "backup-value-zotero-group",
  "backup-problem-not-downloaded", "backup-problem-unreadable", "backup-problem-not-a-backup", "backup-problem-settings-file",
  "backup-problem-newer", "backup-problem-invalid", "backup-problem-settings-unusable", "backup-problem-busy", "backup-problem-changed", "backup-problem-not-saved",
  "citations-key-formats", "citations-style-formats", "citations-filter-formats",
  "look-sample-title-2", "look-sample-title-3",
  "settings-read-only", "settings-newer-title", "settings-newer-description",
  "settings-shortcut-restored",
  "citations-default-description",
  "citations-recent", "citations-recent-description", "citations-recent-words",
  "citations-editor", "citations-editor-description", "citations-editor-words", "citations-editor-address", "citations-editor-address-description",
  "citations-editor-copy", "citations-editor-copied", "citations-editor-port-in-use", "citations-editor-bad-setting", "citations-editor-not-saved",

  "settings-look",
  "settings-citations",
  "settings-search",
  "settings-no-results",
  "settings-results",
  "settings-results-count",
  "settings-surprises-words",
  "settings-read-failed",
  "settings-forget-failed", "settings-open-failed",
  "settings-choice-failed",
  "settings-startup-words",
  "settings-paste-words",
  "settings-appearance-words",
  "look-appearance",
  "look-appearance-description",
  "look-system",
  "look-light",
  "look-dark",
  "look-accent",
  "look-accent-system",
  "look-accent-ibis",
  "look-accent-words",
  "look-window-size",
  "look-window-small",
  "look-window-medium",
  "look-window-large",
  "look-window-custom",
  "look-window-custom-note",
  "look-text-size",
  "look-default-text",
  "look-larger-text",
  "look-sample-title",
  "look-sample-authors",
  "look-sample-journal",
  "citations-default",
] as const;
