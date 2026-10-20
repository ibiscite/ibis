/** The seven type icons; every CSL item type maps to one of them. */
export type ReferenceKind = "article" | "book" | "chapter" | "thesis" | "web" | "report" | "other";

const kindOfType: Record<string, ReferenceKind> = {
  "article": "article", "article-journal": "article", "article-magazine": "article", "article-newspaper": "article",
  "paper-conference": "article", "review": "article", "review-book": "article", "manuscript": "article", "periodical": "article",
  "book": "book", "classic": "book", "collection": "book",
  "chapter": "chapter", "entry": "chapter", "entry-dictionary": "chapter", "entry-encyclopedia": "chapter",
  "thesis": "thesis",
  "webpage": "web", "post": "web", "post-weblog": "web",
  "report": "report", "standard": "report",
};

/** The type icon for a CSL item type (as sent with each result); anything else is "other". */
export function kindOf(itemType: string | undefined): ReferenceKind {
  return itemType !== undefined && Object.hasOwn(kindOfType, itemType) ? kindOfType[itemType] ?? "other" : "other";
}

/* Ibis's own icons (docs/design.md, "Icons"): drawn on a 20 px grid with a
   1.5 px stroke and rounded ends, the same on every platform. Type icons say
   what a reference is and never reuse an attachment drawing (the document and
   the globe), so a row never shows the same picture twice. Article, book,
   thesis and web page come from the design prototype's set; chapter (a book
   with a ribbon), report (a clipboard with bars) and other (a box) are drawn
   to match. Every icon is decorative. */

const typePaths: Record<ReferenceKind, readonly string[]> = {
  article: ["M5 2.5h7l3 3v12H5z", "M8 8.5h4.5M8 11.5h4.5M8 14.5h3"],
  book: ["M4 4.5A2 2 0 0 1 6 2.5h9.5v12H6a2 2 0 0 0-2 2z", "M4 16.5a2 2 0 0 0 2 1h9.5"],
  chapter: ["M4 4.5A2 2 0 0 1 6 2.5h9.5v12H6a2 2 0 0 0-2 2z", "M4 16.5a2 2 0 0 0 2 1h9.5", "M9 2.5v6l2-1.5 2 1.5v-6"],
  thesis: ["m10 3.5 8 4-8 4-8-4z", "M5 9v4c0 1.4 2.2 2.5 5 2.5s5-1.1 5-2.5V9"],
  web: ["M2.5 5.5a2 2 0 0 1 2-2h11a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-11a2 2 0 0 1-2-2z", "M2.5 7.5h15"],
  report: ["M7.5 3.5H5v14h10v-14h-2.5", "M7.5 2.5h5V5h-5z", "M8 14.5v-2.5M10 14.5v-5M12 14.5V11"],
  other: ["M10 2.5 16.5 6v8L10 17.5 3.5 14V6z", "M3.5 6 10 9.5 16.5 6M10 9.5v8"],
};

/** The leading type icon of a result row; unknown kinds fall back to "other". */
export function TypeIcon({ itemType }: { itemType: string | undefined }) {
  const paths = typePaths[kindOf(itemType)];
  return (
    <svg className="type-icon" aria-hidden="true" focusable="false" width="20" height="20" viewBox="0 0 20 20" fill="none"
      stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      {paths.map((d) => <path key={d} d={d} />)}
    </svg>
  );
}

/** The tile's add (plus) or added (check) glyph, as in the prototype's tile. */
export function AddGlyph({ added }: { added: boolean }) {
  return (
    <svg className="add-glyph" aria-hidden="true" focusable="false" width="18" height="18" viewBox="0 0 20 20" fill="none"
      stroke="currentColor" strokeWidth={added ? 2 : 1.8} strokeLinecap="round" strokeLinejoin="round">
      <path d={added ? "m5 10.5 3.2 3.2L15 6.5" : "M10 5v10M5 10h10"} />
    </svg>
  );
}
