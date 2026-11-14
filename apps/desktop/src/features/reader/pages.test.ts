import { expect, test } from "vitest";
import { choosePages, citableLabel, confirmOffset, formatPages, parsePages, roman } from "./pages";

// An invented book: Roman front matter, two pages with no printed number, then 417 onwards.
const labels = ["xi", "xii", null, null, "417", "418", "419", "420", "421", "422", "423"];

test("only short labels of letters, digits, '-' and '.' can be cited", () => {
  expect(["417", "xii", "A-3", "S1.2"].map(citableLabel)).toEqual(["417", "xii", "A-3", "S1.2"]);
  expect(["", "   ", "page 4", "4​", "a".repeat(17), null].map(citableLabel)).toEqual([null, null, null, null, null, null]);
  expect(citableLabel("  A-3  ")).toBe("A-3");
  expect(citableLabel("e\u0301")).toBe("é");
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
  expect(parsePages("12, 419–417, 5-6-7", labels)).toEqual({ text: "12, 419–417, 5-6-7", selected: new Set(), ambiguous: [], unknown: ["12", "419–417", "5-6-7"] });
});

test.each(["\txii", "xii\n", "\rxii", "xii\u0085", "xii\u200b", "\u0000xii", "xii\u202e"])("control characters in %j are rejected before trimming", (text) => {
  expect(citableLabel(text)).toBeNull();
  expect(parsePages(text, ["xii", text])).toEqual({ text, selected: new Set(), ambiguous: [], unknown: [text] });
  expect(formatPages([text], new Set([0]))).toBe("");
  expect(confirmOffset([null], 0, text)).toEqual([null]);
});

test("unknown numeric labels cannot fall back to physical PDF positions", () => {
  const numbered = Array.from({ length: 20 }, (_, index) => String(index + 417));
  expect(parsePages("12", numbered)).toEqual({ text: "12", selected: new Set(), ambiguous: [], unknown: ["12"] });
});

test("exact labels containing hyphens win over ranges, with NFC and case folding", () => {
  const printed = ["A", "3", "A-3", "é"];
  expect([...parsePages("a-3, e\u0301", printed).selected]).toEqual([2, 3]);
  expect([...parsePages("A–3", printed).selected]).toEqual([0, 1]);
});

test("ambiguity stays separate from known pages and unknown text", () => {
  expect(parsePages("XII, 417, 12", ["xi", "xii", "XII", "417"])).toEqual({
    text: "XII, 417, 12", selected: new Set([3]),
    ambiguous: [{ text: "XII", candidates: [1, 2] }], unknown: ["12"],
  });
});

test.each(["xi–xii", "xii–xiv", "xii–xii"])("ambiguous endpoints in %s never resolve a range", (text) => {
  const result = parsePages(text, ["xi", "xii", "XII", "xiv"]);
  expect(result.selected.size).toBe(0);
  expect(result.ambiguous).toHaveLength(1);
  expect(result.unknown).toEqual([]);
});

test("an explicit second physical choice preserves identity separately from its printed text", () => {
  const printed = ["xi", "xii", "xii"];
  const choice = choosePages(printed, new Set([2]));
  expect(choice).toEqual({ text: "xii", selected: new Set([2]), ambiguous: [], unknown: [] });
  expect(parsePages(choice.text, printed).selected.size).toBe(0);
});

test("a confirmed printed number numbers only the unlabelled run around it", () => {
  expect(confirmOffset(labels, 3, "416")).toEqual(["xi", "xii", "415", "416", "417", "418", "419", "420", "421", "422", "423"]);
  // Already labelled, not a number or not citable: nothing changes.
  expect(confirmOffset(labels, 4, "9")).toEqual(labels);
  expect(confirmOffset(labels, 2, "four")).toEqual(labels);
});
