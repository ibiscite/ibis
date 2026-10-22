import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { SearchResult } from "../../api";
import { SelectionTray } from "./SelectionTray";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const chosen = new Map<string, SearchResult>(Array.from({ length: 3 }, (_, i) => [`k${i}`, {
  revision: "1", id: { source: "refs.bib", native_key: `k${i}` }, key: `synthetic_${i}_long_key`, title: `Title ${i}`, authors: "Author",
  year: 2020, matched: ["title"], first_author: "Author", author_count: 1, container: null, item_type: "book",
}]));

test("the tray says it continues only while chips are below its edge, and keeps each full key (#746)", () => {
  // JSDOM has no layout: a tray of 100 px showing 40 px at a time.
  let top = 0;
  vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockReturnValue(100);
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(40);
  vi.spyOn(HTMLElement.prototype, "scrollTop", "get").mockImplementation(() => top);
  render(<SelectionTray announce={false} selected={chosen} removeLabel={(r) => `Remove ${r.key}`} onRemove={() => undefined} />);
  const tray = screen.getByRole("list", { name: "Selected references" });
  expect(tray.hasAttribute("data-more")).toBe(true);
  top = 60;
  fireEvent.scroll(tray);
  expect(tray.hasAttribute("data-more")).toBe(false);
  top = 20;
  fireEvent.scroll(tray);
  expect(tray.hasAttribute("data-more")).toBe(true);
  // A shortened chip still names its whole key, to the pointer and to assistive technology.
  expect(screen.getByText("synthetic_0_long_key").getAttribute("title")).toBe("synthetic_0_long_key");
  expect(screen.getByRole("button", { name: "Remove synthetic_0_long_key" })).toBeTruthy();
});
