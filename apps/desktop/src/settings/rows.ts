import { bindingKeys, DEFAULT_PICKER_SHORTCUT, shortcutBindings } from "../keys";
import { SHORTCUT_ACTIONS } from "../i18n/settings";
import type { PickerPreferences } from "../bindings";
import { defaultOutput, defaultStyle } from "../format/outputs";
import { currentPlatform } from "../platform";
import type { InterfaceMessages, MessageId } from "../i18n";

export const PAGES = ["general", "look", "citations", "shortcuts", "about"] as const;
export type Page = typeof PAGES[number];
/** A default marks a resettable setting; action and information rows omit it. */
export type SettingRow = { id: string; page: Page; label: MessageId; description?: MessageId; keywords?: MessageId; preference?: keyof PickerPreferences; default?: PickerPreferences[keyof PickerPreferences] };
export const ROWS: readonly SettingRow[] = [
  { id: "open-shortcut", default: DEFAULT_PICKER_SHORTCUT, page: "general", label: "shortcuts-open", description: "shortcuts-description", keywords: "settings-shortcut-words" },
  { id: "cli-command", page: "general", label: "command-install" },
  { id: "login", default: false, page: "general", label: "general-login-label", description: "general-login-description", keywords: "settings-startup-words" },
  { id: "menu-bar", preference: "menu_bar", default: true, page: "general", label: "general-menu-bar-label", description: "general-menu-bar-description" },
  { id: "small-surprises", preference: "small_surprises", default: true, page: "general", label: "general-small-surprises-label", description: "general-small-surprises-description", keywords: "settings-surprises-words" },
  { id: "backup-settings", page: "general", label: "backup-label", description: "backup-description", keywords: "backup-words" },
  { id: "restore-settings", page: "general", label: "restore-label", description: "restore-description", keywords: "backup-words" },
  { id: "paste-label", preference: "paste", default: true, page: "general", label: "general-paste-label", description: "general-paste-description", keywords: "settings-paste-words" },
  { id: "appearance", preference: "appearance", default: "system", page: "look", label: "look-appearance", description: "look-appearance-description", keywords: "settings-appearance-words" },
  { id: "accent", preference: "accent", default: "system", page: "look", label: "look-accent", keywords: "look-accent-words" },
  { id: "rows", preference: "density", default: "comfortable", page: "look", label: "general-rows" },
  { id: "window-size", default: "medium", page: "look", label: "look-window-size", keywords: "look-window-large" },
  { id: "text-size", preference: "larger_text", default: false, page: "look", label: "look-text-size" },
  { id: "default-format", preference: "output", default: defaultStyle, page: "citations", label: "citations-documents", description: "citations-default-description" },
  { id: "default-key-format", preference: "key_output", default: defaultOutput, page: "citations", label: "citations-keys" },
  { id: "open-settings-file", page: "citations", label: "settings-open-file", description: "settings-file-description" },
  { id: "recent-citations", preference: "recent_citations", default: true, page: "citations", label: "citations-recent", description: "citations-recent-description", keywords: "citations-recent-words" },
  { id: "editor-endpoint", default: false, page: "citations", label: "citations-editor", description: "citations-editor-description", keywords: "citations-editor-words" },
  { id: "about-ibis", page: "about", label: "settings-about", keywords: "delight-about-words" },
  { id: "automatic", default: false, page: "about", label: "about-automatic", description: "about-automatic-description" },
  { id: "check-updates", page: "about", label: "about-check-label", keywords: "about-check" },
];

/** Search and rendering read the same descriptions. */
export function rowDetails(id: string, text: InterfaceMessages["text"]) {
  const row = ROWS.find((row) => row.id === id)!;
  const platform = currentPlatform();
  return { label: text(row.label, { platform }), description: row.description ? text(row.description, { platform }) : undefined };
}

export function preferenceDefault(row: SettingRow, preferences: PickerPreferences) {
  if (!row.preference || row.default === undefined) return null;
  const value = preferences[row.preference] ?? row.default;
  return value === row.default ? null : { patch: { [row.preference]: row.default }, previous: { [row.preference]: value } };
}

/** Fixed keyboard rows share their definitions with the rendered catalogue. */
export function settingsRows(enterPastes: boolean): readonly SettingRow[] {
  return [...ROWS, ...shortcutBindings(enterPastes).filter((binding) => bindingKeys(binding, currentPlatform() === "macos").length).map((binding): SettingRow => ({
    id: `key-${binding.action}`, page: "shortcuts", label: binding.label ?? SHORTCUT_ACTIONS[binding.action],
  }))];
}
