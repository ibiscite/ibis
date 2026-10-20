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
