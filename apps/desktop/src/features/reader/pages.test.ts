import { expect, test } from "vitest";
import { formatPages } from "./pages";

// An invented book: Roman front matter, two pages with no printed number, then 417 onwards.
const labels = ["xi", "xii", null, null, "417", "418", "419", "420", "421", "422", "423"];

test("neighbouring printed pages join into a range; anything else is listed", () => {
  expect(formatPages(labels, new Set([4, 5, 6, 10]))).toBe("417–419, 423");
  expect(formatPages(labels, new Set([0, 1]))).toBe("xi–xii");
  expect(formatPages(labels, new Set([1, 4]))).toBe("xii, 417");
  // A page with no printed number cannot be cited by number, so it is left out.
  expect(formatPages(labels, new Set([2, 4]))).toBe("417");
  expect(formatPages(labels, new Set())).toBe("");
});
