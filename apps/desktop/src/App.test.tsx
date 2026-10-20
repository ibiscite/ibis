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

/** Opens the More menu and picks an action from it, as a user would. */
async function chooseFromMore(name: string, role: "menuitem" | "menuitemcheckbox" = "menuitem") {
  if (!screen.queryByRole("menu")) fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  fireEvent.click(await screen.findByRole(role, { name }));
}

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
    set_pinned: () => null,
    copy_format: () => "pandoc",
    picker_preferences: () => ({ citation_style: null, density: "comfortable", larger_text: false }),
    set_picker_preferences: () => null,
    find_sources: () => ({ zotero: "absent", list: 1, files: [], cut_short: false }),
    library_status: () => ({ state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null }),
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
  expect((await screen.findByText("Ibis could not search your library. Use Reload, then try again.")).getAttribute("role")).toBe("alert");
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
  await act(async () => { status({ state: "none", has_snapshot: false, zotero: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null }); });
  fireEvent.click(await screen.findByRole("button", { name: "Choose library to get started" }));
  await waitFor(() => expect(calls).toContain("choose_library"));
  expect(screen.queryByRole("alert")).toBeNull();
  expect(calls).not.toContain("search");
});

test("typing after a library load failure keeps its single explanation visible", async () => {
  backend({
    library_status: () => ({ state: "error", has_snapshot: false, zotero: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: "the synthetic library is unreadable", note: null }),
    search: () => { throw new Error("no library is open"); },
  });
  render(<App />);
  expect((await screen.findByText(/The library could not be opened/)).textContent).toContain("synthetic library is unreadable");
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  expect(screen.queryByText("Ibis could not search your library. Use Reload, then try again.")).toBeNull();
  expect(screen.getAllByRole("alert")).toHaveLength(1);
});

test("the empty-state chooser opens the existing dialog and remains available after cancellation", async () => {
  const calls = backend({
    library_status: () => ({ state: "none", has_snapshot: false, zotero: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null }),
    choose_library: () => null,
  });
  render(<App />);
  const chooser = await screen.findByRole("button", { name: "Choose library to get started" });
  fireEvent.click(chooser);
  await waitFor(() => expect(calls).toContain("choose_library"));
  expect(screen.getByRole("button", { name: "Choose library to get started" })).toBe(chooser);
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  expect(screen.getByRole("menuitem", { name: "Choose library…" })).toBeTruthy();
});

test("typing and clearing before choosing a library keeps the calm hint and chooser", async () => {
  const calls = backend({
    library_status: () => ({ state: "none", has_snapshot: false, zotero: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null }),
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
    library_status: () => ({ state: "none", has_snapshot: false, zotero: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null }),
    search: () => [result("first", "First reference")],
  });
  render(<App />);
  await screen.findByRole("button", { name: "Choose library to get started" });
  expect(calls).not.toContain("search");
  await emit("library:changed", { state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null });
  expect(await screen.findByText("First reference")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Choose library to get started" })).toBeNull();
});

test("a failed status request does not suppress search errors", async () => {
  backend({
    library_status: () => { throw new Error("status unavailable"); },
    search: () => { throw new Error("search unavailable"); },
  });
  render(<App />);
  expect((await screen.findByText("Ibis could not search your library. Use Reload, then try again.")).getAttribute("role")).toBe("alert");
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
      resolveStatus({ state: "none", has_snapshot: false, zotero: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null });
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
  expect((await screen.findByText("Ibis could not search your library. Use Reload, then try again.")).getAttribute("role")).toBe("alert");
  expect(screen.queryByText("Sleep and memory")).toBeNull();
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
});

test("Quit is available from the picker and from the keyboard", async () => {
  const calls = backend({ quit: () => null });
  render(<App />);
  await chooseFromMore("Quit Ibis");
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
  expect((await screen.findByText("Ibis could not search your library. Use Reload, then try again.")).getAttribute("role")).toBe("alert");
  open = true;
  await emit("library:changed", { state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null });
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
    library_status: () => ({ state: "none", has_snapshot: false, zotero: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null }),
    choose_library: () => null,
    reload_library: () => null,
  });
  render(<App />);
  const bar = screen.getByRole("region", { name: "Library" });
  await waitFor(() => expect(bar.textContent).toContain("No library chosen"));
  await chooseFromMore("Choose library…");
  await waitFor(() => expect(calls).toContain("choose_library"));
  await emit("library:changed", { state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 3, warnings: 1, error: null, note: null });
  await waitFor(() => expect(bar.textContent).toContain("refs.bib: 3 references, 1 reading notice"));
  await chooseFromMore("Reload");
  await waitFor(() => expect(calls).toContain("reload_library"));
  await emit("library:changed", {
    state: "error",
    has_snapshot: true, zotero: false,
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
    library_status: () => ({ state: "none", has_snapshot: false, zotero: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null }),
    search: ({ query }) => { queries.push(query); return [result("first", "Matching reference")]; },
  });
  render(<App />);
  await screen.findByRole("button", { name: "Choose library to get started" });
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  await act(async () => { await emit("library:changed", { state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null }); });
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
  expect(screen.getByText("Ibis could not check your library. Choose the file again.").getAttribute("role")).toBe("alert");
  expect(screen.getByText("Ibis could not read your saved copy format. Check Copy as before copying.").getAttribute("role")).toBe("alert");
  expect(screen.getByRole("region", { name: "Library" }).textContent).not.toContain("No library chosen");
  expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe("pandoc");
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
});

test.each([
  ["choose_library", "The file chooser did not open. Try again."],
  ["reload_library", "Ibis could not reload your library. Choose the file again."],
  ["hide_picker", "Ibis could not hide this window. Try again."],
  ["quit", "Ibis could not quit. Try again."],
])("%s failures are reported without rejected IPC payloads", async (command, notice) => {
  backend({ [command]: () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); } });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("region", { name: "Library" }).textContent).toMatch(/reference/));
  if (command === "choose_library") await chooseFromMore("Choose library…");
  else if (command === "reload_library") await chooseFromMore("Reload");
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
  await screen.findByText("Ibis could not hide this window. Try again.");
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
    await screen.findByText("Ibis could not focus the search box. Restart Ibis to try again.");
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
    library_status: () => ({ state: open ? "loaded" : "none", has_snapshot: open, zotero: false, name: open ? "refs.bib" : null, snapshot_name: open ? "refs.bib" : null, references: open ? 1 : 0, warnings: 0, error: null, note: null }),
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
    library_status: () => ({ state: "loading", has_snapshot: false, zotero: false, name: "研究 refs.bib", snapshot_name: "研究 refs.bib", references: 0, warnings: 0, error: null, note: null }),
    choose_library: () => null,
  });
  render(<App />);
  await screen.findByText("Opening 研究 refs.bib…");
  expect(screen.queryByText("Choose a library to get started")).toBeNull();
  expect(document.body.textContent).not.toContain("No library chosen");
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  expect(calls).not.toContain("search");
  await chooseFromMore("Choose library…");
  await waitFor(() => expect(calls).toContain("choose_library"));
  await act(async () => { await emit("library:changed", { state: "loaded", has_snapshot: true, zotero: false, name: "研究 refs.bib", snapshot_name: "研究 refs.bib", references: 1, warnings: 0, error: null, note: null }); });
  await waitFor(() => expect(calls).toContain("search"));
});

test("a reload keeps the previous library searchable and announces the reload", async () => {
  backend({
    library_status: () => ({ state: "reloading", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null }),
    search: () => [result("kept", "Kept reference")],
  });
  render(<App />);
  await screen.findByText("Reloading refs.bib…");
  await screen.findByText("Kept reference");
});


test("a failed switch names the retained library in its accessible status", async () => {
  backend({
    library_status: () => ({ state: "error", has_snapshot: true, zotero: false, name: "missing B.bib", snapshot_name: "active A.bib", references: 3, warnings: 0, error: "The library file is unavailable; keeping the last loaded references", note: null }),
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
    library_status: () => ({ state: "error", has_snapshot: false, zotero: false, name: "研究 refs.bib", snapshot_name: "研究 refs.bib", references: 0, warnings: 0, error: "The library file is unavailable.", note: null }),
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
  await chooseFromMore("Choose library…");
  await waitFor(() => expect(calls).toContain("choose_library"));
});

test("a failed reload reports one file error while kept references remain searchable", async () => {
  backend({
    library_status: () => ({ state: "error", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: "The library file is unavailable; keeping the last loaded references.", note: null }),
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
  backend({ library_status: () => ({ state: "loading", has_snapshot: false, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 0, warnings: 0, error: null, note: null, settings_notice: notice }) });
  render(<App />);
  expect((await screen.findByText(notice)).getAttribute("role")).toBe("alert");
  await act(async () => { await emit("library:changed", { state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null, settings_notice: notice }); });
  expect(screen.getByText(notice).getAttribute("role")).toBe("alert");
  expect(screen.getAllByText(notice)).toHaveLength(1);
});

test.each(["", "sleep"])("an empty loaded library is explained even for query %s", async (query) => {
  backend({ library_status: () => ({ state: "loaded", has_snapshot: true, zotero: false, name: "empty 研究.bib", snapshot_name: "empty 研究.bib", references: 0, warnings: 0, error: null, note: null }) });
  render(<App />);
  await screen.findByText("empty 研究.bib: 0 references");
  if (query) fireEvent.change(screen.getByLabelText("Search references"), { target: { value: query } });
  await screen.findByText("No references found in empty 研究.bib. Choose a BibTeX or BibLaTeX export with references, then try again.");
  expect(screen.queryByText("Type to find a reference")).toBeNull();
  expect(screen.queryByText("No matching references")).toBeNull();
  expect(screen.getByRole("button", { name: "Choose library…" })).toBeTruthy();
});


test.each([false, true])("a failed library open is not described as an empty export (retained: %s)", async (retained) => {
  backend({ library_status: () => ({ state: "error", has_snapshot: retained, zotero: false, name: "missing B.bib", snapshot_name: retained ? "empty A.bib" : null, references: 0, warnings: 0, error: "The library file is unavailable", note: null }) });
  render(<App />);
  await screen.findByRole("alert");
  expect(document.body.textContent).not.toContain("No references found");
  expect(document.body.textContent).not.toContain("Choose a BibTeX or BibLaTeX export");
});

test("a citation style shows a safe preview and copies rich text with its warnings", async () => {
  const { InlineWorker } = await import("./format/inline-worker");
  vi.stubGlobal("Worker", InlineWorker);
  const copies: Record<string, unknown>[] = [];
  backend({
    search: () => [result("a", "First"), result("b", "Second")],
    csl_items: (args) => JSON.stringify((args.ids as { native_key: string }[]).map((id) => id.native_key === "a"
      ? { id: "a", type: "book", title: "First <img src=x onerror=alert(1)>", author: [{ family: "Garcia", given: "Ana" }], issued: { "date-parts": [[2021]] } }
      : { id: "b", type: "book", title: "Second", author: [{ family: 5 }] })),
    copy_formatted: (args) => { copies.push(args); return null; },
  });
  try {
    render(<App />);
    await screen.findByText("First");
    fireEvent.change(screen.getByLabelText("Copy as"), { target: { value: "style:ieee" } });
    const preview = await screen.findByRole("region", { name: "Preview" });
    await within(preview).findByText("[1]");
    expect(within(preview).getByText(/Provisional/)).toBeTruthy();
    const box = screen.getByLabelText("Search references");
    fireEvent.keyDown(box, { key: "ArrowDown" });
    fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
    fireEvent.keyDown(box, { key: "ArrowUp" });
    fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
    fireEvent.change(screen.getByLabelText("Copy as"), { target: { value: "style:apa" } });
    await within(preview).findByText("(Garcia, 2021); Second");
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(copies).toHaveLength(1));
    expect(copies[0]).toEqual({ text: "(Garcia, 2021); Second", html: "(Garcia, 2021); Second" });
    await screen.findByText("Copied");
    expect(screen.getAllByText(/“Second”: this style could not format the reference/).length).toBeGreaterThan(0);
  } finally {
    vi.unstubAllGlobals();
  }
});

test("Enter never copies a formatted preview of an earlier choice", async () => {
  const { InlineWorker } = await import("./format/inline-worker");
  vi.stubGlobal("Worker", InlineWorker);
  const copies: Record<string, unknown>[] = [];
  const book = (key: string, family: string) => ({ id: key, type: "book", title: key, author: [{ family }], issued: { "date-parts": [[2021]] } });
  backend({
    search: () => [result("a", "First"), result("b", "Second")],
    csl_items: (args) => JSON.stringify((args.ids as { native_key: string }[]).map((id) =>
      id.native_key === "a" ? book("a", "Alpha") : book("b", "Beta"))),
    copy_formatted: (args) => { copies.push(args); return null; },
  });
  try {
    render(<App />);
    await screen.findByText("First");
    fireEvent.change(screen.getByLabelText("Copy as"), { target: { value: "style:apa" } });
    const preview = await screen.findByRole("region", { name: "Preview" });
    await within(preview).findByText("(Alpha, 2021)");
    const box = screen.getByLabelText("Search references");
    // Enter after the choice changed but before its preview is ready.
    act(() => { fireEvent.keyDown(box, { key: "ArrowDown" }); });
    act(() => { fireEvent.keyDown(box, { key: "Enter" }); });
    expect(await screen.findByText(/still being formatted/)).toBeTruthy();
    await within(preview).findByText("(Beta, 2021)");
    expect(copies).toEqual([]);
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(copies.map((copy) => copy.text)).toEqual(["(Beta, 2021)"]));
  } finally {
    vi.unstubAllGlobals();
  }
});

test("the design scale keeps text readable and every colour role legible in both themes", () => {
  for (const [, token, size] of tokens.matchAll(/(--type-[\w-]+-size):\s*([\d.]+)rem/g)) {
    expect(Number(size), `${token} is at least 12 px`).toBeGreaterThanOrEqual(.75);
  }
  const hex = (value: string) => [1, 3, 5].map((i) => parseInt(value.slice(i, i + 2), 16) / 255);
  const luminance = (rgb: number[]) => rgb.map((c) => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4)
    .reduce((sum, c, i) => sum + c * [0.2126, 0.7152, 0.0722][i]!, 0);
  const pair = (token: string) => {
    const found = tokens.match(new RegExp(`${token}:\\s*light-dark\\((#[0-9a-f]{6}),\\s*(#[0-9a-f]{6})\\)`));
    expect(found, `${token} needs defined theme colors`).not.toBeNull();
    return [found![1]!, found![2]!];
  };
  const surface = pair("--surface");
  for (const role of ["--muted", "--citation", "--status-done", "--status-look", "--status-stop"]) {
    pair(role).forEach((colour, mode) => {
      const [lighter, darker] = [luminance(hex(colour)), luminance(hex(surface[mode]!))].sort((a, b) => b - a);
      expect((lighter! + .05) / (darker! + .05), `${role} in ${mode ? "dark" : "light"} mode`).toBeGreaterThanOrEqual(4.5);
    });
  }
});

test("Ctrl+K opens More with every action, Escape closes it before hiding the picker", async () => {
  const calls = backend({ hide_picker: () => null, reload_library: () => null });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  await waitFor(() => expect(screen.getByRole("region", { name: "Library" }).textContent).toMatch(/reference/));
  fireEvent.keyDown(box, { key: "k", ctrlKey: true });
  const menu = await screen.findByRole("menu");
  const items = within(menu).getAllByRole("menuitem").map((item) => item.textContent);
  expect(items).toEqual(["Choose library…", "Use Zotero", "Reload", "Rows: Comfortable", "Quit IbisCtrl+Q"]);
  expect(within(menu).getByRole("menuitemcheckbox", { name: "Larger text" }).getAttribute("aria-checked")).toBe("false");
  expect(document.activeElement).toBe(within(menu).getAllByRole("menuitem")[0]);
  fireEvent.keyDown(document.activeElement!, { key: "ArrowUp" });
  expect(document.activeElement?.textContent).toContain("Quit Ibis");
  const canvas = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  try {
    const found = await axe.run(document.body, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa"] }, rules: { "color-contrast": { enabled: false } } });
    expect(found.violations.map((v) => v.id)).toEqual([]);
  } finally {
    canvas.mockRestore();
  }
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  expect(screen.queryByRole("menu")).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole("button", { name: /^More/ }));
  expect(calls).not.toContain("hide_picker");
  fireEvent.keyDown(box, { key: "Escape" });
  await waitFor(() => expect(calls).toContain("hide_picker"));
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  fireEvent.pointerDown(document.body);
  expect(screen.queryByRole("menu")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  fireEvent.keyDown(within(screen.getByRole("menu")).getAllByRole("menuitem")[0]!, { key: "Tab" });
  expect(screen.queryByRole("menu")).toBeNull();
  expect(document.activeElement).toBe(box);
  expect(screen.getByRole("button", { name: /^More/ }).textContent).toContain("K");
  expect(screen.getByRole("button", { name: /^More/ }).getAttribute("aria-keyshortcuts")).toBe("Control+K");
});

test("a row says why it matched only when the match is not visible", async () => {
  backend({ search: () => [
    { ...result("a", "Visible match"), matched: ["title", "abstract"] },
    { ...result("b", "Hidden match"), matched: ["abstract", "keyword"] },
  ] });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "spindle" } });
  await screen.findByText("Hidden match");
  const rows = within(screen.getByRole("listbox", { name: "Results" })).getAllByRole("option");
  expect(rows[0]?.textContent).not.toContain("Matched in");
  expect(rows[1]?.textContent).toContain("Matched in the abstract");
  expect(screen.getByText("Best matches")).toBeTruthy();
});

test("Escape clears the search before it hides the picker, keeping chosen references", async () => {
  const calls = backend({ search: () => [result("a", "First")] });
  render(<App />);
  const box = screen.getByLabelText("Search references") as HTMLInputElement;
  fireEvent.change(box, { target: { value: "first" } });
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  fireEvent.keyDown(box, { key: "Escape" });
  expect(box.value).toBe("");
  expect(within(screen.getByRole("list", { name: "Selected references" })).getByText("a")).toBeTruthy();
  expect(calls).not.toContain("hide_picker");
  fireEvent.keyDown(box, { key: "Escape" });
  await waitFor(() => expect(calls).toContain("hide_picker"));
  expect(screen.getByRole("list", { name: "Selected references" })).toBeTruthy();
});

test("a pinned picker stays open after copying, and Escape never unpins it", async () => {
  const pins: unknown[] = [];
  const calls = backend({
    search: () => [result("a", "First")],
    copy_citation: () => ({ text: "@a", warnings: [] }),
    set_pinned: ({ on }) => { pins.push(on); return null; },
  });
  render(<App />);
  const pin = screen.getByRole("button", { name: "Pin Ibis" });
  expect(pin.getAttribute("aria-pressed")).toBe("false");
  fireEvent.click(pin);
  expect(pin.getAttribute("aria-pressed")).toBe("true");
  await waitFor(() => expect(pins).toEqual([true]));
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "first" } });
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Enter" });
  await screen.findByText("@a");
  fireEvent.keyDown(box, { key: "Escape" });
  expect(pin.getAttribute("aria-pressed")).toBe("true");
  expect(calls).not.toContain("hide_picker");
  fireEvent.click(pin);
  await waitFor(() => expect(pins).toEqual([true, false]));
  fireEvent.change(box, { target: { value: "first" } });
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(calls).toContain("hide_picker"));
});

test("a failed pin is reported", async () => {
  backend({ set_pinned: () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); } });
  render(<App />);
  const pin = screen.getByRole("button", { name: "Pin Ibis" });
  fireEvent.click(pin);
  expect((await screen.findByText("Ibis could not change the pin. Try again.")).getAttribute("role")).toBe("alert");
  expect(pin.getAttribute("aria-pressed")).toBe("false");
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
});

test.each([[30_000, true, "first"], [61_000, true, ""], [61_000, false, "first"]])(
  "a search is kept unless the picker was hidden for over a minute (%i ms, hidden: %s)", async (away, hidden, kept) => {
    backend({ search: () => [result("a", "First")] });
    const now = vi.spyOn(performance, "now").mockReturnValue(1_000_000);
    try {
      render(<App />);
      const box = screen.getByLabelText("Search references") as HTMLInputElement;
      fireEvent.change(box, { target: { value: "first" } });
      await screen.findByText("First");
      // A pinned picker loses focus without hiding; only a real hide starts the minute.
      fireEvent(window, new Event("blur"));
      if (hidden) await act(async () => { await emit("picker:hidden"); });
      now.mockReturnValue(1_000_000 + away);
      await act(async () => { await emit("picker:shown"); });
      expect(box.value).toBe(kept);
      expect(document.activeElement).toBe(box);
    } finally {
      now.mockRestore();
    }
  });

test("a saved citation style, density and text size come back at start", async () => {
  backend({
    copy_format: () => "typst",
    picker_preferences: () => ({ citation_style: "apa", density: "compact", larger_text: true }),
  });
  render(<App />);
  await waitFor(() => expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe("style:apa"));
  expect(document.documentElement.dataset.density).toBe("compact");
  expect(document.documentElement.dataset.textSize).toBe("larger");
});

test.each(["pandoc", "autocite", "parencite", "textcite", "cite", "typst", "keys"])("choosing %s survives a restart without copying", async (choice) => {
  let remembered = "typst";
  const calls = backend({
    copy_format: () => remembered,
    set_picker_preferences: ({ copyFormat }) => { if (copyFormat) remembered = copyFormat as string; return null; },
  });
  const first = render(<App />);
  const select = screen.getByLabelText("Copy as") as HTMLSelectElement;
  await waitFor(() => expect(select.value).toBe("typst"));
  fireEvent.change(select, { target: { value: choice } });
  await waitFor(() => expect(remembered).toBe(choice));
  expect(calls).not.toContain("copy_citation");
  first.unmount();
  render(<App />);
  await waitFor(() => expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe(choice));
});

test("rapid A-B-A choices keep the last failed choice for retry without replaying a saved key", async () => {
  let finish = () => undefined as void;
  const firstSave = new Promise<void>((resolve) => { finish = resolve; });
  const keys: unknown[] = [];
  let remembered = "typst";
  backend({
    copy_format: () => remembered,
    set_picker_preferences: async ({ copyFormat }) => {
      keys.push(copyFormat);
      const attempt = keys.length;
      if (attempt === 1) await firstSave;
      if (attempt === 3) throw new Error("SYNTHETIC_PRIVATE_SAVE_ERROR");
      if (copyFormat) remembered = copyFormat as string;
      return null;
    },
  });
  render(<App />);
  const select = screen.getByLabelText("Copy as") as HTMLSelectElement;
  await waitFor(() => expect(select.value).toBe("typst"));
  fireEvent.change(select, { target: { value: "cite" } });
  await waitFor(() => expect(keys).toEqual(["cite"]));
  fireEvent.change(select, { target: { value: "pandoc" } });
  fireEvent.change(select, { target: { value: "cite" } });
  expect(keys).toEqual(["cite"]);
  await act(async () => { finish(); await firstSave; });
  await screen.findByText(/could not read or save your picker choices/);
  expect(remembered).toBe("pandoc");
  expect(screen.queryByText(/SYNTHETIC_PRIVATE_SAVE_ERROR/)).toBeNull();
  await chooseFromMore("Rows: Comfortable");
  await waitFor(() => expect(screen.queryByText(/could not read or save your picker choices/)).toBeNull());
  expect(remembered).toBe("cite");
  expect(keys).toEqual(["cite", "pandoc", "cite", "cite"]);
  remembered = "keys"; // A hand edit after a successful save must survive.
  await chooseFromMore("Larger text", "menuitemcheckbox");
  await waitFor(() => expect(keys).toHaveLength(5));
  expect(keys[4]).toBeNull();
  expect(remembered).toBe("keys");
});

test.each(["menu", "shortcut"])("%s Quit drains ordered format choices before exiting", async (route) => {
  let finishFirst = () => undefined as void;
  let finishLast = () => undefined as void;
  const first = new Promise<void>((resolve) => { finishFirst = resolve; });
  const last = new Promise<void>((resolve) => { finishLast = resolve; });
  const keys: unknown[] = [];
  const calls = backend({
    copy_format: () => "typst",
    set_picker_preferences: async ({ copyFormat }) => { keys.push(copyFormat); await (keys.length === 1 ? first : last); return null; },
  });
  render(<App />);
  const select = screen.getByLabelText("Copy as") as HTMLSelectElement;
  await waitFor(() => expect(select.value).toBe("typst"));
  fireEvent.change(select, { target: { value: "cite" } });
  await waitFor(() => expect(keys).toEqual(["cite"]));
  fireEvent.change(select, { target: { value: "pandoc" } });
  if (route === "menu") await chooseFromMore("Quit Ibis");
  else fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "q", ctrlKey: true });
  expect(calls).not.toContain("quit");
  await act(async () => { finishFirst(); await first; });
  await waitFor(() => expect(keys).toEqual(["cite", "pandoc"]));
  expect(calls).not.toContain("quit");
  await act(async () => { finishLast(); await last; });
  await waitFor(() => expect(calls).toContain("quit"));
});

test.each(["menu", "shortcut"])("%s Quit waits for the initial read, then saves a key and clears the old style together", async (route) => {
  let finishReading = () => undefined as void;
  const reading = new Promise<void>((resolve) => { finishReading = resolve; });
  let rememberedKey = "typst";
  let rememberedStyle: unknown = "apa";
  let atQuit: unknown = null;
  const calls = backend({
    copy_format: async () => { await reading; return "typst"; },
    picker_preferences: async () => { await reading; return { citation_style: "apa", density: "comfortable", larger_text: false }; },
    set_picker_preferences: ({ preferences, copyFormat }) => {
      rememberedStyle = (preferences as { citation_style: unknown }).citation_style;
      if (copyFormat) rememberedKey = copyFormat as string;
      return null;
    },
    quit: () => { atQuit = { copy_format: rememberedKey, citation_style: rememberedStyle }; return null; },
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Copy as"), { target: { value: "cite" } });
  try {
    if (route === "menu") await chooseFromMore("Quit Ibis");
    else await act(async () => { fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "q", ctrlKey: true }); });
    expect(calls).not.toContain("quit");
    expect(atQuit).toBeNull();
    await act(async () => { finishReading(); await reading; });
    await waitFor(() => expect(atQuit).toEqual({ copy_format: "cite", citation_style: null }));
    expect(calls.filter((call) => call === "set_picker_preferences")).toHaveLength(1);
  } finally {
    await act(async () => { finishReading(); await reading; });
  }
});

test("a refused preference save reports failure and still allows Quit", async () => {
  const calls = backend({
    copy_format: () => "typst",
    set_picker_preferences: () => { throw new Error("SYNTHETIC_PRIVATE_SAVE_ERROR"); },
  });
  render(<App />);
  await waitFor(() => expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe("typst"));
  fireEvent.change(screen.getByLabelText("Copy as"), { target: { value: "cite" } });
  await screen.findByText(/could not read or save your picker choices/);
  await chooseFromMore("Quit Ibis");
  await waitFor(() => expect(calls).toContain("quit"));
  expect(screen.queryByText(/SYNTHETIC_PRIVATE_SAVE_ERROR/)).toBeNull();
});

test("a stuck settings request cannot keep Quit waiting indefinitely", async () => {
  const calls = backend({
    copy_format: () => "typst",
    set_picker_preferences: () => new Promise(() => undefined),
  });
  render(<App />);
  await waitFor(() => expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe("typst"));
  vi.useFakeTimers();
  fireEvent.change(screen.getByLabelText("Copy as"), { target: { value: "cite" } });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "q", ctrlKey: true });
  expect(calls).not.toContain("quit");
  await act(() => vi.advanceTimersByTimeAsync(2_000));
  expect(calls).toContain("quit");
});

test("format, density and text size choices are saved without losing each other", async () => {
  const saved: unknown[] = [];
  backend({
    copy_format: () => "typst",
    set_picker_preferences: ({ preferences }) => { saved.push(preferences); return null; },
  });
  render(<App />);
  const select = screen.getByLabelText("Copy as") as HTMLSelectElement;
  await waitFor(() => expect(select.value).toBe("typst"));
  fireEvent.change(select, { target: { value: "style:ieee" } });
  await chooseFromMore("Rows: Comfortable");
  await chooseFromMore("Larger text", "menuitemcheckbox");
  fireEvent.change(select, { target: { value: "pandoc" } });
  await waitFor(() => expect(saved).toHaveLength(4));
  expect(saved).toEqual([
    { citation_style: "ieee", density: "comfortable", larger_text: false },
    { citation_style: "ieee", density: "spacious", larger_text: false },
    { citation_style: "ieee", density: "spacious", larger_text: true },
    { citation_style: null, density: "spacious", larger_text: true },
  ]);
  expect(document.documentElement.dataset.density).toBe("spacious");
});

test("a choice made while saved choices load is kept and saved, not replaced", async () => {
  let finish = () => undefined as void;
  const loaded = new Promise<void>((resolve) => { finish = resolve; });
  const saved: unknown[] = [];
  const keys: unknown[] = [];
  backend({
    copy_format: async () => { await loaded; return "typst"; },
    picker_preferences: async () => { await loaded; return { citation_style: "apa", density: "compact", larger_text: false }; },
    set_picker_preferences: ({ preferences, copyFormat }) => { saved.push(preferences); keys.push(copyFormat); return null; },
  });
  render(<App />);
  const select = screen.getByLabelText("Copy as") as HTMLSelectElement;
  fireEvent.change(select, { target: { value: "cite" } });
  await act(async () => { finish(); await loaded; });
  await waitFor(() => expect(saved).toEqual([{ citation_style: null, density: "compact", larger_text: false }]));
  expect(select.value).toBe("cite");
  expect(keys).toEqual(["cite"]);
});

test("unreadable saved choices are reported and never overwritten with defaults", async () => {
  const calls = backend({
    copy_format: () => "typst",
    picker_preferences: () => { throw new Error("synthetic settings failure"); },
  });
  render(<App />);
  await screen.findByText(/could not read or save your picker choices/);
  // The saved key format still applies, so the next copy does not replace it.
  expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe("typst");
  fireEvent.change(screen.getByLabelText("Copy as"), { target: { value: "style:apa" } });
  await chooseFromMore("Larger text", "menuitemcheckbox");
  expect(document.documentElement.dataset.textSize).toBe("larger");
  expect(calls).not.toContain("set_picker_preferences");
});

const NO_LIBRARY = { state: "none", has_snapshot: false, zotero: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, note: null };

test("first-run setup lists found .bib files by name and folder, and opens one by its list number", async () => {
  const opened: unknown[] = [];
  backend({
    library_status: () => NO_LIBRARY,
    find_sources: () => ({
      zotero: "access_off",
      list: 7,
      files: [{ name: "thesis.bib", folder: "PhD" }, { name: "refs.bib", folder: "Downloads" }],
      cut_short: true,
    }),
    open_found_library: (args) => { opened.push(args); return null; },
  });
  render(<App />);
  const found = await screen.findByRole("region", { name: "Found on this computer" });
  expect(within(found).getAllByRole("button").map((b) => b.textContent)).toEqual(["thesis.bib in PhD", "refs.bib in Downloads"]);
  expect(screen.getByText(/turn on “Allow other applications on this computer to communicate with Zotero”/)).toBeTruthy();
  expect(screen.getByText(/Some folders were too large/)).toBeTruthy();
  fireEvent.click(within(found).getByRole("button", { name: "refs.bib in Downloads" }));
  await waitFor(() => expect(opened).toEqual([{ list: 7, index: 1 }]));
  expect(screen.queryByRole("alert")).toBeNull();
});

test("a refused choice from an older list looks again and shows the new list", async () => {
  let searches = 0;
  backend({
    library_status: () => NO_LIBRARY,
    find_sources: () => {
      searches += 1;
      return { zotero: "absent", list: searches, files: [{ name: `v${searches}.bib`, folder: "Documents" }], cut_short: false };
    },
    open_found_library: () => { throw new Error("That file is no longer offered"); },
  });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "v1.bib in Documents" }));
  expect(await screen.findByRole("button", { name: "v2.bib in Documents" })).toBeTruthy();
  expect(screen.getByText("That file could not be opened from the list. Pick it again from the new list.")).toBeTruthy();
});

test("when looking for libraries fails, setup still offers the file chooser and Zotero and says why", async () => {
  backend({
    library_status: () => NO_LIBRARY,
    find_sources: () => { throw new Error("synthetic failure"); },
  });
  render(<App />);
  expect(await screen.findByText("Ibis could not look for libraries on this computer. Choose a file instead.")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Choose library to get started" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Use Zotero" })).toBeTruthy();
  expect(screen.queryByRole("region", { name: "Found on this computer" })).toBeNull();
});

test("setup failures go away once a library is chosen", async () => {
  let status = NO_LIBRARY as Record<string, unknown>;
  backend({
    library_status: () => status,
    find_sources: () => { throw new Error("synthetic failure"); },
  });
  render(<App />);
  const failure = "Ibis could not look for libraries on this computer. Choose a file instead.";
  await screen.findByText(failure);
  status = { ...NO_LIBRARY, state: "loaded", has_snapshot: true, name: "refs.bib", snapshot_name: "refs.bib", references: 1 };
  await act(async () => { await emit("library:changed", status); });
  await waitFor(() => expect(screen.queryByText(failure)).toBeNull());
});

test("setup says what it found, says when it found nothing, and Look again keeps focus", async () => {
  let answer: unknown = { zotero: "available", list: 1, files: [{ name: "refs.bib", folder: "" }], cut_short: false };
  backend({ library_status: () => NO_LIBRARY, find_sources: () => answer });
  render(<App />);
  expect((await screen.findByText(/Zotero is open/)).textContent).toBe("Zotero is open, so Use Zotero can read your library. 1 .bib file found.");
  expect(screen.getByRole("button", { name: "refs.bib" })).toBeTruthy();
  answer = { zotero: "absent", list: 2, files: [], cut_short: false };
  const again = screen.getByRole("button", { name: "Look again" });
  again.focus();
  fireEvent.click(again);
  expect(await screen.findByText("No Zotero or .bib files found nearby. Use Choose library… to pick a file.")).toBeTruthy();
  expect(document.activeElement).toBe(again);
  expect(screen.queryByRole("region", { name: "Found on this computer" })).toBeNull();
});

function zoteroStatus(libraries: unknown[]) {
  return { state: "loaded", has_snapshot: true, zotero: true, zotero_libraries: libraries,
    name: libraries.length > 1 ? `${libraries.length} Zotero libraries` : "Zotero library",
    snapshot_name: "Zotero library", references: 2, warnings: 0, error: null, note: null };
}

test("More lists Zotero libraries as checkboxes and turning one on reads them together", async () => {
  const chosen: unknown[][] = [];
  backend({
    library_status: () => zoteroStatus(["user"]),
    zotero_groups: () => [{ id: 42, name: "Synthetic lab" }, { id: 7, name: "Reading group" }],
    use_zotero_libraries: ({ libraries }) => { chosen.push(libraries as unknown[]); return null; },
  });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("region", { name: "Library" }).textContent).toMatch(/Zotero/));
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  const group = await screen.findByRole("group", { name: "Zotero libraries" });
  await within(group).findByRole("menuitemcheckbox", { name: "Synthetic lab" });
  const [mine, lab, reading] = within(group).getAllByRole("menuitemcheckbox");
  const boxes = [mine!, lab!, reading!];
  expect(boxes.map((box) => [box.textContent, box.getAttribute("aria-checked")])).toEqual([
    ["My Library", "true"], ["Synthetic lab", "false"], ["Reading group", "false"],
  ]);
  // The only library on cannot be turned off.
  expect(mine!.getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(mine!);
  fireEvent.click(lab!);
  await waitFor(() => expect(chosen).toEqual([["user", { group: 42 }]]));
  expect(screen.getByRole("menu")).toBeTruthy();
});

test("with several Zotero libraries each result names its library, and a left group can be turned off", async () => {
  const chosen: unknown[][] = [];
  backend({
    library_status: () => zoteroStatus(["user", { group: 42 }, { group: 9 }]),
    zotero_groups: () => [{ id: 42, name: "Synthetic lab" }],
    search: ({ query }) => (query ? [result("a", "Mine", "zotero:user"), result("b", "Shared", "zotero:group:42")] : []),
    use_zotero_libraries: ({ libraries }) => { chosen.push(libraries as unknown[]); return null; },
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "x" } });
  const shared = (await screen.findByText("Shared")).closest("li")!;
  await within(shared).findByText("Synthetic lab");
  expect(within(screen.getByText("Mine").closest("li")!).getByText("My Library")).toBeTruthy();
  await chooseFromMore("Group 9", "menuitemcheckbox");
  await waitFor(() => expect(chosen).toEqual([["user", { group: 42 }]]));
});

test("a failed group list is reported and My Library stays available", async () => {
  backend({
    library_status: () => zoteroStatus(["user"]),
    zotero_groups: () => { throw new Error("synthetic: Zotero closed"); },
  });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("region", { name: "Library" }).textContent).toMatch(/Zotero/));
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  expect(await screen.findByText(/could not list your Zotero groups/)).toBeTruthy();
  const group = screen.getByRole("group", { name: "Zotero libraries" });
  expect(within(group).getAllByRole("menuitemcheckbox").map((box) => box.textContent)).toEqual(["My Library"]);
});

test.each(["More", "Escape", "Tab", "outside click"])("closing More with %s dismisses its group-list error and reopening retries", async (closeWith) => {
  let requests = 0;
  let rejectRetry: (error: Error) => void = () => undefined;
  backend({
    library_status: () => zoteroStatus(["user"]),
    zotero_groups: () => {
      if (++requests === 1) throw new Error("synthetic: Zotero closed");
      return new Promise((_, reject) => { rejectRetry = reject; });
    },
  });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("region", { name: "Library" }).textContent).toMatch(/Zotero/));
  const more = screen.getByRole("button", { name: /^More/ });
  fireEvent.click(more);
  await screen.findByText(/could not list your Zotero groups/);
  if (closeWith === "More") fireEvent.click(more);
  else if (closeWith === "outside click") fireEvent.pointerDown(screen.getByLabelText("Search references"));
  else fireEvent.keyDown(screen.getByRole("menu"), { key: closeWith });
  expect(screen.queryByRole("menu")).toBeNull();
  expect(screen.queryByText(/could not list your Zotero groups/)).toBeNull();
  fireEvent.click(more);
  await waitFor(() => expect(requests).toBe(2));
  expect(screen.queryByText(/could not list your Zotero groups/)).toBeNull();
  await act(async () => { rejectRetry(new Error("synthetic: still closed")); });
  expect(await screen.findByText(/could not list your Zotero groups/)).toBeTruthy();
});

test("background group-name lookup keeps cached names without showing a closed-menu error", async () => {
  let unavailable = false;
  backend({
    library_status: () => zoteroStatus(["user", { group: 42 }]),
    zotero_groups: () => {
      if (unavailable) throw new Error("synthetic: Zotero closed");
      return [{ id: 42, name: "Synthetic lab" }];
    },
    search: () => [result("shared", "Shared", "zotero:group:42")],
  });
  render(<App />);
  const shared = (await screen.findByText("Shared")).closest("li")!;
  await within(shared).findByText("Synthetic lab");
  unavailable = true;
  await act(async () => { await emit("library:changed", zoteroStatus(["user", { group: 42 }])); });
  expect(screen.queryByText(/could not list your Zotero groups/)).toBeNull();
  expect(within(shared).getByText("Synthetic lab")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  expect(await screen.findByText(/could not list your Zotero groups/)).toBeTruthy();
  expect(screen.getByRole("menuitemcheckbox", { name: "Synthetic lab" })).toBeTruthy();
});

test.each([false, true])("a late group-list failure stays dismissed after closing More (several libraries: %s)", async (several) => {
  const rejected: ((error: Error) => void)[] = [];
  const calls = backend({
    library_status: () => zoteroStatus(several ? ["user", { group: 42 }] : ["user"]),
    zotero_groups: () => new Promise((_, reject) => { rejected.push(reject); }),
  });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("region", { name: "Library" }).textContent).toMatch(/Zotero/));
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  await waitFor(() => expect(calls).toContain("zotero_groups"));
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
  await act(async () => { rejected.forEach((reject) => reject(new Error("synthetic: late failure"))); });
  expect(screen.queryByText(/could not list your Zotero groups/)).toBeNull();
});

test("a late group-list failure from a previous opening cannot replace a successful menu refresh", async () => {
  const requests: { resolve: (groups: unknown) => void; reject: (error: Error) => void }[] = [];
  backend({
    library_status: () => zoteroStatus(["user", { group: 42 }]),
    zotero_groups: () => new Promise((resolve, reject) => { requests.push({ resolve, reject }); }),
  });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("region", { name: "Library" }).textContent).toMatch(/Zotero/));
  const more = screen.getByRole("button", { name: /^More/ });
  fireEvent.click(more);
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
  const old = [...requests];
  fireEvent.click(more);
  const current = requests.at(-1)!;
  await act(async () => { current.resolve([{ id: 42, name: "Synthetic lab" }]); });
  expect(await screen.findByRole("menuitemcheckbox", { name: "Synthetic lab" })).toBeTruthy();
  await act(async () => { old.forEach(({ reject }) => reject(new Error("synthetic: old failure"))); });
  expect(screen.queryByText(/could not list your Zotero groups/)).toBeNull();
  expect(screen.getByRole("menuitemcheckbox", { name: "Synthetic lab" })).toBeTruthy();
});

test("switching to a file removes a previous Zotero group-list failure", async () => {
  backend({
    library_status: () => zoteroStatus(["user"]),
    zotero_groups: () => { throw new Error("synthetic: Zotero closed"); },
  });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("region", { name: "Library" }).textContent).toMatch(/Zotero/));
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  await screen.findByText(/could not list your Zotero groups/);
  await act(async () => { await emit("library:changed", { state: "loaded", has_snapshot: true, zotero: false,
    name: "synthetic.bib", snapshot_name: "synthetic.bib", references: 1, warnings: 0, error: null, note: null }); });
  expect(screen.queryByRole("group", { name: "Zotero libraries" })).toBeNull();
  expect(screen.queryByText(/could not list your Zotero groups/)).toBeNull();
});

test("two Zotero libraries turned on before the first is reported are both kept, and a refused choice is undone", async () => {
  const chosen: unknown[][] = [];
  let refuse = false;
  backend({
    library_status: () => zoteroStatus(["user"]),
    zotero_groups: () => [{ id: 42, name: "Synthetic lab" }, { id: 7, name: "Reading group" }],
    use_zotero_libraries: ({ libraries }) => {
      if (refuse) throw new Error("synthetic refusal");
      chosen.push(libraries as unknown[]);
      return null;
    },
  });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("region", { name: "Library" }).textContent).toMatch(/Zotero/));
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  const group = await screen.findByRole("group", { name: "Zotero libraries" });
  fireEvent.click(await within(group).findByRole("menuitemcheckbox", { name: "Synthetic lab" }));
  fireEvent.click(within(group).getByRole("menuitemcheckbox", { name: "Reading group" }));
  await waitFor(() => expect(chosen).toEqual([["user", { group: 42 }], ["user", { group: 42 }, { group: 7 }]]));
  expect(within(group).getByRole("menuitemcheckbox", { name: "Reading group" }).getAttribute("aria-checked")).toBe("true");

  await act(async () => { await emit("library:changed", zoteroStatus(["user", { group: 42 }, { group: 7 }])); });
  refuse = true;
  const menu = screen.getByRole("group", { name: "Zotero libraries" });
  fireEvent.click(within(menu).getByRole("menuitemcheckbox", { name: "Reading group" }));
  await screen.findByText(/could not start reading Zotero/);
  expect(within(menu).getByRole("menuitemcheckbox", { name: "Reading group" }).getAttribute("aria-checked")).toBe("true");
});

test("when Zotero refuses other apps and no files are found, setup explains Zotero without saying nothing was found", async () => {
  backend({ library_status: () => NO_LIBRARY, find_sources: () => ({ zotero: "access_off", list: 1, files: [], cut_short: false }) });
  render(<App />);
  expect(await screen.findByText(/does not let other apps read it yet/)).toBeTruthy();
  expect(screen.queryByText(/No Zotero or .bib files found/)).toBeNull();
});

test("setup asks where you write, sets Copy as to match, and shows the shortcut", async () => {
  backend({ library_status: () => NO_LIBRARY });
  render(<App />);
  const places = await screen.findByRole("region", { name: "Where do you write?" });
  fireEvent.click(within(places).getByRole("button", { name: "LaTeX" }));
  await waitFor(() => expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe("cite"));
  expect(within(places).getByRole("button", { name: "LaTeX" }).getAttribute("aria-pressed")).toBe("true");
  fireEvent.click(within(places).getByRole("button", { name: "Word or Google Docs" }));
  await waitFor(() => expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe("style:apa"));
  expect(screen.getByText("Citations copy as APA. Pick another style under Copy as.")).toBeTruthy();
  expect(screen.getByText(/^Open Ibis from any app with /)).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Copy as"), { target: { value: "style:ieee" } });
  expect(await screen.findByText("Citations copy as IEEE. Pick another style under Copy as.")).toBeTruthy();
  expect(within(places).getByRole("button", { name: "Word or Google Docs" }).getAttribute("aria-pressed")).toBe("false");
});

test("shortcuts read as people type them on each system", async () => {
  const { shortcutLabel } = await import("./LibrarySetup");
  expect(shortcutLabel("Control+Command+R", true)).toBe("⌃⌘R");
  expect(shortcutLabel("Control+Shift+Y", false)).toBe("Ctrl+Shift+Y");
  expect(shortcutLabel("CmdOrCtrl+Shift+KeyR", true)).toBe("⌘⇧R");
  expect(shortcutLabel("cmdorctrl + shift + keyr", false)).toBe("Ctrl+Shift+R");
  expect(shortcutLabel("control+command+r", true)).toBe("⌃⌘R");
  expect(shortcutLabel("Alt+Space", false)).toBe("Alt+Space");
});

test("first-run writing choices save the key and style together before copying", async () => {
  const saved: unknown[] = [];
  const calls = backend({
    library_status: () => NO_LIBRARY,
    set_picker_preferences: ({ preferences, copyFormat }) => {
      saved.push({ style: (preferences as { citation_style: unknown }).citation_style, key: copyFormat });
      return null;
    },
  });
  render(<App />);
  const places = await screen.findByRole("region", { name: "Where do you write?" });
  fireEvent.click(within(places).getByRole("button", { name: "Typst" }));
  await waitFor(() => expect(saved).toEqual([{ style: null, key: "typst" }]));
  fireEvent.click(within(places).getByRole("button", { name: "Word or Google Docs" }));
  await waitFor(() => expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe("style:apa"));
  await waitFor(() => expect(saved).toEqual([{ style: null, key: "typst" }, { style: "apa", key: null }]));
  expect(calls).not.toContain("copy_citation");
});
