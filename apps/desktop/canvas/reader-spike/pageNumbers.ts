// Page numbers for the reader spike (#1252): the physical index, the printed
// label and where the label came from stay apart, and nothing is guessed.

export type LabelSource =
  /** The PDF's /PageLabels gave this page a non-empty label. */
  | "pdf-page-labels"
  /** The PDF has page labels, but this page's range gives it none. */
  | "pdf-empty-label"
  /** No readable /PageLabels (absent or malformed: PDF.js does not tell them apart). */
  | "none";

export type PageNumber = {
  /** 1-based position in the page tree; what "PDF page 8" means. */
  physical: number;
  /** What a citation may use; null when the PDF does not say. */
  printed: string | null;
  source: LabelSource;
  /** Other physical pages carrying the same printed label. */
  sharedWith: number[];
};

/** `labels` is PDF.js's `getPageLabels()`: null, or one string per page. */
export function pageNumbers(labels: readonly string[] | null, pageCount: number): PageNumber[] {
  // A label list of the wrong length is not evidence for any page.
  const usable = labels && labels.length === pageCount ? labels : null;
  const pages: PageNumber[] = Array.from({ length: pageCount }, (_, i) => {
    const label = usable?.[i] ?? null;
    const printed = label && label.trim() ? label : null;
    return { physical: i + 1, printed, source: !usable ? "none" : printed ? "pdf-page-labels" : "pdf-empty-label", sharedWith: [] };
  });
  const byLabel = new Map<string, number[]>();
  for (const page of pages) if (page.printed) byLabel.set(page.printed, [...byLabel.get(page.printed) ?? [], page.physical]);
  for (const page of pages) if (page.printed) page.sharedWith = byLabel.get(page.printed)!.filter((n) => n !== page.physical);
  return pages;
}

/** The indicator words from docs/design.md: "PDF page 8 · cite p. 416". */
export function indicator(page: PageNumber): string {
  if (!page.printed) return `PDF page ${page.physical} · Citation page unknown`;
  return `PDF page ${page.physical} · cite p. ${page.printed}`;
}

export function sourceWords(page: PageNumber): string {
  if (page.source === "none") return "This PDF gives no readable page labels";
  if (page.source === "pdf-empty-label") return "The PDF's page labels leave this page blank";
  if (page.sharedWith.length) return `From the PDF's page labels; also on PDF page ${page.sharedWith.join(", ")}`;
  return "From the PDF's page labels";
}

/** Physical pages whose printed label is `label`; several means ask, never pick. */
export function physicalForLabel(pages: readonly PageNumber[], label: string): number[] {
  return pages.filter((page) => page.printed === label).map((page) => page.physical);
}
