import type { OUTPUTS } from "../bindings";

type Id = (typeof OUTPUTS)[number]["id"];

/** Output names and descriptions that are words rather than proper names
 * (Pandoc, APA and BibLaTeX \autocite read the same everywhere). The English
 * catalogue repeats the engine's own text, which a test holds it to. */
export const OUTPUT_NAMES = {
  "pandoc-bare": "output-name-pandoc-bare",
  keys: "output-name-keys",
  "doi-links": "output-name-doi-links",
  "scannable-cite": "output-name-scannable-cite",
  "chicago-author-date": "output-name-chicago-author-date",
  "chicago-notes-bibliography": "output-name-chicago-notes-bibliography",
  "china-national-standard-gb-t-7714-2025-numeric": "output-name-gb-numeric",
  "china-national-standard-gb-t-7714-2025-author-date": "output-name-gb-author-date",
  "china-national-standard-gb-t-7714-2025-note": "output-name-gb-note",
} as const satisfies Partial<Record<Id, string>>;

export const OUTPUT_DESCRIPTIONS = {
  apa: "output-description-apa",
  "chicago-notes-bibliography": "output-description-chicago-notes-bibliography",
  "modern-language-association": "output-description-mla",
  "china-national-standard-gb-t-7714-2025-numeric": "output-description-gb-numeric",
  "china-national-standard-gb-t-7714-2025-author-date": "output-description-gb-author-date",
  "china-national-standard-gb-t-7714-2025-note": "output-description-gb-note",
} as const satisfies Partial<Record<Id, string>>;
