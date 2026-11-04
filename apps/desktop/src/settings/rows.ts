import type { InterfaceMessages, MessageId } from "../i18n";

export const PAGES = ["general", "look", "citations", "shortcuts", "about"] as const;
export type Page = typeof PAGES[number];
export const ROWS: readonly { id: string; page: Page; label: MessageId; description?: MessageId; keywords?: MessageId }[] = [
  { id: "open-shortcut", page: "general", label: "shortcuts-open", description: "shortcuts-description" },
  { id: "login", page: "general", label: "general-login-label", description: "general-login-description", keywords: "settings-startup-words" },
  { id: "menu-bar", page: "general", label: "general-menu-bar-label", description: "general-menu-bar-description" },
  { id: "small-surprises", page: "general", label: "general-small-surprises-label", description: "general-small-surprises-description", keywords: "settings-surprises-words" },
  { id: "backup-settings", page: "general", label: "backup-label", description: "backup-description", keywords: "backup-words" },
  { id: "restore-settings", page: "general", label: "restore-label", description: "restore-description", keywords: "backup-words" },
  { id: "paste-label", page: "general", label: "general-paste-label", description: "general-paste-description", keywords: "settings-paste-words" },
  { id: "appearance", page: "look", label: "look-appearance", description: "look-appearance-description", keywords: "settings-appearance-words" },
  { id: "accent", page: "look", label: "look-accent", keywords: "look-accent-words" },
  { id: "rows", page: "look", label: "general-rows" },
  { id: "window-size", page: "look", label: "look-window-size", keywords: "look-window-large" },
  { id: "text-size", page: "look", label: "look-text-size" },
  { id: "default-format", page: "citations", label: "citations-default", description: "citations-default-description" },
  { id: "recent-citations", page: "citations", label: "citations-recent", description: "citations-recent-description", keywords: "citations-recent-words" },
  { id: "about-ibis", page: "about", label: "settings-about", keywords: "delight-about-words" },
  { id: "automatic", page: "about", label: "about-automatic", description: "about-automatic-description", keywords: "about-check" },
];

/** Search and rendering read the same descriptions. */
export function rowDetails(id: string, text: InterfaceMessages["text"]) {
  const row = ROWS.find((row) => row.id === id)!;
  return { label: text(row.label), description: row.description ? text(row.description) : undefined };
}
