import { bindingKeys, DEFAULT_PICKER_SHORTCUT, shortcutBindings } from "../keys";
import { SHORTCUT_ACTIONS } from "../i18n/settings";
import type { PickerPreferences } from "../bindings";
import { defaultOutput, defaultStyle } from "../format/outputs";
import { currentPlatform } from "../platform";
import type { InterfaceMessages, MessageId } from "../i18n";

export const PAGES = ["general", "look", "citations", "ai-apps", "shortcuts", "about"] as const;
export type Page = typeof PAGES[number];
type Control = { control: "switch" | "documents" | "keys" }
  | { control: "choice" | "appearance" | "menu"; options: readonly { value: string | boolean; label: MessageId; currentOnly?: boolean }[] }
  | { control?: undefined };
/** A setting's declaration. Its help line is part of it: a row cannot be declared
 * without saying what it does, and search reads the same words. */
export type SettingRow = Control & {
  id: string; order?: number; page: Page; section?: string; note?: MessageId; group?: MessageId; label: MessageId; description: MessageId; keywords?: MessageId;
  preference?: keyof PickerPreferences; native?: NativeSetting; default?: string | boolean;
};
type PreferenceDeclaration<K extends keyof PickerPreferences> = SettingRow & { order: number; default: NonNullable<PickerPreferences[K]>; control: Exclude<Control["control"], undefined> };
/** Exhaustive over the generated owner contract: a new preference needs its row. */
export const PREFERENCES = {
  picker_placement: { order: 4, id: "picker-placement", default: "centre", control: "menu", options: [{ value: "centre", label: "general-placement-centre" }, { value: "top-left", label: "general-placement-top-left" }, { value: "top-right", label: "general-placement-top-right" }, { value: "bottom-left", label: "general-placement-bottom-left" }, { value: "bottom-right", label: "general-placement-bottom-right" }], page: "general", group: "general-behavior-title", label: "general-placement", description: "general-placement-description", keywords: "general-placement-words" },
  menu_bar: { order: 3, id: "menu-bar", default: true, control: "switch", page: "general", group: "general-behavior-title", label: "general-menu-bar-label", description: "general-menu-bar-description" },
  small_surprises: { order: 5, id: "small-surprises", default: true, control: "switch", page: "general", group: "general-behavior-title", label: "general-small-surprises-label", description: "general-small-surprises-description", keywords: "settings-surprises-words" },
  paste: { order: 5, id: "paste-label", section: "paste", default: true, control: "switch", page: "general", group: "general-paste-title", label: "general-paste-label", description: "general-paste-description", keywords: "settings-paste-words" },
  appearance: { order: 1, id: "appearance", default: "system", control: "appearance", options: [{ value: "system", label: "look-system" }, { value: "light", label: "look-light" }, { value: "dark", label: "look-dark" }], page: "look", label: "look-appearance", description: "look-appearance-description", keywords: "settings-appearance-words" },
  accent: { order: 2, id: "accent", default: "system", control: "choice", options: [{ value: "system", label: "look-accent-system" }, { value: "ibis", label: "look-accent-ibis" }], page: "look", label: "look-accent", description: "look-accent-description", keywords: "look-accent-words" },
  density: { order: 3, id: "rows", default: "comfortable", control: "choice", options: [{ value: "compact", label: "general-density-compact" }, { value: "comfortable", label: "general-density-comfortable" }], page: "look", label: "general-rows", description: "general-rows-description" },
  larger_text: { order: 5, id: "text-size", default: false, control: "choice", options: [{ value: false, label: "look-default-text" }, { value: true, label: "look-larger-text" }], page: "look", label: "look-text-size", description: "look-text-size-description" },
  output: { order: 1, id: "default-format", default: defaultStyle, control: "documents", page: "citations", label: "citations-documents", description: "citations-default-description" },
  key_output: { order: 2, id: "default-key-format", default: defaultOutput, control: "keys", page: "citations", label: "citations-keys", description: "citations-keys-description" },
  recent_citations: { order: 3, id: "recent-citations", default: true, control: "switch", page: "citations", label: "citations-recent", description: "citations-recent-description", keywords: "citations-recent-words" },
} satisfies { [K in keyof PickerPreferences]-?: PreferenceDeclaration<K> };
const preferences: SettingRow[] = Object.entries(PREFERENCES).map(([preference, row]) => ({ ...row, preference: preference as keyof PickerPreferences }));
export const NATIVE_SETTINGS = {
  "open-shortcut": { order: 1, id: "open-shortcut", default: DEFAULT_PICKER_SHORTCUT, page: "general", group: "general-behavior-title", label: "shortcuts-open", description: "shortcuts-description", keywords: "settings-shortcut-words" },
  "login": { order: 2, id: "login", default: false, control: "switch", page: "general", group: "general-behavior-title", label: "general-login-label", description: "general-login-description", keywords: "settings-startup-words" },
  "window-size": { order: 4, id: "window-size", note: "look-window-custom-note", default: "medium", control: "choice", options: [{ value: "small", label: "look-window-small" }, { value: "medium", label: "look-window-medium" }, { value: "large", label: "look-window-large" }, { value: "custom", label: "look-window-custom", currentOnly: true }], page: "look", label: "look-window-size", description: "look-window-size-description", keywords: "look-window-large" },
  "editor-endpoint": { order: 4, id: "editor-endpoint", default: false, control: "switch", page: "citations", label: "citations-editor", description: "citations-editor-description", keywords: "citations-editor-words" },
  "automatic": { id: "automatic", default: false, control: "switch", page: "about", group: "about-updates", label: "about-automatic", description: "about-automatic-description" },
} satisfies Record<string, SettingRow>;
export type NativeSetting = keyof typeof NATIVE_SETTINGS;
export const ROWS: readonly SettingRow[] = ([
  ...preferences,
  ...Object.entries(NATIVE_SETTINGS).map(([native, row]) => ({ ...row, native: native as NativeSetting })),
  { id: "cli-command", page: "ai-apps", group: "ai-apps-command-line", label: "command-install", description: "command-install-description", keywords: "command-words" },
  { id: "agent-apps", page: "ai-apps", label: "settings-ai-apps", description: "ai-apps-description", keywords: "ai-apps-words" },
  { id: "attachment-text", page: "ai-apps", label: "attachment-text-label", description: "attachment-text-description", keywords: "attachment-text-words" },
  { id: "backup-settings", page: "general", label: "backup-label", description: "backup-description", keywords: "backup-words" },
  { id: "restore-settings", page: "general", label: "restore-label", description: "restore-description", keywords: "backup-words" },
  { id: "open-settings-file", page: "general", group: "settings-advanced-title", label: "settings-open-file", description: "settings-file-description" },
  { id: "about-ibis", page: "about", label: "settings-about", description: "settings-about-description", keywords: "delight-about-words" },
  { id: "check-updates", page: "about", label: "about-check-label", description: "about-check-description", keywords: "about-check" },
] satisfies SettingRow[]).sort((a: SettingRow, b: SettingRow) => (a.order ?? 99) - (b.order ?? 99));

/** Search and rendering read the same descriptions. */
export function rowDetails(id: string, text: InterfaceMessages["text"]) {
  const row = ROWS.find((row) => row.id === id)!;
  const platform = currentPlatform();
  return { label: text(row.label, { platform }), description: text(row.description, { platform }) };
}

/** What Settings search looks through: every setting, and each fixed key, whose
 * row is the key itself and needs no help line. */
export type SearchRow = Omit<SettingRow, "description"> & { description?: MessageId };
/** Fixed keyboard rows share their definitions with the rendered catalogue. */
export function settingsRows(enterPastes: boolean): readonly SearchRow[] {
  return [...ROWS, ...shortcutBindings(enterPastes).filter((binding) => bindingKeys(binding, currentPlatform() === "macos").length).map((binding): SearchRow => ({
    id: `key-${binding.action}`, page: "shortcuts", label: binding.label ?? SHORTCUT_ACTIONS[binding.action],
  }))];
}
