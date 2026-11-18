// Every canvas board, in one list the canvas bundle and the playground share.
export const scenarios = ["notice", "results", "unknown-app", "unknown-app-empty", "first-run", "more", "format-menu", "writings", "writings-pending", "cited-since-opening", "project-check-conflict", "states", "parts", "empty", "file-drag", "error", "loading", "empty-search", "shelf", "reference-details", "reference-details-more", "show-as-menu", "reader", "commands", "page-in-search", "peek", "library-selector", "library-menu", "shortcuts", "pinned", "no-matches", "missing", "zotero-closed", "unreadable", "stale", "compact", "larger-text", "right-to-left", "long-titles", "update", "undo-notice", "citation-order", "citation-handles", "citation-details-larger", "tokens", "settings-general", "settings-look", "settings-citations", "settings-ai-apps", "settings-shortcuts", "settings-about", "settings-details", "settings-newer", "settings-broken", "text-80", "text-120", "share-bibtex", "token-warnings", "sample", "editor-request", "picker-refusal", "retained-citation",
  // The picker, setup and Settings in each translated interface language.
  ...languageScenes(["results", "first-run", "settings-general"] as const, ["de", "ar"] as const)] as const;
export type Scenario = typeof scenarios[number];

/** `<scenario>-<language>`: a scenario drawn in another interface language. */
function languageScenes<S extends string, L extends string>(bases: readonly S[], languages: readonly L[]) {
  return bases.flatMap((base) => languages.map((language) => `${base}-${language}` as `${S}-${L}`));
}

/** The scenario a language scene draws, and the language it speaks. */
export function sceneOf(scenario: string): { base: string; language: "en-US" | "de" | "ar" } {
  const match = /^(.+)-(de|ar)$/.exec(scenario);
  return match ? { base: match[1]!, language: match[2] as "de" | "ar" } : { base: scenario, language: "en-US" };
}
