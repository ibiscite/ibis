// What a result row shows. Pure; no React.
import type { SearchResult } from "../../api";

// Fields a row already shows; a match anywhere else is named under the row.
const VISIBLE_FIELDS = new Set(["title", "author", "citekey", "year"]);

/** The strongest match the row does not show, if every match is hidden. */
export function hiddenMatch(matched: string[]): string | null {
  if (matched.length === 0 || matched.some((field) => VISIBLE_FIELDS.has(field))) return null;
  return matched[0] ?? null;
}

/** Rows name one or two authors in full and shorten three or more to
 * "García et al.". The count comes from the record's creators, so a name with
 * a comma is one; with one or two creators the joined surnames are the short form. */
export function abbreviatedAuthors(r: SearchResult): boolean {
  return Boolean(r.first_author) && r.author_count > 2;
}

/** "García 2021" for the tile's tooltip: first author and year, else the key. */
export function tileName(r: SearchResult): string {
  const first = r.first_author?.trim();
  return first ? (r.year === null ? first : `${first} ${r.year}`) : r.key;
}
