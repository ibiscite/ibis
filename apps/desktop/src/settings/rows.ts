import type { InterfaceMessages, MessageId } from "../i18n";

export const PAGES = ["general", "look", "libraries", "citations", "shortcuts", "advanced", "about"] as const;
export type Page = typeof PAGES[number];
export const ROWS: readonly { id: string; page: Page; label: MessageId; description?: MessageId; keywords?: MessageId }[] = [
  { id: "open-shortcut", page: "general", label: "shortcuts-open", description: "shortcuts-description" },
  { id: "login", page: "general", label: "general-login-label", description: "general-login-description", keywords: "settings-startup-words" },
  { id: "menu-bar", page: "general", label: "general-menu-bar", description: "general-menu-bar-unavailable" },
  { id: "paste-label", page: "general", label: "general-paste-label", description: "general-paste-description", keywords: "settings-paste-words" },
  { id: "welcome", page: "general", label: "general-welcome", description: "settings-coming-soon" },
  { id: "appearance", page: "look", label: "look-appearance", description: "look-appearance-description", keywords: "settings-appearance-words" },
  { id: "accent", page: "look", label: "look-accent", description: "look-accent-description" },
  { id: "rows", page: "look", label: "general-rows" },
  { id: "window-size", page: "look", label: "look-window-size", description: "settings-coming-soon" },
  { id: "search-icon", page: "look", label: "look-search-icon" },
  { id: "text-size", page: "look", label: "look-text-size" },
  { id: "preview", page: "look", label: "general-preview", description: "general-preview-description" },
  { id: "sources", page: "libraries", label: "libraries-sources", description: "settings-coming-soon" },
  { id: "zotero-libraries", page: "libraries", label: "libraries-search-in", description: "settings-coming-soon" },
  { id: "add-library", page: "libraries", label: "libraries-add", description: "settings-coming-soon" },
  { id: "remove-library", page: "libraries", label: "libraries-remove", description: "settings-coming-soon" },
  { id: "default-format", page: "citations", label: "citations-default", description: "citations-default-description" },
  { id: "per-app", page: "citations", label: "citations-per-app", description: "settings-coming-soon" },
  { id: "printed-pages", page: "citations", label: "citations-printed", description: "settings-coming-soon" },
  { id: "forget-formats", page: "citations", label: "citations-forget", description: "settings-coming-soon" },
  { id: "styles", page: "citations", label: "citations-styles", description: "settings-coming-soon" },
  { id: "choose-shortcut", page: "shortcuts", label: "shortcuts-choose", description: "settings-coming-soon" },
  { id: "placement", page: "advanced", label: "advanced-placement", description: "settings-coming-soon" },
  { id: "reset-position", page: "advanced", label: "advanced-reset-position", description: "settings-coming-soon" },
  { id: "settings-file", page: "advanced", label: "advanced-file", description: "settings-coming-soon" },
  { id: "command-line", page: "advanced", label: "advanced-command", description: "settings-coming-soon" },
  { id: "problem-report", page: "advanced", label: "advanced-report", description: "settings-coming-soon" },
  { id: "reset-settings", page: "advanced", label: "advanced-reset", description: "settings-coming-soon" },
  { id: "version-label", page: "about", label: "about-check" },
  { id: "automatic", page: "about", label: "about-automatic", description: "about-automatic-description" },
  { id: "report-problem", page: "about", label: "about-report", description: "settings-coming-soon" },
  { id: "suggest-idea", page: "about", label: "about-suggest", description: "settings-coming-soon" },
  { id: "licences", page: "about", label: "about-licences", description: "settings-coming-soon" },
];

/** Search and rendering read the same descriptions. */
export function rowDetails(id: string, text: InterfaceMessages["text"]) {
  const row = ROWS.find((row) => row.id === id)!;
  return { label: text(row.label), description: row.description ? text(row.description) : undefined };
}
