import type { MessageId } from "../i18n";

export const PAGES = ["general", "look", "libraries", "citations", "shortcuts", "advanced", "about"] as const;
export type Page = typeof PAGES[number];
export const ROWS: readonly { id: string; page: Page; label: MessageId; description?: MessageId; keywords?: MessageId }[] = [
  { id: "open-shortcut", page: "general", label: "shortcuts-open", description: "shortcuts-description" },
  { id: "login", page: "general", label: "general-login-label", description: "general-login-description", keywords: "settings-startup-words" },
  { id: "menu-bar", page: "general", label: "general-menu-bar", description: "general-menu-bar-description" },
  { id: "paste-label", page: "general", label: "general-paste-label", description: "general-paste-description", keywords: "settings-paste-words" },
  { id: "welcome", page: "general", label: "general-welcome" },
  { id: "appearance", page: "look", label: "look-appearance", keywords: "settings-appearance-words" },
  { id: "accent", page: "look", label: "look-accent", description: "look-accent-description" },
  { id: "rows", page: "look", label: "general-rows" },
  { id: "window-size", page: "look", label: "look-window-size" },
  { id: "search-icon", page: "look", label: "look-search-icon" },
  { id: "text-size", page: "look", label: "look-text-size" },
  { id: "preview", page: "look", label: "general-preview", description: "general-preview-description" },
  { id: "sources", page: "libraries", label: "libraries-sources" },
  { id: "zotero-libraries", page: "libraries", label: "libraries-search-in" },
  { id: "add-library", page: "libraries", label: "libraries-add" },
  { id: "remove-library", page: "libraries", label: "libraries-remove" },
  { id: "default-format", page: "citations", label: "citations-default" },
  { id: "per-app", page: "citations", label: "citations-per-app" },
  { id: "printed-pages", page: "citations", label: "citations-printed" },
  { id: "forget-formats", page: "citations", label: "citations-forget" },
  { id: "styles", page: "citations", label: "citations-styles" },
  { id: "choose-shortcut", page: "shortcuts", label: "shortcuts-choose" },
  { id: "placement", page: "advanced", label: "advanced-placement" },
  { id: "reset-position", page: "advanced", label: "advanced-reset-position" },
  { id: "settings-file", page: "advanced", label: "advanced-file" },
  { id: "command-line", page: "advanced", label: "advanced-command" },
  { id: "problem-report", page: "advanced", label: "advanced-report" },
  { id: "reset-settings", page: "advanced", label: "advanced-reset" },
  { id: "version-label", page: "about", label: "about-check" },
  { id: "automatic", page: "about", label: "about-automatic", description: "about-automatic-description" },
  { id: "report-problem", page: "about", label: "about-report" },
  { id: "suggest-idea", page: "about", label: "about-suggest" },
  { id: "licences", page: "about", label: "about-licences" },
];

export function rememberedPage(): Page {
  try { return pageOf(localStorage.getItem("ibis.settings.page")); } catch { return "general"; }
}
function pageOf(value: string | null): Page { return PAGES.includes(value as Page) ? value as Page : "general"; }
export function rememberPage(page: Page) {
  try { localStorage.setItem("ibis.settings.page", page); } catch { /* Navigation still works without storage. */ }
}
