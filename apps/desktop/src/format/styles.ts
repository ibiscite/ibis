import type { CitationStyle } from "../bindings";

// The bundled citation styles by id and title. Kept apart from the engine so
// the picker can list them without loading the engine or the style files.
export const STYLE_TITLES = {
  apa: "APA (7th edition)",
  "chicago-author-date": "Chicago (author-date)",
  "harvard-cite-them-right": "Harvard (Cite Them Right)",
  ieee: "IEEE",
  "modern-language-association": "MLA (9th edition)",
} as const;

export type StyleId = keyof typeof STYLE_TITLES;

/**
 * A numeric style's number depends on every reference the document cites,
 * which one copy cannot see, so these styles copy the reference entry
 * instead, without its number (#1129).
 */
export function copiesEntry(style: StyleId): boolean {
  return style === "ieee";
}

// The settings file stores a style by the same id (ADR 0020); fail the type
// check if the Rust list and this one ever differ.
type Same<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;
void (true satisfies Same<StyleId, CitationStyle>);

// One word for the closed format control in the picker header.
export const STYLE_NAMES: Record<StyleId, string> = {
  apa: "APA",
  "chicago-author-date": "Chicago",
  "harvard-cite-them-right": "Harvard",
  ieee: "IEEE",
  "modern-language-association": "MLA",
};

// What each style really copies for the synthetic one-author 2021 menu example.
// IEEE copies the reference entry (#1129).
export const STYLE_SAMPLES: Record<StyleId, string> = {
  apa: "(García, 2021)",
  "chicago-author-date": "(García 2021)",
  "harvard-cite-them-right": "(García, 2021)",
  ieee: "A. García, “Synthetic sleep study,” Journal of Tests, 2021.",
  "modern-language-association": "(García)",
};
