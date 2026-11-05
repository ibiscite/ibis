import { createContext, useContext } from "react";
import type { CitationReference } from "../picker/choice";

/** What the reader needs from a document, whatever draws it (decision 0038).
 * The PDF.js adapter of decision 0036 implements this behind the bytes command;
 * until #1252 closes only the invented source below does, so Show PDF never
 * appears in the running app. */
export type PageSource = {
  /** Stable for one opening of one file: a content hash for a PDF. */
  id: string;
  /** Printed labels in PDF order; null where the page has no citable label. */
  labels: readonly (string | null)[];
  /** Draws page `index` into `into` and returns how to stop: a render that
   * leaves the view is cancelled, never drawn late. */
  draw: (index: number, into: HTMLElement) => () => void;
};

/** Which source, if any, belongs to a reference. Absent means no reader. */
export const PageSources = createContext<((reference: CitationReference) => PageSource | null) | null>(null);
export function usePageSource(reference: CitationReference | undefined): PageSource | null {
  const find = useContext(PageSources);
  return reference && find ? find(reference) : null;
}

const words = ["garden", "lantern", "orchard", "evening", "path", "quiet", "map", "cloud", "harbour", "season", "letter", "field", "light", "river", "walk", "notebook"];
/** Invented text only: a sentence made from page and line numbers. */
function sentence(page: number, line: number) {
  const pick = (n: number) => words[(page * 7 + line * 3 + n) % words.length];
  return `The ${pick(0)} beside the ${pick(1)} keeps a ${pick(2)} of ${pick(3)} and ${pick(4)}, as the ${pick(5)} turns toward the ${pick(6)}.`;
}

/** An invented book for the canvas and tests: Roman front matter, two pages
 * without a printed number, then numbered pages from `first`. */
export function inventedSource(id = "invented", first = 415, count = 24): PageSource {
  const labels = Array.from({ length: count }, (_, index) => index < 2 ? ["xi", "xii"][index]! : index < 4 ? null : String(first + index - 4));
  return {
    id, labels,
    draw(index, into) {
      const page = document.createElement("div");
      page.className = "invented-page";
      const heading = document.createElement("h4");
      heading.textContent = index < 2 ? "Preface" : index < 4 ? "Plates" : `Chapter ${Math.floor((index - 4) / 6) + 1}`;
      page.append(heading);
      for (let line = 0; line < 9; line++) {
        const paragraph = document.createElement("p");
        paragraph.textContent = `${sentence(index, line)} ${sentence(index, line + 9)}`;
        page.append(paragraph);
      }
      const folio = document.createElement("span");
      folio.className = "invented-folio";
      folio.textContent = labels[index] ?? "";
      page.append(folio);
      into.replaceChildren(page);
      return () => into.replaceChildren();
    },
  };
}
