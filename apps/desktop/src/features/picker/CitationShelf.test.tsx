import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useRef, useState } from "react";
import type { SearchResult } from "../../api";
import type { FormattedCitation } from "../../format/formatted";
import { chipLabels, CitationShelf } from "./CitationShelf";
import axe from "axe-core";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const records: SearchResult[] = ["a", "b", "c"].map((key) => ({ revision: "1", id: { source: `source-${key}`, native_key: "shared" },
  key, title: `Synthetic ${key}`, authors: "García", year: 2021, matched: ["title"], first_author: "García", author_count: 1, container: null, item_type: "book" }));
const selected = (count = 3) => new Map(records.slice(0, count).map((r) => [r.id.source, r]));
const formatted = (order = [0, 1, 2]): FormattedCitation => ({ markup: [], text: `(${order.map((i) => `@${records[i]!.key}`).join("; ")})`, html: "", warnings: [],
  segments: [{ item: null, markup: ["("] }, ...order.flatMap((item, index) => [...(index ? [{ item: null, markup: ["; "] }] : []), { item, markup: [`@${records[item]!.key}`] }]), { item: null, markup: [")"] }] });
const shelf = (props: Partial<Parameters<typeof CitationShelf>[0]> = {}) => <CitationShelf announce={false} selected={selected()} removeLabel={(r) => `Remove ${r.key}`}
  onRemove={() => undefined} style={null} keys shown={formatted()} focusable previewIds={records.map((r) => r.id.source)} {...props} />;
const group = () => screen.getByRole("grid", { name: "Selected references" });
const names = () => within(group()).getAllByRole("row").map((r) => r.getAttribute("aria-label"));

test("labels disambiguate repeated author/year without using citation keys as identities", () => {
  expect(chipLabels(records)).toEqual(["García 2021 · a", "García 2021 · b", "García 2021 · c"]);
});

test("authoritative sorted segments preserve canonical punctuation and caller identity", async () => {
  const removed = vi.fn();
  const { container } = render(shelf({ shown: formatted([2, 0, 1]), onRemove: removed }));
  expect(names()).toEqual(["García 2021 · c", "García 2021 · a", "García 2021 · b"]);
  expect([...container.querySelectorAll(".citation-text")].map((r) => r.textContent)).toEqual(["@c", "@a", "@b"]);
  expect(screen.getByRole("region", { name: "Preview" }).title).toBe("(@c; @a; @b)");
  fireEvent.click(screen.getByRole("button", { name: "Remove a" }));
  expect(removed).toHaveBeenCalledExactlyOnceWith(new Set(["source-a"]));
  expect((await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } })).violations).toEqual([]);
});

test("pending, reordered and opaque replies keep one focused collection and surviving order", () => {
  const view = render(shelf({ selected: selected(2), shown: formatted([1, 0]), previewIds: ["source-a", "source-b"] }));
  const grid = group();
  const focused = within(grid).getAllByRole("row")[0]!;
  act(() => focused.focus());
  view.rerender(shelf({ shown: null }));
  expect(group()).toBe(grid); expect(document.activeElement).toBe(focused);
  expect(within(grid).getByText("@b")).toBeTruthy();
  expect(names()).toEqual(["García 2021 · b", "García 2021 · a", "García 2021 · c"]);
  view.rerender(shelf({ shown: formatted([2, 1, 0]) }));
  expect(group()).toBe(grid); expect(document.activeElement).toBe(focused);
  expect(names()).toEqual(["García 2021 · c", "García 2021 · b", "García 2021 · a"]);
  view.rerender(shelf({ shown: { markup: [], text: "[1–3]", html: "", warnings: [], segments: [{ item: null, markup: ["[1–3]"] }] } }));
  expect(group()).toBe(grid); expect(document.activeElement).toBe(focused);
  expect(screen.getByText("[1–3]")).toBeTruthy();
});

test.each(["partial", "wrong-text", "invalid-index", "repeated-owner"])("%s ownership keeps the canonical aggregate and all named removal controls", (kind) => {
  const made = formatted();
  if (kind === "partial") made.segments![2]!.item = 1;
  if (kind === "wrong-text") made.text = "Canonical citation";
  if (kind === "invalid-index") made.segments![1]!.item = 99;
  if (kind === "repeated-owner") made.segments![3]!.item = 0;
  render(shelf({ shown: made }));
  expect(screen.getByText(made.text)).toBeTruthy();
  expect(screen.getAllByRole("button", { name: /^Remove/ })).toHaveLength(3);
});

test("hidden output keeps all 50 references removable without folding or formatting", () => {
  const many = new Map(Array.from({ length: 50 }, (_, i) => [`identity-${i}`, { ...records[0]!, key: `key-${i}` }]));
  render(shelf({ selected: many, shown: undefined, previewIds: undefined }));
  expect(screen.queryByRole("region", { name: "Preview" })).toBeNull();
  expect(within(group()).getAllByRole("row")).toHaveLength(50);
  expect(screen.queryByText(/^\+\d/)).toBeNull();
});

test("consecutive keyboard removal returns to Search only after the last identity", async () => {
  function Picker() {
    const [chosen, setChosen] = useState(selected()); const search = useRef<HTMLInputElement>(null);
    return <><input ref={search} aria-label="Search" />{shelf({ selected: chosen, shown: null, searchRef: search,
      onRemove: (ids) => setChosen((previous) => new Map([...previous].filter(([id]) => !ids.has(id)))) })}</>;
  }
  render(<Picker />);
  act(() => within(group()).getAllByRole("row")[0]!.focus());
  for (let remaining = 2; remaining >= 0; remaining--) {
    fireEvent.keyDown(document.activeElement!, { key: "Delete" });
    await waitFor(() => expect(screen.queryAllByRole("row")).toHaveLength(remaining));
  }
  await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Search" })));
});

test("copy receipt shows its exact output, closing beak and confirmation", () => {
  const { container } = render(shelf({ selected: new Map(), shown: { markup: [], text: "Copied pair", html: "", warnings: [] }, copied: true }));
  expect(screen.getByText("Copied pair")).toBeTruthy(); expect(screen.getByText("Copied")).toBeTruthy();
  expect(container.querySelectorAll("svg.shelf-mark")).toHaveLength(2);
  for (const mark of container.querySelectorAll("svg")) { expect(mark.getAttribute("aria-hidden")).toBe("true"); expect(mark.getAttribute("focusable")).toBe("false"); }
});

test("choice and empty screens hide the shelf, and stale output keeps its recovery explanation", () => {
  const view = render(shelf({ visible: false }));
  expect(screen.queryByRole("region", { name: "Preview" })).toBeNull();
  view.rerender(shelf({ shown: "stale" }));
  expect(screen.getByRole("region", { name: "Preview" }).getAttribute("data-state")).toBe("stale");
  expect(screen.getAllByRole("button", { name: /^Remove/ })).toHaveLength(3);
});
