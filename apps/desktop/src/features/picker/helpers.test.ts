import { expect, test } from "vitest";
import type React from "react";
import type { SearchResult } from "../../api";
import { choiceKey, choiceName, idKey, sameLibrary, styleOf } from "./choice";
import { abbreviatedAuthors, hiddenMatch, tileName } from "./rows";
import { isKeysShortcut, isMenuShortcut, isPreviewShortcut, isQuitShortcut } from "./shortcuts";

const record = (more: Partial<SearchResult> = {}): SearchResult => ({
  revision: "1", id: { source: "refs.bib", native_key: "a" }, key: "a", title: "T", authors: "García", year: 2021,
  matched: [], first_author: "García", author_count: 1, container: null, item_type: "article-journal", ...more,
});

test("a choice names a key format or a citation style", () => {
  expect(styleOf("pandoc")).toBeNull();
  expect(styleOf("style:apa")).toBe("apa");
  expect(choiceName("style:apa")).toBe("APA");
  expect(choiceName("pandoc")).toBeTruthy();
});

test("selection identity is the record, not its key or position", () => {
  expect(idKey({ source: "a", native_key: "b" })).not.toBe(idKey({ source: "a\u0000b", native_key: "" }));
  expect(choiceKey([record()])).not.toBe(choiceKey([record({ revision: "2" })]));
  expect(choiceKey([record({ key: "x" })])).toBe(choiceKey([record({ key: "y" })]));
  expect(sameLibrary("user", "user")).toBe(true);
  expect(sameLibrary({ group: 3 }, { group: 3 })).toBe(true);
  expect(sameLibrary({ group: 3 }, "user")).toBe(false);
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

test("a letter shortcut typed as punctuation is not the keys shortcut", () => {
  const press = (key: string, code: string) =>
    ({ key, code, ctrlKey: true, metaKey: false, altKey: false, shiftKey: false, target: null }) as unknown as React.KeyboardEvent;
  // Hebrew types / on the Q key: Ctrl+Q there quits rather than listing the keys.
  expect(isQuitShortcut(press("/", "KeyQ"), false)).toBe(true);
  expect(isKeysShortcut(press("/", "KeyQ"), false)).toBe(false);
  // Ctrl+/ on its own key, and Ctrl+? on AZERTY's comma key, still list the keys.
  expect(isKeysShortcut(press("/", "Slash"), false)).toBe(true);
  expect(isKeysShortcut({ ...press("?", "KeyM"), shiftKey: true } as React.KeyboardEvent, false)).toBe(true);
  expect(isMenuShortcut(press("/", "Slash"), false)).toBe(false);
});

test("preview shortcuts use logical I, the platform command key and strict modifiers", () => {
  const press = (more: Partial<React.KeyboardEvent> = {}) => ({ key: "i", code: "KeyI", ctrlKey: true,
    metaKey: false, altKey: false, shiftKey: false, target: null, ...more }) as React.KeyboardEvent;
  for (const mac of [false, true]) {
    const modifier = mac ? { ctrlKey: false, metaKey: true } : {};
    for (const key of ["i", "I"]) expect(isPreviewShortcut(press({ key, code: "Quote", ...modifier }), mac)).toBe(true);
    for (const key of ["'", "ш", "/"]) expect(isPreviewShortcut(press({ key, ...modifier }), mac)).toBe(false);
  }
  expect(isKeysShortcut(press({ key: "/" }), false)).toBe(true);
  expect(isPreviewShortcut(press({ ctrlKey: false, metaKey: true }), true)).toBe(true);
  for (const modifiers of [{ altKey: true }, { shiftKey: true }, { metaKey: true }, { ctrlKey: false }]) {
    expect(isPreviewShortcut(press(modifiers), false)).toBe(false);
  }
  expect(isPreviewShortcut(press(), true)).toBe(false);
  expect(isPreviewShortcut(press({ key: "j" }), false)).toBe(false);
});
