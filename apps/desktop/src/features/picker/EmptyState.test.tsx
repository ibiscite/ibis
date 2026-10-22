import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { galleryStates } from "../../gallery-fixtures";
import { EmptyState } from "./EmptyState";

afterEach(() => { cleanup(); vi.useRealTimers(); });
const loaded = galleryStates.find((state) => state.id === "no-results")!.library;

function mount(query = "", empty = false) {
  const choose = vi.fn(), clear = vi.fn(), searchAll = vi.fn();
  const { container } = render(<EmptyState library={empty ? { ...loaded, references: 0 } : loaded} query={query}
    narrowed onChooseLibrary={choose} onClearSearch={clear} onSearchAll={searchAll} onReload={vi.fn()} setup={<p>Setup preserved</p>} />);
  return { container, choose, clear, searchAll };
}

test.each(["", "unmatched"])("one empty-state template has a plain title, description and decorative icon (query=%s)", (query) => {
  const { container } = mount(query);
  expect(container.querySelector(".empty-title")?.textContent).toBe(query ? "No matching references" : "Type to find a reference");
  expect(container.querySelector(".empty-description")?.textContent).toBe(query
    ? "Try a surname, year or fewer words."
    : "Search by title, author, year, citation key or DOI.");
  expect(container.querySelector(".empty-icon")?.getAttribute("aria-hidden")).toBe("true");
  expect(screen.queryByRole("heading")).toBeNull();
});

test("idle instructions show only the two decorative key hints without an action", () => {
  const { container } = mount();
  const keys = container.querySelector(".empty-keys");
  expect(keys?.getAttribute("aria-hidden")).toBe("true");
  expect([...container.querySelectorAll(".key-hint-label")].map((label) => label.textContent)).toEqual(["Copy", "Add to citation"]);
  expect([...container.querySelectorAll(".key-hint")].every((hint) => hint.firstElementChild?.className === "key-hint-label")).toBe(true);
  expect(screen.queryByRole("button")).toBeNull();
  expect(container.querySelector(".empty-actions")).toBeNull();
});

test.each(["loading", "reloading"] as const)("%s stays blank for 400 ms, then shows only a spinner and the filename", async (state) => {
  vi.useFakeTimers();
  const { container } = render(<EmptyState library={{ ...loaded, state }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={null} />);
  expect(container.textContent).toBe("");
  await act(() => vi.advanceTimersByTimeAsync(399));
  expect(container.textContent).toBe("");
  await act(() => vi.advanceTimersByTimeAsync(1));
  const title = `${state === "loading" ? "Opening" : "Reloading"} example.bib…`;
  expect(screen.getByRole("progressbar", { name: title })).toBeTruthy();
  expect(screen.getByRole("status").getAttribute("aria-live")).toBe("polite");
  expect(container.querySelector(".empty-spinner")?.getAttribute("aria-hidden")).toBe("true");
  expect(container.textContent).toBe(title);
  expect(container.querySelector(".empty-description, .empty-icon, .empty-actions")).toBeNull();
  expect(screen.queryByRole("button")).toBeNull();
});

test("a fast open cancels its loader; another filename starts its own delay", async () => {
  vi.useFakeTimers();
  const props = { query: "", narrowed: false, onChooseLibrary: vi.fn(), onClearSearch: vi.fn(), onSearchAll: vi.fn(), onReload: vi.fn(), setup: null };
  const { rerender } = render(<EmptyState {...props} library={{ ...loaded, state: "loading" }} />);
  await act(() => vi.advanceTimersByTimeAsync(200));
  rerender(<EmptyState {...props} library={loaded} />);
  await act(() => vi.advanceTimersByTimeAsync(400));
  expect(screen.queryByRole("progressbar")).toBeNull();
  rerender(<EmptyState {...props} library={{ ...loaded, state: "loading" }} />);
  await act(() => vi.advanceTimersByTimeAsync(400));
  expect(screen.getByRole("progressbar", { name: "Opening example.bib…" })).toBeTruthy();
  rerender(<EmptyState {...props} library={{ ...loaded, state: "loading", name: "other.bib" }} />);
  expect(screen.queryByRole("progressbar")).toBeNull();
  await act(() => vi.advanceTimersByTimeAsync(399));
  expect(screen.queryByRole("progressbar")).toBeNull();
  await act(() => vi.advanceTimersByTimeAsync(1));
  expect(screen.getByRole("progressbar", { name: "Opening other.bib…" })).toBeTruthy();
});

test("the delayed loader shortens mixed-script names visually and exposes the full name once", async () => {
  vi.useFakeTimers();
  const name = `دراسة-研究-${"🧑🏽‍🔬e\u0301".repeat(50)}-library.bib`;
  const { container } = render(<EmptyState library={{ ...loaded, state: "loading", name }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={null} />);
  await act(() => vi.advanceTimersByTimeAsync(400));
  const loader = screen.getByRole("progressbar", { name: `Opening ${name}…` });
  expect(loader.textContent).toMatch(/^Opening دراسة-研究-.*….*library\.bib…$/);
  expect(loader.textContent?.length).toBeLessThan(name.length);
  expect(loader.querySelector("span")?.getAttribute("aria-hidden")).toBe("true");
  expect(container.querySelector(".visually-hidden")).toBeNull();
});

test.each([null, ""])("a missing loading name has the same visible and accessible fallback (%s)", async (name) => {
  vi.useFakeTimers();
  render(<EmptyState library={{ ...loaded, state: "loading", name }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={null} />);
  await act(() => vi.advanceTimersByTimeAsync(400));
  expect(screen.getByRole("progressbar", { name: "Opening library…" }).textContent).toBe("Opening library…");
});

test("no matches retain clear-search and search-all actions", () => {
  const { container, clear, searchAll } = mount("unmatched");
  expect(container.querySelector(".empty-actions")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Clear search" }));
  fireEvent.click(screen.getByRole("button", { name: "Search all Zotero libraries" }));
  expect(clear).toHaveBeenCalledOnce();
  expect(searchAll).toHaveBeenCalledOnce();
});

test("an empty file gets a separate title and recovery line, keeping the chooser", () => {
  const { container, choose } = mount("", true);
  expect(container.querySelector(".empty-title")?.textContent).toBe("No references in example.bib");
  expect(container.querySelector(".empty-description")?.textContent).toBe("Choose a file with references.");
  fireEvent.click(screen.getByRole("button", { name: "Choose library…" }));
  expect(choose).toHaveBeenCalledOnce();
});

test("an empty Zotero library offers reload as well as choosing another library", () => {
  const reload = vi.fn(), choose = vi.fn();
  render(<EmptyState library={{ ...loaded, zotero: true, references: 0 }} query="" narrowed={false}
    onChooseLibrary={choose} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={reload} setup={null} />);
  fireEvent.click(screen.getByRole("button", { name: "Reload" }));
  fireEvent.click(screen.getByRole("button", { name: "Choose library…" }));
  expect(reload).toHaveBeenCalledOnce();
  expect(choose).toHaveBeenCalledOnce();
});

test.each(["file", "zotero", "no-results"])("%s has exactly one enabled suggested action, with quieter alternatives", (state) => {
  const empty = state !== "no-results";
  const { container } = render(<EmptyState library={{ ...loaded, references: empty ? 0 : 24, zotero: state === "zotero" }}
    query={empty ? "" : "unmatched"} narrowed onChooseLibrary={vi.fn()} onClearSearch={vi.fn()}
    onSearchAll={vi.fn()} onReload={vi.fn()} setup={null} />);
  const actions = container.querySelectorAll<HTMLButtonElement>(".empty-actions .suggested-action");
  expect(actions).toHaveLength(1);
  expect(actions.item(0).disabled).toBe(false);
  expect(actions.item(0).textContent).toBe(state === "file" ? "Choose library…" : state === "zotero" ? "Reload" : "Clear search");
  const alternative = container.querySelector(".empty-alternative");
  expect(alternative?.textContent ?? null).toBe(state === "file" ? null : state === "zotero" ? "Choose library…" : "Search all Zotero libraries");
});

test.each(["🧑🏽‍🔬", "e\u0301"])("a long empty-library name keeps its extension, full accessible text and graphemes (%s)", (unit) => {
  const name = `دراسة-研究-${unit.repeat(50)}-library.bib`;
  const { container } = render(<EmptyState library={{ ...loaded, references: 0, snapshot_name: name }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={null} />);
  const title = container.querySelector(".empty-title")!;
  const visual = title.querySelector('[aria-hidden="true"]')!;
  expect(visual).toBeTruthy();
  expect(visual.textContent).toContain("…");
  expect(visual.textContent).toMatch(/library\.bib$/);
  expect([...new Intl.Segmenter().segment(visual.textContent!)].length).toBeLessThanOrEqual(57);
  expect(visual.textContent?.replace("No references in دراسة-研究-", "").replace("-library.bib", "").replace("…", "").replaceAll(unit, "")).toBe("");
  expect(title.querySelector(".visually-hidden")?.textContent).toBe(`No references in ${name}`);
  expect(screen.getByRole("button", { name: "Choose library…" })).toBeTruthy();
});

test.each([null, ""])("a missing empty-library snapshot name keeps a meaningful title (%s)", (snapshot_name) => {
  const { container } = render(<EmptyState library={{ ...loaded, references: 0, snapshot_name }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={null} />);
  expect(container.querySelector(".empty-title")?.textContent).toBe("No references in this library");
});

test("markup in an empty-library name remains plain text", () => {
  const name = '<img src=x onerror="alert(1)">.bib';
  const { container } = render(<EmptyState library={{ ...loaded, references: 0, snapshot_name: name }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={null} />);
  expect(container.querySelector(".empty-title")?.textContent).toBe(`No references in ${name}`);
  expect(container.querySelector("img")).toBeNull();
});

test("a webview without Segmenter can import and render the complete name", async () => {
  const legacyIntl = Object.create(Intl) as typeof Intl;
  Object.defineProperty(legacyIntl, "Segmenter", { value: undefined });
  vi.stubGlobal("Intl", legacyIntl);
  try {
    vi.resetModules();
    const { EmptyState: LegacyEmptyState } = await import("./EmptyState");
    const name = `${"🧑🏽‍🔬e\u0301".repeat(50)}.bib`;
    const { container } = render(<LegacyEmptyState library={{ ...loaded, references: 0, snapshot_name: name }} query="" narrowed={false}
      onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={null} />);
    expect(container.querySelector(".empty-title")?.textContent).toBe(`No references in ${name}`);
    expect(screen.getByRole("button", { name: "Choose library…" })).toBeTruthy();
  } finally {
    vi.unstubAllGlobals();
  }
});
