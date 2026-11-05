import { expect, test } from "vitest";
import { citableLabel, confirmOffset, formatPages, parsePages, roman } from "./pages";

// An invented book: Roman front matter, two pages with no printed number, then 417 onwards.
const labels = ["xi", "xii", null, null, "417", "418", "419", "420", "421", "422", "423"];

test("only short labels of letters, digits, '-' and '.' can be cited", () => {
  expect(["417", "xii", "A-3", "S1.2"].map(citableLabel)).toEqual(["417", "xii", "A-3", "S1.2"]);
  expect(["", "   ", "page 4", "4​", "a".repeat(17), null].map(citableLabel)).toEqual([null, null, null, null, null, null]);
});

test("Roman numerals count, so front matter forms ranges too", () => {
  expect(["i", "iv", "xii", "XL", "mcm"].map(roman)).toEqual([1, 4, 12, 40, 1900]);
  expect(roman("vii1")).toBeNull();
});

test("neighbouring printed pages join into a range; anything else is listed", () => {
  expect(formatPages(labels, new Set([4, 5, 6, 10]))).toBe("417–419, 423");
  expect(formatPages(labels, new Set([0, 1]))).toBe("xi–xii");
  expect(formatPages(labels, new Set([1, 4]))).toBe("xii, 417");
  // A page with no printed number cannot be cited by number, so it is left out.
  expect(formatPages(labels, new Set([2, 4]))).toBe("417");
  expect(formatPages(labels, new Set())).toBe("");
});

test("typed pages select the same thumbnails, and nothing unknown is guessed", () => {
  expect([...parsePages("417–419, 423", labels).selected]).toEqual([4, 5, 6, 10]);
  expect([...parsePages("417-419; 423", labels).selected]).toEqual([4, 5, 6, 10]);
  expect([...parsePages("XI", labels).selected]).toEqual([0]);
  expect(parsePages("12, 419–417, 5-6-7", labels)).toEqual({ selected: new Set(), unknown: ["12", "419–417", "5-6-7"] });
});

test("a confirmed printed number numbers only the unlabelled run around it", () => {
  expect(confirmOffset(labels, 3, "416")).toEqual(["xi", "xii", "415", "416", "417", "418", "419", "420", "421", "422", "423"]);
  // Already labelled, not a number or not citable: nothing changes.
  expect(confirmOffset(labels, 4, "9")).toEqual(labels);
  expect(confirmOffset(labels, 2, "four")).toEqual(labels);
});
