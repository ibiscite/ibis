import { expect, test } from "vitest";
import { enterLabel } from "./enterLabel";

test("action and output describe paste, copy-only, kept-open and pasted-list citations", () => {
  expect(enterLabel("style:apa", 1, "Example Word", true, false, false)).toBe("Paste into Example Word · APA");
  expect(enterLabel("style:apa", 1, "Example Word", false, false, false)).toBe("Copy as APA");
  expect(enterLabel("pandoc", 2, "Example Editor", true, true, false)).toBe("Copy 2 as Pandoc");
  expect(enterLabel("pandoc", 2, null, false, false, true)).toBe("Copy 2 citations · Pandoc");
  expect(enterLabel("style:ieee", 1, null, false, false, false)).toBe("Copy IEEE entry");
});

test("numeric styles identify reference entries when pasted or copied as a list", () => {
  expect(enterLabel("style:ieee", 1, "Example Word", true, false, false)).toBe("Paste into Example Word · IEEE entry");
  expect(enterLabel("style:ieee", 2, "Example Word", true, false, true)).toBe("Paste into Example Word · 2 IEEE entries");
  expect(enterLabel("style:ieee", 2, null, false, false, true)).toBe("Copy 2 IEEE entries");
});
