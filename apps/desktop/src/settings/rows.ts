import type { InterfaceMessages, MessageId } from "../i18n";

export const PAGES = ["general", "look", "citations", "shortcuts", "about"] as const;
export type Page = typeof PAGES[number];
export const ROWS: readonly { id: string; page: Page; label: MessageId; description?: MessageId; keywords?: MessageId }[] = [
  { id: "open-shortcut", page: "general", label: "shortcuts-open", description: "shortcuts-description" },
  { id: "login", page: "general", label: "general-login-label", description: "general-login-description", keywords: "settings-startup-words" },
  { id: "paste-label", page: "general", label: "general-paste-label", description: "general-paste-description", keywords: "settings-paste-words" },
  { id: "appearance", page: "look", label: "look-appearance", description: "look-appearance-description", keywords: "settings-appearance-words" },
  { id: "accent", page: "look", label: "look-accent", description: "look-accent-description" },
  { id: "rows", page: "look", label: "general-rows" },
  { id: "search-icon", page: "look", label: "look-search-icon" },
  { id: "text-size", page: "look", label: "look-text-size" },
  { id: "preview", page: "look", label: "general-preview", description: "general-preview-description" },
  { id: "default-format", page: "citations", label: "citations-default", description: "citations-default-description" },
  { id: "forget-formats", page: "citations", label: "citations-forget", description: "citations-forget-description" },
  { id: "version-label", page: "about", label: "about-check" },
  { id: "automatic", page: "about", label: "about-automatic", description: "about-automatic-description" },
];

/** Search and rendering read the same descriptions. */
export function rowDetails(id: string, text: InterfaceMessages["text"]) {
  const row = ROWS.find((row) => row.id === id)!;
  return { label: text(row.label), description: row.description ? text(row.description) : undefined };
}
