import { expect, test } from "vitest";
import { kindOf } from "./icons";

test("every CSL item type gets one of the seven type icons", () => {
  expect(["article-journal", "book", "chapter", "thesis", "webpage", "report", "dataset"].map(kindOf))
    .toEqual(["article", "book", "chapter", "thesis", "web", "report", "other"]);
  expect(kindOf("paper-conference")).toBe("article");
  expect(kindOf("entry-encyclopedia")).toBe("chapter");
  expect(kindOf("post-weblog")).toBe("web");
  // Unknown, missing, oddly cased or prototype-named types fall back to "other".
  for (const type of [undefined, "", "ARTICLE-JOURNAL", "article-journal\u0000", "__proto__", "constructor", "toString"]) {
    expect(kindOf(type)).toBe("other");
  }
});
