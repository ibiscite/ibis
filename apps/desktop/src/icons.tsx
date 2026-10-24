import { typePaths } from "./icon-paths";

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
