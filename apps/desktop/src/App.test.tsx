import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import axe from "axe-core";
import * as api from "./api";
import capability from "../src-tauri/capabilities/default.json";
import config from "../src-tauri/tauri.conf.json";

// Vitest can load these existing runtime builtins without a Node-types dependency.
const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const tokens = readFileSync(new FileURL("./tokens.css", import.meta.url), "utf8");
const styles = readFileSync(new FileURL("./styles.css", import.meta.url), "utf8");

afterEach(async () => {
  vi.useRealTimers();
  cleanup();
  // Event listeners are removed asynchronously after unmount; let that
  // finish before the mocked Tauri internals are cleared.
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
});

test("copy confirmation expires, but copy warnings remain until dismissed", async () => {
  let warnings: string[] = [];
  backend({
    search: () => [result("a", "First")],
    copy_citation: () => ({ text: "@a", warnings }),
  });
  render(<App />);
  await screen.findByText("First");
  const box = screen.getByLabelText("Search references");
  vi.useFakeTimers();
  await act(async () => { fireEvent.keyDown(box, { key: "Enter" }); });
  expect(screen.getByText("@a")).toBeTruthy();
  await act(() => vi.advanceTimersByTimeAsync(3000));
  expect(screen.queryByText("@a")).toBeNull();
  vi.useRealTimers();
  warnings = ["A warning to read"];
  fireEvent.keyDown(box, { key: "Enter" });
  await screen.findByText("A warning to read");
  vi.useFakeTimers();
  await act(() => vi.advanceTimersByTimeAsync(3000));
  expect(screen.getByText("A warning to read")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Dismiss copy warnings" }));
  expect(screen.queryByLabelText("Copy warnings")).toBeNull();
});

function result(key: string, title: string, source = "refs.bib") {
  return { id: { source, native_key: key }, key, title, authors: "Smith", year: 2020, matched: ["title"] };
}

type Handler = (args: Record<string, unknown>) => unknown;

test("the borderless picker has a permitted drag region around its editable search input", async () => {
  backend();
  render(<App />);
  const box = screen.getByLabelText("Search references");
  expect(box.closest("[data-tauri-drag-region]")).toBeTruthy();
  expect(box.hasAttribute("data-tauri-drag-region")).toBe(false);
  expect(capability.permissions).toContain("core:window:allow-start-dragging");
  fireEvent.change(box, { target: { value: "sleep" } });
  await waitFor(() => expect((box as HTMLInputElement).value).toBe("sleep"));
});

/** Mock the Rust commands; returns the list of commands called. */
function backend(overrides: Record<string, Handler> = {}): string[] {
  const calls: string[] = [];
  const defaults: Record<string, Handler> = {
    shortcut_status: () => ({ shortcut: "Control+Command+R", registered: true, error: null }),
    search: () => [],
    hide_picker: () => null,
    copy_format: () => "pandoc",
    library_status: () => ({ state: "loaded", has_snapshot: true, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null }),
  };
  mockIPC(
    (cmd, args) => {
      calls.push(cmd);
      const handler = overrides[cmd] ?? defaults[cmd];
      if (!handler) throw new Error(`unexpected command ${cmd}`);
      return handler((args ?? {}) as Record<string, unknown>);
    },
    { shouldMockEvents: true },
  );
  return calls;
}

test("typing a query shows results from the search command", async () => {
  backend({
    search: ({ query }) =>
      query ? [result("smith2020", "Sleep and memory")] : [],
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  expect(await screen.findByText("Sleep and memory")).toBeTruthy();
  expect(screen.getByText("smith2020")).toBeTruthy();
});

test("errors from the backend are shown, not swallowed", async () => {
  backend({
    search: () => {
      throw new Error("no library is open");
    },
  });
  render(<App />);
  expect((await screen.findByText("Search is unavailable; reload the library and try again.")).getAttribute("role")).toBe("alert");
});

test("first launch waits for library status and offers a chooser without a search error", async () => {
  let status: (value: unknown) => void = () => undefined;
  const calls = backend({
    library_status: () => new Promise((resolve) => { status = resolve; }),
    search: () => { throw new Error("no library is open"); },
    choose_library: () => null,
  });
  render(<App />);
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByRole("alert")).toBeNull();
  expect(calls).not.toContain("search");
  await act(async () => { status({ state: "none", has_snapshot: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null }); });
  fireEvent.click(await screen.findByRole("button", { name: "Choose library to get started" }));
  await waitFor(() => expect(calls).toContain("choose_library"));
  expect(screen.queryByRole("alert")).toBeNull();
  expect(calls).not.toContain("search");
});

test("typing after a library load failure keeps its single explanation visible", async () => {
  backend({
    library_status: () => ({ state: "error", has_snapshot: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: "the synthetic library is unreadable", note: null }),
    search: () => { throw new Error("no library is open"); },
  });
  render(<App />);
  expect((await screen.findByText(/The library could not be opened/)).textContent).toContain("synthetic library is unreadable");
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  expect(screen.queryByText("Search is unavailable; reload the library and try again.")).toBeNull();
  expect(screen.getAllByRole("alert")).toHaveLength(1);
});

test("the empty-state chooser opens the existing dialog and remains available after cancellation", async () => {
  const calls = backend({
    library_status: () => ({ state: "none", has_snapshot: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null }),
    choose_library: () => null,
  });
  render(<App />);
  const chooser = await screen.findByRole("button", { name: "Choose library to get started" });
  fireEvent.click(chooser);
  await waitFor(() => expect(calls).toContain("choose_library"));
  expect(screen.getByRole("button", { name: "Choose library to get started" })).toBe(chooser);
  expect(screen.getByRole("button", { name: "Choose library…" })).toBeTruthy();
});

test("typing and clearing before choosing a library keeps the calm hint and chooser", async () => {
  const calls = backend({
    library_status: () => ({ state: "none", has_snapshot: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null }),
    search: () => { throw new Error("no library is open"); },
    choose_library: () => null,
  });
  render(<App />);
  const chooser = await screen.findByRole("button", { name: "Choose library to get started" });
  const box = screen.getByLabelText("Search references");
  for (const value of ["sleep", ""]) {
    await act(async () => { fireEvent.change(box, { target: { value } }); });
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("Choose a library to get started")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Choose library to get started" })).toBe(chooser);
  }
  expect(calls).not.toContain("search");
  fireEvent.click(chooser);
  await waitFor(() => expect(calls).toContain("choose_library"));
  expect(screen.getByRole("button", { name: "Choose library to get started" })).toBe(chooser);
});

test("opening the first library searches the untouched empty query", async () => {
  const calls = backend({
    library_status: () => ({ state: "none", has_snapshot: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null }),
    search: () => [result("first", "First reference")],
  });
  render(<App />);
  await screen.findByRole("button", { name: "Choose library to get started" });
  expect(calls).not.toContain("search");
  await emit("library:changed", { state: "loaded", has_snapshot: true, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null });
  expect(await screen.findByText("First reference")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Choose library to get started" })).toBeNull();
});

test("a failed status request does not suppress search errors", async () => {
  backend({
    library_status: () => { throw new Error("status unavailable"); },
    search: () => { throw new Error("search unavailable"); },
  });
  render(<App />);
  expect((await screen.findByText("Search is unavailable; reload the library and try again.")).getAttribute("role")).toBe("alert");
});

test.each([
  ["resolves", false], ["rejects", false],
  ["resolves", true], ["rejects", true],
] as const)("a startup status request that %s cannot overwrite a newer library event (failed: %s)", async (outcome, failed) => {
  let resolveStatus: (value: unknown) => void = () => undefined;
  let rejectStatus: (reason: Error) => void = () => undefined;
  backend({
    library_status: () => new Promise((resolve, reject) => {
      resolveStatus = resolve;
      rejectStatus = reject;
    }),
    search: () => failed ? [] : [result("new", "New library reference")],
  });
  render(<App />);
  await act(async () => { await Promise.resolve(); });
  await act(async () => {
    await emit("library:changed", {
      name: "new.bib", snapshot_name: "new.bib", references: failed ? 0 : 1, warnings: 0,
      error: failed ? "synthetic load failed" : null, note: null,
    });
  });
  if (failed) await screen.findByText(/synthetic load failed/);
  else await screen.findByText("New library reference");
  await act(async () => {
    if (outcome === "resolves") {
      resolveStatus({ state: "none", has_snapshot: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null });
    } else rejectStatus(new Error("stale status unavailable"));
  });
  expect(screen.getByRole("region", { name: "Library" }).textContent).toContain("new.bib");
  expect(screen.queryByRole("button", { name: "Choose library to get started" })).toBeNull();
  if (failed) expect(screen.getByText(/synthetic load failed/)).toBeTruthy();
  else expect(screen.getByText("New library reference")).toBeTruthy();
});

test("Escape hides the picker", async () => {
  const calls = backend();
  render(<App />);
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Escape" });
  await waitFor(() => expect(calls).toContain("hide_picker"));
});

test("a shortcut that could not be registered is explained", async () => {
  backend({
    shortcut_status: () => ({ shortcut: "Control+Command+R", registered: false, error: "another app may be using it" }),
  });
  render(<App />);
  expect((await screen.findByText(/another app may be using it/)).getAttribute("role")).toBe("status");
});

test("a failed search clears the results of the previous query", async () => {
  backend({
    search: ({ query }) => {
      if (query === "diet") throw new Error("SYNTHETIC_PRIVATE_PAYLOAD");
      return query ? [result("smith2020", "Sleep and memory")] : [];
    },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "sleep" } });
  expect(await screen.findByText("Sleep and memory")).toBeTruthy();
  fireEvent.change(box, { target: { value: "diet" } });
  expect((await screen.findByText("Search is unavailable; reload the library and try again.")).getAttribute("role")).toBe("alert");
  expect(screen.queryByText("Sleep and memory")).toBeNull();
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
});

test("Quit is available from the picker and from the keyboard", async () => {
  const calls = backend({ quit: () => null });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "Quit Ibis" }));
  await waitFor(() => expect(calls.filter((c) => c === "quit")).toHaveLength(1));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "q", ctrlKey: true });
  await waitFor(() => expect(calls.filter((c) => c === "quit")).toHaveLength(2));
});

test("a rejected shortcut setting is explained even when the default works", async () => {
  backend({
    shortcut_status: () => ({
      shortcut: "Control+Command+R",
      registered: true,
      error: 'the shortcut "Control+Nonsense" in settings is not valid; using Control+Command+R',
    }),
  });
  render(<App />);
  expect((await screen.findByText(/Control\+Nonsense/)).getAttribute("role")).toBe("status");
});

test("the query is searched again when a library finishes opening", async () => {
  let open = false;
  backend({
    search: ({ query }) => {
      if (!open) throw new Error("no library is open");
      return query ? [result("smith2020", "Sleep and memory")] : [];
    },
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  expect((await screen.findByText("Search is unavailable; reload the library and try again.")).getAttribute("role")).toBe("alert");
  open = true;
  await emit("library:changed", { state: "loaded", has_snapshot: true, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null });
  expect(await screen.findByText("Sleep and memory")).toBeTruthy();
  expect(screen.queryByRole("alert")).toBeNull();
});

test("Enter copies the highlighted result in the remembered format and hides the picker", async () => {
  const copies: unknown[] = [];
  const calls = backend({
    copy_format: () => "typst",
    search: ({ query }) => (query ? [result("a", "First"), result("b", "Second")] : []),
    copy_citation: (args) => {
      copies.push(args);
      return { text: "@b", warnings: [] };
    },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  await waitFor(() => expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe("typst"));
  fireEvent.change(box, { target: { value: "s" } });
  await screen.findByText("Second");
  fireEvent.keyDown(box, { key: "ArrowDown" });
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(calls).toContain("hide_picker"));
  expect(copies).toEqual([{ ids: [{ source: "refs.bib", native_key: "b" }], format: "typst" }]);
  expect(screen.getAllByRole("status").some((status) => status.textContent?.includes("Copied @b"))).toBe(true);
});

test("the selection is kept by record identity across queries and copied in order", async () => {
  const copies: unknown[] = [];
  backend({
    search: ({ query }) =>
      query === "one" ? [result("same", "One", "a.bib")] : query === "two" ? [result("same", "Two", "b.bib")] : [],
    copy_citation: (args) => {
      copies.push(args);
      return { text: "[@same; @same]", warnings: [] };
    },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "one" } });
  await screen.findByText("One");
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  fireEvent.change(box, { target: { value: "two" } });
  fireEvent.click(await screen.findByText("Two"));
  expect(screen.getAllByRole("button", { name: /Remove same/ })).toHaveLength(2);
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(copies[0]).toEqual({
    ids: [
      { source: "a.bib", native_key: "same" },
      { source: "b.bib", native_key: "same" },
    ],
    format: "pandoc",
  });
});

test("warnings keep the picker open and failures say nothing was copied", async () => {
  let fail = false;
  const calls = backend({
    search: ({ query }) => (query ? [result("a", "First")] : []),
    copy_citation: () => {
      if (fail) throw new Error("SYNTHETIC_PRIVATE_PAYLOAD");
      return { text: "a", warnings: ["bare keys cannot carry page numbers"] };
    },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "f" } });
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Enter" });
  expect((await screen.findByLabelText("Copy warnings")).textContent).toContain("page numbers");
  expect(calls).not.toContain("hide_picker");
  fail = true;
  fireEvent.keyDown(box, { key: "Enter" });
  expect((await screen.findByRole("alert")).textContent).toContain("Nothing was copied");
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
});

test("Enter while composing text with an input method copies nothing", async () => {
  const calls = backend({ search: ({ query }) => (query ? [result("a", "First")] : []) });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "f" } });
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Enter", isComposing: true });
  fireEvent.keyDown(box, { key: "Enter", keyCode: 229 });
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(calls).not.toContain("copy_citation");
});

test("Enter never copies a result left over from the previous query", async () => {
  let release: (value: unknown) => void = () => undefined;
  const calls = backend({
    search: ({ query }) =>
      query === "old" ? [result("old", "Old result")] : query ? new Promise((resolve) => (release = resolve)) : [],
    copy_citation: () => ({ text: "x", warnings: [] }),
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "old" } });
  await screen.findByText("Old result");
  fireEvent.change(box, { target: { value: "new" } });
  fireEvent.keyDown(box, { key: "Enter" });
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(calls).not.toContain("copy_citation");
  release([result("new", "New result")]);
  await screen.findByText("New result");
});

test("the library is shown, can be chosen and reloaded, and failures are explained", async () => {
  const calls = backend({
    library_status: () => ({ state: "none", has_snapshot: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null }),
    choose_library: () => null,
    reload_library: () => null,
  });
  render(<App />);
  const bar = screen.getByRole("region", { name: "Library" });
  await waitFor(() => expect(bar.textContent).toContain("No library chosen"));
  fireEvent.click(screen.getByRole("button", { name: "Choose library…" }));
  await waitFor(() => expect(calls).toContain("choose_library"));
  await emit("library:changed", { state: "loaded", has_snapshot: true, name: "refs.bib", snapshot_name: "refs.bib", references: 3, warnings: 1, error: null, note: null });
  await waitFor(() => expect(bar.textContent).toContain("refs.bib: 3 references, 1 could not be read fully"));
  fireEvent.click(screen.getByRole("button", { name: "Reload" }));
  await waitFor(() => expect(calls).toContain("reload_library"));
  await emit("library:changed", {
    state: "error",
    has_snapshot: true,
    name: "refs.bib",
    snapshot_name: "refs.bib",
    references: 3,
    warnings: 1,
    error: "cannot read the library file",
    note: null,
  });
  expect((await screen.findByText(/The library could not be opened/)).textContent).toContain("cannot read");
});

test("opening the first library searches the query typed before choosing it", async () => {
  const queries: unknown[] = [];
  backend({
    library_status: () => ({ state: "none", has_snapshot: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null }),
    search: ({ query }) => { queries.push(query); return [result("first", "Matching reference")]; },
  });
  render(<App />);
  await screen.findByRole("button", { name: "Choose library to get started" });
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  await act(async () => { await emit("library:changed", { state: "loaded", has_snapshot: true, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null }); });
  await screen.findByText("Matching reference");
  expect(queries).toEqual(["sleep"]);
});

test("counts use singular and plural and announce only settled current searches", async () => {
  let release: (value: unknown) => void = () => undefined;
  backend({ search: ({ query }) => query === "one" ? [result("a", "Single")]
    : query === "pending" ? new Promise((resolve) => { release = resolve; })
    : query === "two" ? [result("a", "First"), result("b", "Second")] : [] });
  vi.useFakeTimers();
  render(<App />);
  await act(async () => { await Promise.resolve(); });
  const box = screen.getByLabelText("Search references");
  await act(async () => { fireEvent.change(box, { target: { value: "one" } }); });
  expect(screen.getByText("1 result")).toBeTruthy();
  const announcement = screen.getByRole("status", { name: "Search results" });
  expect(announcement.getAttribute("aria-live")).toBe("polite");
  expect(announcement.textContent).toBe("");
  await act(() => vi.advanceTimersByTimeAsync(200));
  await act(async () => { fireEvent.change(box, { target: { value: "pending" } }); });
  await act(() => vi.advanceTimersByTimeAsync(500));
  expect(announcement.textContent).toBe("");
  await act(async () => { fireEvent.change(box, { target: { value: "two" } }); });
  await act(async () => { release([result("stale", "Stale")]); });
  expect(screen.getByText("2 results")).toBeTruthy();
  await act(() => vi.advanceTimersByTimeAsync(200));
  expect(announcement.textContent).toBe("");
  await act(() => vi.advanceTimersByTimeAsync(200));
  expect(announcement.textContent).toBe("2 results");
  await act(async () => { fireEvent.change(box, { target: { value: "none" } }); });
  await act(() => vi.advanceTimersByTimeAsync(400));
  expect(announcement.textContent).toBe("0 results");
});

test("the focus ring contrasts with both themes independent of the system accent", () => {
  const focusToken = styles.match(/:focus-visible\s*\{[^}]*outline:\s*[^;]*solid var\((--[\w-]+)/)?.[1];
  expect(focusToken).toBeTruthy();
  const theme = (token: string) => {
    const pair = tokens.match(new RegExp(`${token}:\\s*light-dark\\((#[0-9a-f]+),\\s*(#[0-9a-f]+)\\)`));
    expect(pair, `${token} needs defined theme colors`).not.toBeNull();
    return [pair?.[1], pair?.[2]].map((hex) => [1, 3, 5].map((i) => parseInt(hex?.slice(i, i + 2) ?? "", 16) / 255));
  };
  const luminance = (rgb: number[]) => rgb.map((c) => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4)
    .reduce((sum, c, i) => sum + c * [0.2126, 0.7152, 0.0722][i]!, 0);
  const surface = theme("--surface"), text = theme("--text"), focus = theme(focusToken!);
  for (const mode of [0, 1]) {
    for (const tint of [0, .04, .08]) {
      const background = surface[mode]!.map((c, i) => c * (1 - tint) + text[mode]![i]! * tint);
      const [lighter, darker] = [luminance(focus[mode]!), luminance(background)].sort((a, b) => b - a);
      expect((lighter! + .05) / (darker! + .05)).toBeGreaterThanOrEqual(3);
    }
  }
});

test("combobox focus is separate from citation selection and only additions/removals are announced", async () => {
  backend({ search: ({ query }) => query === "reorder"
    ? [result("same", "Second", "b.bib"), result("same", "First", "a.bib")]
    : [result("same", "First", "a.bib"), result("same", "Second", "b.bib")] });
  render(<App />);
  await screen.findByText("First");
  const box = screen.getByRole("combobox", { name: "Search references" });
  const list = screen.getByRole("listbox", { name: "Results" });
  expect(list.hasAttribute("aria-multiselectable")).toBe(false);
  expect(within(list).getAllByRole("option", { selected: true })).toHaveLength(1);
  const notice = screen.getByRole("status", { name: "Citation selection" });
  expect(notice.getAttribute("aria-live")).toBe("polite");
  expect(notice.textContent).toBe("");
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  expect(notice.textContent).toBe("1 reference selected for citation");
  const first = screen.getByText("First").closest('[role="option"]')!;
  expect(first.hasAttribute("data-citation-selected")).toBe(true);
  expect(first.textContent).toContain("Added to citation");
  fireEvent.keyDown(box, { key: "ArrowDown" });
  const second = screen.getByText("Second").closest('[role="option"]')!;
  expect(second.getAttribute("aria-selected")).toBe("true");
  expect(first.getAttribute("aria-selected")).toBe("false");
  expect(notice.textContent).toBe("1 reference selected for citation");
  expect(second.hasAttribute("data-citation-selected")).toBe(false);
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true, isComposing: true });
  expect(notice.textContent).toBe("1 reference selected for citation");
  fireEvent.change(box, { target: { value: "reorder" } });
  await waitFor(() => expect(within(list).getAllByRole("option")[0]?.textContent).toContain("Second"));
  expect(screen.getByText("First").closest('[role="option"]')?.hasAttribute("data-citation-selected")).toBe(true);
  expect(within(list).getAllByRole("option", { selected: true })).toHaveLength(1);
  expect(document.getElementById(box.getAttribute("aria-activedescendant")!)?.textContent).toContain("Second");
  fireEvent.click(screen.getByText("Second"));
  expect(notice.textContent).toBe("2 references selected for citation");
  fireEvent.click(screen.getAllByRole("button", { name: "Remove same from the selection" })[0]!);
  expect(notice.textContent).toBe("1 reference selected for citation");
  expect(screen.getByText("First").closest('[role="option"]')?.hasAttribute("data-citation-selected")).toBe(false);
  fireEvent.click(screen.getByText("Second"));
  expect(notice.textContent).toBe("0 references selected for citation");
});

test("failed status and saved format reads show safe notices without hiding search results", async () => {
  backend({
    library_status: () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); },
    copy_format: () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); },
    search: () => [result("a", "Available reference")],
  });
  render(<App />);
  await screen.findByText("Available reference");
  expect(screen.getByText("Library status unavailable. Choose a library to try again.").getAttribute("role")).toBe("alert");
  expect(screen.getByText("Saved copy format could not be read. Check Copy as before copying.").getAttribute("role")).toBe("alert");
  expect(screen.getByRole("region", { name: "Library" }).textContent).not.toContain("No library chosen");
  expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe("pandoc");
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
});

test.each([
  ["choose_library", "The library chooser is unavailable. Try again."],
  ["reload_library", "Reload is unavailable. Try choosing the library again."],
  ["hide_picker", "The picker could not be hidden. Try again."],
  ["quit", "Ibis could not quit. Try again."],
])("%s failures are reported without rejected IPC payloads", async (command, notice) => {
  backend({ [command]: () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); } });
  render(<App />);
  await screen.findByRole("button", { name: "Reload" });
  if (command === "choose_library") fireEvent.click(screen.getByRole("button", { name: "Choose library…" }));
  else if (command === "reload_library") fireEvent.click(screen.getByRole("button", { name: "Reload" }));
  else fireEvent.keyDown(screen.getByLabelText("Search references"), command === "quit"
    ? { key: "q", ctrlKey: true } : { key: "Escape" });
  expect((await screen.findByText(notice)).getAttribute("role")).toBe("alert");
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
});

test("copy remains confirmed when hiding after a successful copy fails", async () => {
  backend({ search: () => [result("a", "First")], copy_citation: () => ({ text: "@a", warnings: [] }),
    hide_picker: () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); } });
  render(<App />);
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByText("The picker could not be hidden. Try again.");
  expect(screen.getByText("@a")).toBeTruthy();
  expect(document.body.textContent).not.toContain("Nothing was copied");
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
});

test("event subscription and cleanup failures are handled with safe messages", async () => {
  const focus = vi.spyOn(api, "onPickerShown").mockRejectedValue(new Error("SYNTHETIC_PRIVATE_PAYLOAD"));
  const library = vi.spyOn(api, "onLibraryChanged").mockResolvedValue(async () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); });
  const log = vi.spyOn(console, "error").mockImplementation(() => undefined);
  try {
    backend();
    const view = render(<App />);
    await screen.findByText("Picker focus updates are unavailable. Restart Ibis to try again.");
    expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
    view.unmount();
    await waitFor(() => expect(log).toHaveBeenCalledWith("Library event cleanup failed."));
    expect(log.mock.calls.flat().join(" ")).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
  } finally { focus.mockRestore(); library.mockRestore(); log.mockRestore(); }
});

test("web permissions grant only event subscriptions and drag-region bootstrap", () => {
  expect([...capability.permissions].sort()).toEqual([
    "core:event:allow-listen", "core:event:allow-unlisten",
    "core:window:allow-internal-toggle-maximize", "core:window:allow-start-dragging",
  ]);
  expect(config.app.windows[0]?.maximizable).toBe(false);
});

test.each([false, true])("axe checks picker structure (library open: %s) and detects a broken combobox", async (open) => {
  backend({
    library_status: () => ({ state: open ? "loaded" : "none", has_snapshot: open, name: open ? "refs.bib" : null, snapshot_name: open ? "refs.bib" : null, references: open ? 1 : 0, warnings: 0, error: null, note: null }),
    search: () => [result("synthetic", "Synthetic reference")],
  });
  render(<App />);
  if (open) await screen.findByText("Synthetic reference");
  else await screen.findByRole("button", { name: "Choose library to get started" });
  const options: axe.RunOptions = {
    runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] },
    // JSDOM has no layout/canvas: theme contrast has its own numeric test above.
    rules: { "color-contrast": { enabled: false } },
  };
  // Report the actual lack of canvas support; do not emulate rendered pixels.
  const canvas = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  try {
    expect((await axe.run(document.body, options)).violations.map((v) => v.id)).toEqual([]);
    screen.getByRole("combobox", { name: "Search references" }).removeAttribute("aria-expanded");
    const broken = await axe.run(document.body, options);
    expect(broken.violations.map((v) => v.id)).toContain("aria-required-attr");
  } finally {
    canvas.mockRestore();
  }
});


test("a pending library check does not offer first-run setup", async () => {
  backend({ library_status: () => new Promise(() => undefined) });
  render(<App />);
  await screen.findByText("Checking library…");
  expect(screen.queryByText("Choose a library to get started")).toBeNull();
});

test("opening a chosen library shows its name and searches only when ready", async () => {
  const calls = backend({
    library_status: () => ({ state: "loading", has_snapshot: false, name: "研究 refs.bib", snapshot_name: "研究 refs.bib", references: 0, warnings: 0, error: null, note: null }),
    choose_library: () => null,
  });
  render(<App />);
  await screen.findByText("Opening 研究 refs.bib…");
  expect(screen.queryByText("Choose a library to get started")).toBeNull();
  expect(document.body.textContent).not.toContain("No library chosen");
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  expect(calls).not.toContain("search");
  fireEvent.click(screen.getByRole("button", { name: "Choose library…" }));
  await waitFor(() => expect(calls).toContain("choose_library"));
  await act(async () => { await emit("library:changed", { state: "loaded", has_snapshot: true, name: "研究 refs.bib", snapshot_name: "研究 refs.bib", references: 1, warnings: 0, error: null, note: null }); });
  await waitFor(() => expect(calls).toContain("search"));
});

test("a reload keeps the previous library searchable and announces the reload", async () => {
  backend({
    library_status: () => ({ state: "reloading", has_snapshot: true, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null }),
    search: () => [result("kept", "Kept reference")],
  });
  render(<App />);
  await screen.findByText("Reloading refs.bib…");
  await screen.findByText("Kept reference");
});


test("a failed switch names the retained library in its accessible status", async () => {
  backend({
    library_status: () => ({ state: "error", has_snapshot: true, name: "missing B.bib", snapshot_name: "active A.bib", references: 3, warnings: 0, error: "The library file is unavailable; keeping the last loaded references", note: null }),
    search: () => [result("kept", "Reference from A")],
  });
  render(<App />);
  await screen.findByText("Reference from A");
  const status = screen.getByRole("status", { name: "Library status" });
  expect(status.textContent).toBe("active A.bib: 3 references");
  expect(status.textContent).not.toContain("missing B.bib");
});

test("an unavailable initial library is named once without a duplicate search error", async () => {
  const calls = backend({
    library_status: () => ({ state: "error", has_snapshot: false, name: "研究 refs.bib", snapshot_name: "研究 refs.bib", references: 0, warnings: 0, error: "The library file is unavailable.", note: null }),
    search: () => { throw new Error("no library is open"); },
    choose_library: () => null,
  });
  render(<App />);
  const notice = await screen.findByRole("alert");
  expect(notice.textContent).toContain("研究 refs.bib");
  expect(notice.textContent).toContain("Choose it again");
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  expect(screen.getAllByRole("alert")).toHaveLength(1);
  expect(document.body.textContent).not.toContain("No library chosen");
  expect(document.body.textContent).not.toContain("keeping the last");
  expect(document.body.textContent).not.toContain("No matching references");
  expect(calls).not.toContain("search");
  fireEvent.click(screen.getByRole("button", { name: "Choose library…" }));
  await waitFor(() => expect(calls).toContain("choose_library"));
});

test("a failed reload reports one file error while kept references remain searchable", async () => {
  backend({
    library_status: () => ({ state: "error", has_snapshot: true, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: "The library file is unavailable; keeping the last loaded references.", note: null }),
    search: () => [result("kept", "Previously loaded reference")],
  });
  render(<App />);
  await screen.findByText("Previously loaded reference");
  expect(screen.getAllByRole("alert")).toHaveLength(1);
  expect(screen.getByRole("alert").textContent).toContain("refs.bib");
  expect(screen.getByRole("alert").textContent).toContain("keeping the last loaded references");
});

test.each(["backup", "defaults"])("settings recovery from %s stays visible through a library reload", async (fallback) => {
  const notice = `settings.json could not be read. Using ${fallback}. Changes are not being saved. Repair /synthetic/settings.json and restart Ibis.`;
  backend({ library_status: () => ({ state: "loading", has_snapshot: false, name: "refs.bib", snapshot_name: "refs.bib", references: 0, warnings: 0, error: null, note: null, settings_notice: notice }) });
  render(<App />);
  expect((await screen.findByText(notice)).getAttribute("role")).toBe("alert");
  await act(async () => { await emit("library:changed", { state: "loaded", has_snapshot: true, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null, settings_notice: notice }); });
  expect(screen.getByText(notice).getAttribute("role")).toBe("alert");
  expect(screen.getAllByText(notice)).toHaveLength(1);
});

test.each(["", "sleep"])("an empty loaded library is explained even for query %s", async (query) => {
  backend({ library_status: () => ({ state: "loaded", has_snapshot: true, name: "empty 研究.bib", snapshot_name: "empty 研究.bib", references: 0, warnings: 0, error: null, note: null }) });
  render(<App />);
  await screen.findByText("empty 研究.bib: 0 references");
  if (query) fireEvent.change(screen.getByLabelText("Search references"), { target: { value: query } });
  await screen.findByText("No references found in empty 研究.bib. Choose a BibTeX or BibLaTeX export with references, then try again.");
  expect(screen.queryByText("Type to find a reference")).toBeNull();
  expect(screen.queryByText("No matching references")).toBeNull();
  expect(screen.getByRole("button", { name: "Choose library…" })).toBeTruthy();
});


test.each([false, true])("a failed library open is not described as an empty export (retained: %s)", async (retained) => {
  backend({ library_status: () => ({ state: "error", has_snapshot: retained, name: "missing B.bib", snapshot_name: retained ? "empty A.bib" : null, references: 0, warnings: 0, error: "The library file is unavailable", note: null }) });
  render(<App />);
  await screen.findByRole("alert");
  expect(document.body.textContent).not.toContain("No references found");
  expect(document.body.textContent).not.toContain("Choose a BibTeX or BibLaTeX export");
});
