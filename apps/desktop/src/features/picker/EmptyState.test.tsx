import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { testStates } from "../../test-fixtures";
import type { LibraryStatus, ZoteroProfiles } from "../../api";
import { EmptyState, ZOTERO_CHECK_EVERY, type ZoteroRecovery } from "./EmptyState";
import { LibrarySetup, type EmptyLibraryChoices } from "../../LibrarySetup";

afterEach(() => { cleanup(); vi.useRealTimers(); });
const setup = (focus: boolean, empty?: EmptyLibraryChoices) => <LibrarySetup
  api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined }}
  perform={async (_action, run) => { await run(); }} autoFocus={focus} empty={empty} />;
const loaded = testStates.find((state) => state.id === "no-results")!.library;
const opening = { ...loaded, has_snapshot: false, references: 0 };

function mount(query = "", empty = false, narrowed = false) {
  const choose = vi.fn(), clear = vi.fn(), searchAll = vi.fn();
  const { container } = render(<EmptyState library={empty ? { ...loaded, references: 0 } : loaded} query={query}
    narrowed={narrowed} onChooseLibrary={choose} onClearSearch={clear} onSearchAll={searchAll} onReload={vi.fn()} setup={() => <p>Setup preserved</p>} />);
  return { container, choose, clear, searchAll };
}

test("Zotero sharing explains reading references separately from macOS paste permission", () => {
  const { container } = render(<EmptyState library={{ ...loaded, state: "error", has_snapshot: false, zotero: true, problem: "zotero_access_off" }} query="" narrowed={false}
    onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setup} />);
  expect(container.textContent).toContain("To read your references, in Zotero › Settings › Advanced turn on “Allow other applications on this computer to communicate with Zotero”.");
  expect(container.textContent).not.toContain("macOS");
});

const waiting = (problem: "zotero_access_off" | "zotero_closed" | "zotero_profiles" | "zotero_missing") =>
  ({ ...loaded, state: "error" as const, has_snapshot: false, zotero: true, problem });
const recovery = (library: LibraryStatus, zotero: ZoteroRecovery, setupNode = setup) => render(<EmptyState library={library} query="" narrowed={false}
  onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setupNode} zotero={zotero} />);
const ask = async (ms: number) => { await act(async () => { await vi.advanceTimersByTimeAsync(ms); }); };

describe("a screen waiting on Zotero", () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => {
    document.documentElement.removeAttribute("data-picker-hidden");
    vi.useRealTimers();
  });

  test.each(["zotero_access_off", "zotero_closed", "zotero_missing"] as const)("%s asks Zotero only while it is shown", async (problem) => {
    const checkSharing = vi.fn(async () => "access_off");
    const { unmount } = recovery(waiting(problem), { checkSharing });
    expect(checkSharing).not.toHaveBeenCalled();
    await ask(ZOTERO_CHECK_EVERY);
    expect(checkSharing).toHaveBeenCalledTimes(1);
    await ask(ZOTERO_CHECK_EVERY);
    expect(checkSharing).toHaveBeenCalledTimes(2);
    // Hidden, nothing polls; shown again, it resumes.
    await act(async () => { document.documentElement.setAttribute("data-picker-hidden", ""); await Promise.resolve(); });
    await ask(ZOTERO_CHECK_EVERY * 5);
    expect(checkSharing).toHaveBeenCalledTimes(2);
    await act(async () => { document.documentElement.removeAttribute("data-picker-hidden"); await Promise.resolve(); });
    await ask(ZOTERO_CHECK_EVERY);
    expect(checkSharing).toHaveBeenCalledTimes(3);
    unmount();
    await ask(ZOTERO_CHECK_EVERY * 5);
    expect(checkSharing).toHaveBeenCalledTimes(3);
  });

  test("a slow answer is never overlapped, and a failed one tries again", async () => {
    let answer: (value: string) => void = () => {};
    const checkSharing = vi.fn(() => new Promise<string>((resolve) => { answer = resolve; }));
    recovery(waiting("zotero_access_off"), { checkSharing });
    await ask(ZOTERO_CHECK_EVERY * 5);
    expect(checkSharing).toHaveBeenCalledTimes(1);
    await act(async () => { answer("access_off"); await Promise.resolve(); });
    await ask(ZOTERO_CHECK_EVERY);
    expect(checkSharing).toHaveBeenCalledTimes(2);
  });

  test("the other screens ask nothing", async () => {
    const checkSharing = vi.fn(async () => "available");
    recovery({ ...loaded, state: "error", has_snapshot: false, problem: "missing" }, { checkSharing });
    await ask(ZOTERO_CHECK_EVERY * 3);
    expect(checkSharing).not.toHaveBeenCalled();
  });

  test("leaving for another library stops the check", async () => {
    const checkSharing = vi.fn(async () => "access_off");
    recovery(waiting("zotero_access_off"), { checkSharing });
    fireEvent.click(screen.getByRole("option", { name: /Use Another Library/ }));
    await ask(ZOTERO_CHECK_EVERY * 3);
    expect(checkSharing).not.toHaveBeenCalled();
  });
});

test("no Zotero here says so and offers the way on, never Open Zotero", () => {
  const openSource = vi.fn();
  render(<EmptyState library={waiting("zotero_missing")} query="" narrowed={false} onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()}
    onReload={vi.fn()} setup={setup} />);
  expect(screen.getByText("Ibis can’t find Zotero’s library")).toBeTruthy();
  expect(screen.getByRole("option", { name: /Use Another Library/ }).getAttribute("data-suggested")).toBe("true");
  expect(screen.getByRole("option", { name: /Try Again/ })).toBeTruthy();
  expect(screen.queryByRole("option", { name: /Open Zotero/ })).toBeNull();
  expect(openSource).not.toHaveBeenCalled();
});

test("several profiles are offered by name and a choice names only that", async () => {
  const chooseProfile = vi.fn();
  recovery(waiting("zotero_profiles"), { listProfiles: async () => ({ names: ["Home", "Work"], chosen: null, running: false }), chooseProfile });
  expect(screen.getByText("Which Zotero profile?")).toBeTruthy();
  fireEvent.click(await screen.findByRole("option", { name: /Work/ }));
  expect(chooseProfile).toHaveBeenCalledExactlyOnceWith("Work");
});

test("a list of profiles that cannot be read still leaves a way forward", async () => {
  const chooseProfile = vi.fn();
  recovery(waiting("zotero_profiles"), { listProfiles: async () => { throw new Error("refused"); }, chooseProfile });
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", expect.stringContaining("could not list your Zotero profiles"));
  expect(screen.getByRole("option", { name: /Try Again/ })).toBeTruthy();
  expect(screen.getByRole("option", { name: /Use Another Library/ })).toBeTruthy();
  expect(chooseProfile).not.toHaveBeenCalled();
});

test("a list that answers after the screen left changes nothing", async () => {
  let answer: (found: ZoteroProfiles) => void = () => {};
  const { unmount } = recovery(waiting("zotero_profiles"), { listProfiles: () => new Promise((resolve) => { answer = resolve; }) });
  unmount();
  await act(async () => { answer({ names: ["Home"], chosen: null, running: false }); await Promise.resolve(); });
  expect(screen.queryByRole("option", { name: /Home/ })).toBeNull();
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

const obstacle = (props: Partial<Parameters<typeof EmptyState>[0]>) => {
  const handlers = { onClearSearch: vi.fn(), onSearchAll: vi.fn(), onRemoveFilter: vi.fn() };
  const { container } = render(<EmptyState library={loaded} query="smith #sleep" narrowed={false} onChooseLibrary={vi.fn()} onReload={vi.fn()} setup={() => null} {...handlers} {...props} />);
  return { ...handlers, title: container.querySelector(".empty-title")?.textContent, buttons: screen.getAllByRole("button").map((button) => button.textContent) };
};

test("a filter is named, with one fix that keeps the plain words", () => {
  const { title, buttons, onRemoveFilter } = obstacle({ unfiltered: "smith" });
  expect(title).toBe("No matches with this filter");
  expect(buttons).toEqual(["Remove Filter"]);
  fireEvent.click(screen.getByRole("button", { name: "Remove Filter" }));
  expect(onRemoveFilter).toHaveBeenCalledWith("smith");
});

test("left-out libraries first say nothing matched, then offer searching them all", () => {
  const { title, buttons } = obstacle({ narrowed: true, query: "qxzq" });
  expect(title).toBe("No matching references");
  expect(document.querySelector(".empty-description")?.textContent).toBe("Some of your Zotero libraries are left out of this search.");
  expect(buttons).toEqual(["Search All Zotero Libraries"]);
});

test("a pasted citation no library holds says so", () => {
  const { title, buttons } = obstacle({ missing: true });
  expect(title).toBe("Not in your libraries");
  expect(buttons).toEqual(["Clear Search"]);
});

test("a filter goes first: it is the nearest thing to undo", () => {
  expect(obstacle({ unfiltered: "smith", narrowed: true, missing: true }).title).toBe("No matches with this filter");
});

test("plain no matches, an empty library and a source failure stay different", () => {
  expect(obstacle({}).title).toBe("No matching references");
  cleanup();
  const { container: empty } = render(<EmptyState library={{ ...loaded, references: 0 }} query="x" narrowed={false} onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={() => <p>Add references</p>} />);
  expect(empty.textContent).toContain("Add references");
  expect(empty.textContent).not.toContain("No matching references");
  cleanup();
  const { container: failed } = render(<EmptyState library={{ ...loaded, state: "error", has_snapshot: false, problem: "unreadable" }} query="x" narrowed={false} onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={() => null} />);
  expect(failed.textContent).not.toContain("No matching references");
  expect(screen.getByRole("alert")).toBeTruthy();
});

test("a new library's hint shows words only: no keys, no action [C11]", () => {
  const { container } = mount();
  expect(container.querySelector(".empty-title")?.textContent).toBe("Type to find a reference");
  expect(container.querySelector(".empty-keys, .key-hint, kbd")).toBeNull();
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

test("no matches offer the one fix for what is in the way", () => {
  const { container, clear } = mount("unmatched");
  expect(container.querySelectorAll(".empty-actions .ui-button")).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: "Clear Search" }));
  expect(clear).toHaveBeenCalledOnce();
  cleanup();
  const { container: scoped, searchAll } = mount("unmatched", false, true);
  expect(scoped.querySelectorAll('.empty-actions .ui-button[data-variant="primary"]')).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: "Search All Zotero Libraries" }));
  expect(searchAll).toHaveBeenCalledOnce();
});

test.each([["Clear Search", false], ["Search All Zotero Libraries", true]] as const)("no matches: Enter and Space invoke only %s once", (label, narrowed) => {
  const { clear, searchAll } = mount("unmatched", false, narrowed);
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
  expect(screen.getByRole("option", { name: /^Choose a Library File…/ })).toBeTruthy();
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
    expect(screen.getByRole("option", { name: /^Choose a Library File…/ })).toBeTruthy();
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
    .toEqual(["Use Zotero", "Choose a Library File…"]);
  expect(screen.getByText("Or drag a .bib or CSL-JSON file into this window.")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Clear Search" })).toBeNull();
});

test("C12 missing-file choices name the requested file and keep both actions after cancellation", () => {
  const choose = vi.fn();
  render(<EmptyState library={{ ...loaded, state: "error", references: 0, has_snapshot: false,
    name: "requested 研究.bib", snapshot_name: "previous.bib", problem: "missing" }} query="" narrowed={false}
    onChooseLibrary={choose} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setup} />);
  const rows = screen.getAllByRole("option");
  expect(rows.map((row) => row.querySelector(".result-title")?.textContent))
    .toEqual(["Find requested 研究.bib", "Use Another Library"]);
  fireEvent.click(rows[0]!);
  expect(choose).toHaveBeenCalledOnce();
  expect(screen.getByRole("option", { name: /^Find requested 研究.bib/ })).toBeTruthy();
  expect(screen.queryByText(/previous.bib/)).toBeNull();
});
