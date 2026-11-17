import { recordId } from "../../test-fixtures";
import { expect, test } from "vitest";
import type { SearchResult } from "../../api";
import { abbreviatedAuthors, hiddenMatch, tileName } from "./rows";

const record = (more: Partial<SearchResult> = {}): SearchResult => ({
  revision: "1", id: { library: "refs.bib", key: recordId("a") }, key: "a", cite: "1", source: "refs.bib", title: "T", authors: "García", year: "2021",
  matched: [], partial: false, first_author: "García", author_count: 1, container: null, item_type: "article-journal", ...more,
});

test("rows name a hidden match, shorten three or more authors and title the tile", () => {
  expect(hiddenMatch([])).toBeNull();
  expect(hiddenMatch(["abstract", "title"])).toBeNull();
  expect(hiddenMatch(["abstract", "note"])).toBe("abstract");
  expect(abbreviatedAuthors(record({ author_count: 2 }))).toBe(false);
  expect(abbreviatedAuthors(record({ author_count: 3 }))).toBe(true);
  expect(abbreviatedAuthors(record({ author_count: 3, first_author: undefined }))).toBe(false);
  expect(tileName(record())).toBe("García 2021");
  expect(tileName(record({ year: null }))).toBe("García");
  expect(tileName(record({ first_author: "  " }))).toBe("a");
});
