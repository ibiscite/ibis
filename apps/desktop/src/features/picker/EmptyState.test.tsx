import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { testStates } from "../../test-fixtures";
import { EmptyState } from "./EmptyState";
import { LibrarySetup, type EmptyLibraryChoices } from "../../LibrarySetup";

afterEach(() => { cleanup(); vi.useRealTimers(); });
const setup = (focus: boolean, empty?: EmptyLibraryChoices) => <LibrarySetup
  api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined }}
  perform={async (_action, run) => { await run(); }} autoFocus={focus} empty={empty} />;
const loaded = testStates.find((state) => state.id === "no-results")!.library;
const opening = { ...loaded, has_snapshot: false, references: 0 };

function mount(query = "", empty = false) {
  const choose = vi.fn(), clear = vi.fn(), searchAll = vi.fn();
  const { container } = render(<EmptyState library={empty ? { ...loaded, references: 0 } : loaded} query={query}
    narrowed onChooseLibrary={choose} onClearSearch={clear} onSearchAll={searchAll} onReload={vi.fn()} setup={() => <p>Setup preserved</p>} />);
  return { container, choose, clear, searchAll };
}

test("Zotero sharing explains reading references separately from macOS paste permission", () => {
  const { container } = render(<EmptyState library={{ ...loaded, state: "error", has_snapshot: false, zotero: true, problem: "zotero_access_off" }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setup} />);
  expect(container.textContent).toContain("To read your references, in Zotero › Settings › Advanced turn on “Allow other applications on this computer to communicate with Zotero”.");
  expect(container.textContent).not.toContain("macOS");
});

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

test.each(["loading", "reloading"] as const)("%s stays blank for 400 ms, then shows the brand mark, explanation and progress", async (state) => {
  vi.useFakeTimers();
  const { container } = render(<EmptyState library={{ ...opening, state }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setup} />);
  expect(container.textContent).toBe("");
  await act(() => vi.advanceTimersByTimeAsync(399));
  expect(container.textContent).toBe("");
  await act(() => vi.advanceTimersByTimeAsync(1));
  const title = `${state === "loading" ? "Opening" : "Reloading"} example.bib…`;
  expect(screen.getByRole("progressbar", { name: title })).toBeTruthy();
  expect(screen.getByRole("status").getAttribute("aria-live")).toBe("polite");
  expect(container.querySelector(".opening-mark")?.getAttribute("aria-hidden")).toBe("true");
  expect(screen.getByText("Reading your library. Zotero can stay closed.")).toBeTruthy();
  expect(container.querySelector(".update-track")).toBeTruthy();
  expect(container.querySelector(".empty-spinner, .empty-actions")).toBeNull();
  expect(screen.queryByRole("button")).toBeNull();
});

test("a fast open cancels its loader; another filename starts its own delay", async () => {
  vi.useFakeTimers();
  const props = { query: "", narrowed: false, onChooseLibrary: vi.fn(), onClearSearch: vi.fn(), onSearchAll: vi.fn(), onReload: vi.fn(), setup: () => null };
  const { rerender } = render(<EmptyState {...props} library={{ ...opening, state: "loading" }} />);
  await act(() => vi.advanceTimersByTimeAsync(200));
  rerender(<EmptyState {...props} library={loaded} />);
  await act(() => vi.advanceTimersByTimeAsync(400));
  expect(screen.queryByRole("progressbar")).toBeNull();
  rerender(<EmptyState {...props} library={{ ...opening, state: "loading" }} />);
  await act(() => vi.advanceTimersByTimeAsync(400));
  expect(screen.getByRole("progressbar", { name: "Opening example.bib…" })).toBeTruthy();
  rerender(<EmptyState {...props} library={{ ...opening, state: "loading", name: "other.bib" }} />);
  expect(screen.queryByRole("progressbar")).toBeNull();
  await act(() => vi.advanceTimersByTimeAsync(399));
  expect(screen.queryByRole("progressbar")).toBeNull();
  await act(() => vi.advanceTimersByTimeAsync(1));
  expect(screen.getByRole("progressbar", { name: "Opening other.bib…" })).toBeTruthy();
});

test("the delayed loader shortens mixed-script names visually and exposes the full name once", async () => {
  vi.useFakeTimers();
  const name = `دراسة-研究-${"🧑🏽‍🔬e\u0301".repeat(50)}-library.bib`;
  const { container } = render(<EmptyState library={{ ...opening, state: "loading", name }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setup} />);
  await act(() => vi.advanceTimersByTimeAsync(400));
  const loader = screen.getByRole("progressbar", { name: `Opening ${name}…` });
  const title = container.querySelector(".opening-title");
  expect(title?.textContent).toMatch(/^Opening دراسة-研究-.*….*library\.bib…$/);
  expect(title?.textContent?.length).toBeLessThan(name.length);
  expect(title?.getAttribute("aria-hidden")).toBe("true");
  expect(loader.getAttribute("aria-valuenow")).toBeNull();
  expect(container.querySelector(".visually-hidden")).toBeNull();
});

test.each([null, ""])("a missing loading name has the same visible and accessible fallback (%s)", async (name) => {
  vi.useFakeTimers();
  render(<EmptyState library={{ ...opening, state: "loading", name }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setup} />);
  await act(() => vi.advanceTimersByTimeAsync(400));
  expect(screen.getByRole("progressbar", { name: "Opening library…" })).toBeTruthy();
  expect(screen.getByText("Opening library…")).toBeTruthy();
});

test("closing a slow opening cancels its timer and reopening gets a fresh delay", async () => {
  vi.useFakeTimers();
  const props = { library: { ...opening, state: "loading" as const }, query: "", narrowed: false,
    onChooseLibrary: vi.fn(), onClearSearch: vi.fn(), onSearchAll: vi.fn(), onReload: vi.fn(), setup };
  const first = render(<EmptyState {...props} />);
  await act(() => vi.advanceTimersByTimeAsync(350));
  first.unmount();
  render(<EmptyState {...props} />);
  await act(() => vi.advanceTimersByTimeAsync(399));
  expect(screen.queryByRole("progressbar")).toBeNull();
  await act(() => vi.advanceTimersByTimeAsync(1));
  expect(screen.getAllByRole("progressbar")).toHaveLength(1);
});

test("a refresh with retained references never adds an opening screen above them", async () => {
  vi.useFakeTimers();
  render(<EmptyState library={{ ...loaded, state: "reloading", has_snapshot: true, references: 3 }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setup} />);
  await act(() => vi.advanceTimersByTimeAsync(2000));
  expect(screen.queryByRole("progressbar")).toBeNull();
  expect(screen.queryByText(/Reading your library/)).toBeNull();
});

test("no matches retain clear-search and search-all actions", () => {
  const { container, clear, searchAll } = mount("unmatched");
  expect(container.querySelector(".empty-actions")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Clear Search" }));
  fireEvent.click(screen.getByRole("button", { name: "Search All Zotero Libraries" }));
  expect(clear).toHaveBeenCalledOnce();
  expect(searchAll).toHaveBeenCalledOnce();
});

test("no matches keep one suggested clear-search action and a quieter search-all action", () => {
  const { container } = mount("unmatched");
  expect(container.querySelectorAll('.empty-actions .ui-button[data-variant="primary"]')).toHaveLength(1);
  expect(container.querySelector('.empty-actions .ui-button[data-variant="primary"]')?.textContent).toBe("Clear Search");
  expect(container.querySelector('.empty-alternative')?.textContent).toBe("Search All Zotero Libraries");
});

test.each(["Clear Search", "Search All Zotero Libraries"])("no matches: Enter and Space invoke only %s once", (label) => {
  const { clear, searchAll } = mount("unmatched");
  const handler = label === "Clear Search" ? clear : searchAll;
  const other = label === "Clear Search" ? searchAll : clear;
  const button = screen.getByRole("button", { name: label });
  button.focus();
  for (const key of ["Enter", " "]) {
    const before = handler.mock.calls.length;
    fireEvent.keyDown(button, { key }); fireEvent.keyDown(button, { key, repeat: true }); fireEvent.keyUp(button, { key });
    expect(handler).toHaveBeenCalledTimes(before + 1);
  }
  expect(other).not.toHaveBeenCalled();
});

test.each(["🧑🏽‍🔬", "e\u0301"])("a long empty-library name keeps its extension, full accessible text and graphemes (%s)", (unit) => {
  const name = `دراسة-研究-${unit.repeat(50)}-library.bib`;
  const { container } = render(<EmptyState library={{ ...loaded, references: 0, snapshot_name: name }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setup} />);
  const title = container.querySelector(".choice-title")!;
  const visual = title.querySelector('[aria-hidden="true"]')!;
  expect(visual).toBeTruthy();
  expect(visual.textContent).toContain("…");
  expect(visual.textContent).toMatch(/library\.bib$/);
  expect([...new Intl.Segmenter().segment(visual.textContent!)].length).toBeLessThanOrEqual(57);
  expect(visual.textContent?.replace("No references in دراسة-研究-", "").replace("-library.bib", "").replace("…", "").replaceAll(unit, "")).toBe("");
  expect(title.querySelector(".visually-hidden")?.textContent).toBe(`No references in ${name}`);
  expect(screen.getByRole("option", { name: /^BibTeX, BibLaTeX or CSL-JSON file/ })).toBeTruthy();
});

test.each([null, ""])("a missing empty-library snapshot name keeps a meaningful title (%s)", (snapshot_name) => {
  const { container } = render(<EmptyState library={{ ...loaded, references: 0, snapshot_name }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setup} />);
  expect(container.querySelector(".choice-title")?.textContent).toBe("No references in this library");
});

test("markup in an empty-library name remains plain text", () => {
  const name = '<img src=x onerror="alert(1)">.bib';
  const { container } = render(<EmptyState library={{ ...loaded, references: 0, snapshot_name: name }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setup} />);
  expect(container.querySelector(".choice-title")?.textContent).toBe(`No references in ${name}`);
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
      onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setup} />);
    expect(container.querySelector(".choice-title")?.textContent).toBe(`No references in ${name}`);
    expect(screen.getByRole("option", { name: /^BibTeX, BibLaTeX or CSL-JSON file/ })).toBeTruthy();
  } finally {
    vi.unstubAllGlobals();
  }
});


test.each([false, true])("C12 zero-reference choices offer Zotero and a file, never no-match actions (Zotero=%s)", (zotero) => {
  const choose = vi.fn(async () => undefined), useZotero = vi.fn(async () => undefined);
  render(<EmptyState library={{ ...loaded, state: "loaded", references: 0, zotero }} query="unmatched" narrowed
    onChooseLibrary={choose} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()}
    setup={(focus, empty) => <LibrarySetup api={{ chooseLibrary: choose, useZotero }}
      perform={async (_action, run) => { await run(); }} autoFocus={focus} empty={empty} />} />);
  expect(screen.getAllByRole("option").map((row) => row.querySelector(".result-title")?.textContent))
    .toEqual(["Zotero", "BibTeX, BibLaTeX or CSL-JSON file"]);
  expect(screen.getByText("You can drop a file here too.")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Clear Search" })).toBeNull();
});

test("C12 missing-file choices name the requested file and keep both actions after cancellation", () => {
  const choose = vi.fn();
  render(<EmptyState library={{ ...loaded, state: "error", references: 0, has_snapshot: false,
    name: "requested 研究.bib", snapshot_name: "previous.bib", problem: "missing" }} query="" narrowed={false}
    onChooseLibrary={choose} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setup} />);
  const rows = screen.getAllByRole("option");
  expect(rows.map((row) => row.querySelector(".result-title")?.textContent))
    .toEqual(["Find requested 研究.bib", "Use another library"]);
  fireEvent.click(rows[0]!);
  expect(choose).toHaveBeenCalledOnce();
  expect(screen.getByRole("option", { name: /^Find requested 研究.bib/ })).toBeTruthy();
  expect(screen.queryByText(/previous.bib/)).toBeNull();
});
