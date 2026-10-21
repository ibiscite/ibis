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
  return {
    revision: "1", id: { source, native_key: key }, key, title, authors: "Smith", year: 2020, matched: ["title"],
    first_author: "Smith", author_count: 1, container: null as string | null, item_type: "article-journal",
  };
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
    show_copied_notice: () => null,
    set_pinned: () => null,
    copy_format: () => "pandoc",
    picker_preferences: () => ({ citation_style: null, density: "comfortable", larger_text: false }),
    set_picker_preferences: () => null,
    login_item: () => ({ name: "Ibis", enabled: false, problem: null }),
    set_login_item: ({ enabled }) => ({ name: "Ibis", enabled, problem: null }),
    find_sources: () => ({ zotero: "absent", list: 1, files: [], cut_short: false }),
    library_status: () => ({ state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null }),
  };
  mockIPC(
    (cmd, args) => {
      calls.push(cmd);
      const handler = overrides[cmd] ?? defaults[cmd];
      if (!handler) throw new Error(`unexpected command ${cmd}`);
      const value = handler((args ?? {}) as Record<string, unknown>);
      if (cmd === "library_status") return Promise.resolve(value).then((status) => ({ revision: "1", ...status as object }));
      if (cmd === "search") return Promise.resolve(value).then((reply) => Array.isArray(reply) ? { revision: reply[0]?.revision ?? "1", results: reply } : reply);
      return value;
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
  await emit("library:changed", { revision: "1", state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null });
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
    await emit("library:changed", { revision: "1",
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

test("the quit shortcut works with Caps Lock on, like the More shortcut", async () => {
  const calls = backend({ search: () => [result("a", "First")], quit: () => null });
  render(<App />);
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "K", ctrlKey: true });
  expect(screen.getByRole("menu")).toBeTruthy();
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Q", ctrlKey: true });
  await waitFor(() => expect(calls).toContain("quit"));
});

test("identical copy warnings are both listed without duplicate React keys", async () => {
  const errors = vi.spyOn(console, "error").mockImplementation(() => undefined);
  backend({ search: () => [result("a", "First")], copy_citation: () => ({ text: "@a", warnings: ["Missing year", "Missing year"] }) });
  render(<App />);
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByLabelText("Copy warnings");
  expect(screen.getAllByText("Missing year")).toHaveLength(2);
  expect(errors.mock.calls.flat().join(" ")).not.toMatch(/same key/);
  errors.mockRestore();
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
  await emit("library:changed", { revision: "1", state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null });
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
  expect(copies).toEqual([{ ids: [{ source: "refs.bib", native_key: "b" }], format: "typst", expectedRevision: "1" }]);
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
  fireEvent.click(await screen.findByText("Two"), { shiftKey: true });
  expect(screen.getAllByRole("button", { name: /Remove same/ })).toHaveLength(2);
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(copies[0]).toEqual({
    ids: [
      { source: "a.bib", native_key: "same" },
      { source: "b.bib", native_key: "same" },
    ],
    format: "pandoc",
    expectedRevision: "1",
  });
});

test.each([
  { position: "first", count: 3, index: 0 },
  { position: "middle", count: 3, index: 1 },
  { position: "last", count: 3, index: 2 },
  { position: "only", count: 1, index: 0 },
].flatMap((entry) => ["pointer", "Enter", "Space"].map((activation) => ({ ...entry, activation }))))(
  "removing the $position selected reference via $activation restores Search navigation", async ({ count, index, activation }) => {
    // Duplicate native keys must remain distinct by source through removal/copy.
    const records = [result("same", "First", "a.bib"), result("same", "Second", "b.bib"), result("last", "Last", "c.bib")];
    const copies: Record<string, unknown>[] = [];
    const calls = backend({
      search: () => records,
      copy_citation: (args) => {
        copies.push(args);
        return { text: `[@${(args.ids as api.RecordId[]).map((id) => id.native_key).join("; @")}]`, warnings: [] };
      },
    });
    render(<App />);
    const box = screen.getByLabelText("Search references");
    fireEvent.change(box, { target: { value: "focus" } });
    await screen.findByText("First");
    for (let i = 0; i < count; i++) {
      if (i > 0) fireEvent.keyDown(box, { key: "ArrowDown" });
      fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
    }
    for (let i = 1; i < count; i++) fireEvent.keyDown(box, { key: "ArrowUp" });
    const active = box.getAttribute("aria-activedescendant");
    const tray = screen.getByRole("list", { name: "Selected references" });
    const remove = within(tray).getAllByRole("button")[index]!;
    act(() => remove.focus());
    if (activation === "pointer") {
      fireEvent.pointerDown(remove, { button: 0 });
      fireEvent.pointerUp(remove, { button: 0 });
      fireEvent.click(remove, { detail: 1 });
    } else {
      // JSDOM does not synthesize native button clicks from keyboard events.
      const key = activation === "Space" ? " " : "Enter";
      fireEvent.keyDown(remove, { key });
      fireEvent.keyUp(remove, { key });
      fireEvent.click(remove, { detail: 0 });
    }
    const remaining = records.slice(0, count).filter((_, i) => i !== index);
    expect([...document.querySelectorAll('[aria-label="Selected references"] code')].map((node) => node.textContent)).toEqual(remaining.map((record) => record.key));
    expect(box.getAttribute("aria-activedescendant")).toBe(active);
    expect(calls).not.toContain("copy_citation");
    expect(calls).not.toContain("hide_picker");
    expect(document.activeElement).toBe(box);
    expect((box as HTMLInputElement).value).toBe("focus");
    fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    expect(box.getAttribute("aria-activedescendant")).toBe("result-1");
    fireEvent.keyDown(document.activeElement!, { key: "Enter" });
    await waitFor(() => expect(copies).toHaveLength(1));
    expect(copies[0]).toEqual({ ids: (remaining.length ? remaining : [records[1]!]).map((record) => record.id), format: "pandoc", expectedRevision: "1" });
    expect(screen.queryByRole("list", { name: "Selected references" })).toBeNull();
  },
);

test.each([false, true])("a held Enter after removal waits for a fresh press (Shift=%s)", async (shiftKey) => {
  const copies: Record<string, unknown>[] = [];
  backend({
    search: () => [result("a", "First"), result("b", "Second")],
    copy_citation: (args) => { copies.push(args); return { text: "copied", warnings: [] }; },
  });
  render(<App />);
  await screen.findByText("First");
  const box = screen.getByLabelText("Search references");
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  fireEvent.keyDown(box, { key: "ArrowDown" });
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  fireEvent.keyDown(box, { key: "ArrowUp" });
  const remove = screen.getByRole("button", { name: "Remove a from the selection" });
  act(() => remove.focus());
  fireEvent.keyDown(remove, { key: "Enter", shiftKey });
  fireEvent.click(remove, { detail: 0 });
  expect(document.activeElement).toBe(box);
  await act(async () => { fireEvent.keyDown(box, { key: "Enter", repeat: true, shiftKey }); });
  expect(copies).toEqual([]);
  expect([...document.querySelectorAll('[aria-label="Selected references"] code')].map((node) => node.textContent)).toEqual(["b"]);
  fireEvent.keyUp(box, { key: "Enter", shiftKey });
  if (shiftKey) {
    fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
    fireEvent.keyUp(box, { key: "Enter", shiftKey: true });
    expect([...document.querySelectorAll('[aria-label="Selected references"] code')].map((node) => node.textContent)).toEqual(["b", "a"]);
  }
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(copies[0]!.ids).toEqual((shiftKey ? ["b", "a"] : ["b"]).map((key) => ({ source: "refs.bib", native_key: key })));
});

test("hovering Remove keeps focus and menu keys unchanged", async () => {
  const calls = backend({ search: () => [result("a", "First"), result("b", "Second")] });
  render(<App />);
  await screen.findByText("First");
  const box = screen.getByLabelText("Search references");
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  const remove = screen.getByRole("button", { name: "Remove a from the selection" });
  const more = screen.getByRole("button", { name: /^More/ });
  act(() => more.focus());
  fireEvent.mouseEnter(remove);
  fireEvent.mouseMove(remove);
  expect(document.activeElement).toBe(more);
  fireEvent.keyDown(more, { key: "k", ctrlKey: true });
  const menu = await screen.findByRole("menu");
  fireEvent.keyDown(menu, { key: "Escape" });
  expect(document.activeElement).toBe(more);
  expect(screen.getByRole("button", { name: "Remove a from the selection" })).toBe(remove);
  expect(calls).not.toContain("copy_citation");
  expect(calls).not.toContain("hide_picker");
});

test.each([20, 40, 50])("%i long-key selections retain order across queries and first/middle/last removal", async (count) => {
  const records = Array.from({ length: count }, (_, i) => result(`synthetic_${i}_${"long_key_".repeat(5)}`, `Selection reference ${i}`));
  const copies: Record<string, unknown>[] = [];
  backend({
    search: ({ query }) => query === "" ? [] : records.slice(Number(query) * 20, (Number(query) + 1) * 20),
    copy_citation: (args) => {
      copies.push(args);
      return { text: `[@${(args.ids as api.RecordId[]).map((id) => id.native_key).join("; @")}]`, warnings: [] };
    },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  for (let start = 0; start < count; start += 20) {
    fireEvent.change(box, { target: { value: String(start / 20) } });
    await screen.findByText(`Selection reference ${start}`);
    for (let i = start; i < Math.min(start + 20, count); i++) {
      fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
      if (i + 1 < Math.min(start + 20, count)) fireEvent.keyDown(box, { key: "ArrowDown" });
    }
  }
  const tray = screen.getByRole("list", { name: "Selected references" });
  expect(within(tray).getAllByRole("listitem")).toHaveLength(count);
  const removed = [0, Math.floor(count / 2), count - 1];
  for (const i of removed) fireEvent.click(within(tray).getByRole("button", { name: `Remove ${records[i]!.key} from the selection` }));
  const expected = records.filter((_, i) => !removed.includes(i));
  expect([...tray.querySelectorAll("code")].map((node) => node.textContent)).toEqual(expected.map((record) => record.key));
  box.focus();
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(copies[0]).toEqual({ ids: expected.map((record) => record.id), format: "pandoc", expectedRevision: "1" });
  await screen.findByText(`[@${expected.map((record) => record.key).join("; @")}]`);
  expect(screen.queryByRole("list", { name: "Selected references" })).toBeNull();
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

test("while a newer search is pending, no key, click or hint acts on the previous query's results", async () => {
  const pending: Record<string, { resolve: (value: unknown) => void; reject: (error: Error) => void }> = {};
  const copied: unknown[] = [];
  backend({
    search: ({ query }) => query === "kept" ? [result("kept", "Kept result")]
      : query === "old" ? [result("old", "Old result")]
      : query ? new Promise((resolve, reject) => { pending[query as string] = { resolve, reject }; }) : [],
    copy_citation: (args) => { copied.push(args); return { text: "x", warnings: [] }; },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  const chosenKeys = () => screen.queryByRole("list", { name: "Selected references" })?.textContent ?? "";
  // A deliberate choice from an earlier search stays chosen across searches.
  fireEvent.change(box, { target: { value: "kept" } });
  await screen.findByText("Kept result");
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  fireEvent.change(box, { target: { value: "old" } });
  await screen.findByText("Old result");

  // Delayed success: the old rows still show, but nothing acts on or offers them.
  fireEvent.change(box, { target: { value: "pending" } });
  await waitFor(() => expect(pending.pending).toBeDefined());
  fireEvent.keyDown(box, { key: "Alt", altKey: true });
  expect(footerKeys()).toBe("Esc Clear search");
  fireEvent.keyUp(box, { key: "Alt" });
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  // Neither Shift+click (add) nor a plain click (copy) acts on a stale row.
  fireEvent.click(screen.getByText("Old result"), { shiftKey: true });
  fireEvent.click(screen.getByText("Old result"));
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(copied).toHaveLength(1));
  expect(chosenKeys()).not.toContain("old");
  expect(copied).toEqual([{ ids: [{ source: "refs.bib", native_key: "kept" }], format: "pandoc", expectedRevision: "1" }]);
  await act(async () => { pending.pending!.resolve([result("new", "New result")]); });
  await screen.findByText("New result");
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  expect(chosenKeys()).toContain("new");

  // Stale failure: a failed newer search leaves nothing to add.
  fireEvent.change(box, { target: { value: "old" } });
  await screen.findByText("Old result");
  fireEvent.change(box, { target: { value: "failing" } });
  await waitFor(() => expect(pending.failing).toBeDefined());
  await act(async () => { pending.failing!.reject(new Error("search failed")); });
  await screen.findByText("Ibis could not search your library. Use Reload, then try again.");
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  expect(chosenKeys()).not.toContain("old");
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
  await emit("library:changed", { revision: "1", state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 3, warnings: 1, error: null, note: null });
  await waitFor(() => expect(bar.textContent).toContain("refs.bib · 3 references · 1 reading notice"));
  await chooseFromMore("Reload");
  await waitFor(() => expect(calls).toContain("reload_library"));
  await emit("library:changed", { revision: "1",
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
  await act(async () => { await emit("library:changed", { revision: "1", state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null }); });
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

test("the Windows focus ring contrasts with both themes independent of the system accent", () => {
  // macOS and Linux draw the ring in the system accent, as their own controls do
  // (docs/design.md); increased contrast and forced colours make it CanvasText.
  // Windows uses Fluent's ink ring, which must reach 3:1 on every row fill.
  expect(styles).toMatch(/:focus-visible\s*\{[^}]*outline:\s*var\(--focus-ring-width\) solid var\(--focus-ring-color\)/);
  const focusToken = tokens.match(/:root\[data-platform="windows"\]\s*\{[^}]*--focus-ring-color:\s*var\((--[\w-]+)\)/)?.[1];
  expect(focusToken).toBe("--text");
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
  expect(box.getAttribute("aria-autocomplete")).toBe("list");
  expect(box.getAttribute("aria-controls")).toBe(list.id);
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
  expect((box as HTMLInputElement).value).toBe("reorder");
  expect(screen.getByText("First").closest('[role="option"]')?.hasAttribute("data-citation-selected")).toBe(true);
  expect(within(list).getAllByRole("option", { selected: true })).toHaveLength(1);
  expect(document.getElementById(box.getAttribute("aria-activedescendant")!)?.textContent).toContain("Second");
  fireEvent.click(screen.getByText("Second"), { shiftKey: true });
  expect(notice.textContent).toBe("2 references selected for citation");
  fireEvent.click(screen.getAllByRole("button", { name: "Remove same from the selection" })[0]!);
  expect(notice.textContent).toBe("1 reference selected for citation");
  expect(screen.getByText("First").closest('[role="option"]')?.hasAttribute("data-citation-selected")).toBe(false);
  fireEvent.click(screen.getByText("Second"), { shiftKey: true });
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
  await act(async () => { await emit("library:changed", { revision: "1", state: "loaded", has_snapshot: true, zotero: false, name: "研究 refs.bib", snapshot_name: "研究 refs.bib", references: 1, warnings: 0, error: null, note: null }); });
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
  expect(status.textContent).toBe("active A.bib · 3 references");
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
  await act(async () => { await emit("library:changed", { revision: "1", state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 1, warnings: 0, error: null, note: null, settings_notice: notice }); });
  expect(screen.getByText(notice).getAttribute("role")).toBe("alert");
  expect(screen.getAllByText(notice)).toHaveLength(1);
});

test.each(["", "sleep"])("an empty loaded library is explained even for query %s", async (query) => {
  backend({ library_status: () => ({ state: "loaded", has_snapshot: true, zotero: false, name: "empty 研究.bib", snapshot_name: "empty 研究.bib", references: 0, warnings: 0, error: null, note: null }) });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toBe("empty 研究.bib · no references"));
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
    expect(copies[0]).toEqual({ text: "(Garcia, 2021); Second", html: "(Garcia, 2021); Second", expectedRevision: "1" });
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

test("resizing More keeps its focused action visible without undoing manual scrolling", async () => {
  backend();
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: /^More/ }));
  const menu = await screen.findByRole("menu");
  fireEvent.keyDown(document.activeElement!, { key: "End" });
  const last = document.activeElement as HTMLElement;
  expect(last.textContent).toContain("Quit Ibis");
  // JSDOM supplies no layout: model a fractional resized scrollport with Quit below it.
  const menuBox = vi.spyOn(menu, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 8, 240, 191.531));
  const itemBox = vi.spyOn(last, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 220.4, 240, 32));
  const height = vi.spyOn(menu, "clientHeight", "get").mockReturnValue(190);
  const border = vi.spyOn(menu, "clientTop", "get").mockReturnValue(1);
  try {
    fireEvent.scroll(menu);
    expect(menu.scrollTop).toBe(0);
    fireEvent.resize(window);
    expect(menu.scrollTop).toBe(54);
    expect(document.activeElement).toBe(last);
  } finally {
    menuBox.mockRestore();
    itemBox.mockRestore();
    height.mockRestore();
    border.mockRestore();
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
  expect(items).toEqual(["Choose library…", "Use Zotero", "Reload", "Rows: Comfortable", "Keyboard shortcutsCtrl+/", "Quit IbisCtrl+Q"]);
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

test("rows with the same first author and year are told apart by their container and type", async () => {
  const garcia = { authors: "García, Okafor", first_author: "García", author_count: 2, year: 2021 };
  let density = "comfortable";
  backend({
    picker_preferences: () => ({ citation_style: null, density, larger_text: false }),
    search: () => [
      { ...result("a", "Sleep spindles"), ...garcia, container: "Journal of Sleep \u202eResearch", matched: ["title"] },
      { ...result("b", "Sleep spindles: a replication"), ...garcia, container: "Sleep", item_type: "book", matched: ["container", "abstract"] },
      { ...result("c", "Untyped"), item_type: "constructor", matched: ["title"] },
    ],
  });
  const { unmount } = render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "spindles" } });
  await screen.findByText("Untyped");
  let rows = within(screen.getByRole("listbox", { name: "Results" })).getAllByRole("option");
  // Comfortable: authors and container, no type word and no first-author repeat.
  expect(rows[0]?.querySelector(".result-container")?.textContent).toBe("Journal of Sleep \u202eResearch");
  expect(rows[0]?.textContent).toContain("García, Okafor");
  expect(rows[0]?.querySelector(".result-type, .result-first-author")).toBeNull();
  // One separator part between authors and container, spaced like the words
  // around it: a no-break space before the dot and a preserved space after it
  // (white-space: pre), and no gap, margin or padding of its own.
  const byline = rows[0]?.querySelector(".result-byline");
  expect(byline?.querySelector(".result-authors + .result-separator[aria-hidden] + .result-container")).toBeTruthy();
  expect(byline?.textContent).toBe("García, Okafor\u00a0· Journal of Sleep \u202eResearch");
  expect(byline?.querySelector(".result-separator")?.textContent).toBe("\u00a0· ");
  // In the narrow layout the line may break after the separator, not inside a word.
  expect(styles).toMatch(/\.result-separator \{ white-space: pre-wrap; \}/);
  // Each clipping part takes its own direction, so a right-to-left row cuts
  // Latin text at its end rather than its start.
  for (const part of [".result-authors", ".result-container", ".result-key"]) expect(rows[0]?.querySelector(part)?.getAttribute("dir")).toBe("auto");
  expect(styles).toMatch(/\.result-separator \{ flex: none; white-space: pre; \}/);
  const bylineRules = [...styles.matchAll(/([^{}]*\.result-(?:byline|authors|separator|container)\b[^{}]*)\{([^}]*)\}/g)];
  expect(bylineRules.length).toBeGreaterThan(0);
  for (const [, , body] of bylineRules) expect(body).not.toMatch(/margin|padding|gap|(^|[^-])width:/);
  // The row may shorten the container, so a match there is always named.
  expect(rows[1]?.textContent).toContain("Matched in");
  expect(rows[2]?.querySelector(".result-container")).toBeNull();
  unmount();

  density = "spacious";
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "spindles" } });
  await screen.findByText("Untyped");
  rows = within(screen.getByRole("listbox", { name: "Results" })).getAllByRole("option");
  expect(rows[0]?.querySelector(".result-type")?.textContent).toBe("Article");
  expect(rows[1]?.querySelector(".result-type")?.textContent).toBe("Book");
  expect(rows[2]?.querySelector(".result-type")).toBeNull();
  cleanup();

  density = "compact";
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "spindles" } });
  await screen.findByText("Untyped");
  rows = within(screen.getByRole("listbox", { name: "Results" })).getAllByRole("option");
  // Compact: the first author follows the title on the one line, before the year.
  const line = rows[0]?.querySelector(".result-line");
  expect(line?.querySelector(".result-first-author")?.getAttribute("dir")).toBe("auto");
  // "et al." is isolated, so it keeps its order beside a right-to-left name.
  expect(line?.querySelector(".result-first-author bdi")?.textContent).toBe("et al.");
  // No reliance on text-align: match-parent, which Chromium ignores here: the
  // container is only as wide as its text, so it sits beside its separator.
  expect(styles).not.toMatch(/match-parent/);
  // It keeps its own width beside a long title, as in the prototype, capped and ellipsized.
  expect(styles).toMatch(/\.result-first-author \{ flex: none; max-width: var\(--key-max-width\);[^}]*text-overflow: ellipsis/);
  // A very narrow Compact row gives the title its own line instead.
  expect(styles).toMatch(/@container \(max-width: 16em\) \{\s*:root\[data-density="compact"\] \.result-line \{ flex-wrap: wrap; row-gap: 0; \}\s*:root\[data-density="compact"\] \.result-title \{ flex-basis: 100%; \}/);
  expect(line?.textContent).toBe("Sleep spindlesGarcía et al.2021");
  expect(rows[2]?.querySelector(".result-first-author")?.textContent).toBe("Smith");
  expect(rows[0]?.querySelector(".result-container, .result-type")).toBeNull();
  // The container is out of sight in Compact, so a match there is named.
  expect(rows[1]?.textContent).toContain("Matched in");
});

test("rows shorten the authors and keep the journal, key and library label visible", async () => {
  backend({ search: () => [
    { ...result("a", "Six authors"), authors: "García, Okafor, Lima, Chen, Novak, Haddad", first_author: "García", author_count: 6, container: "Journal of Sleep Research" },
    // Two creators are both named; the core counts an organisation with a comma as one.
    { ...result("b", "Organisation"), authors: "Smith, Institute, Research", first_author: "Smith", author_count: 2, container: "Sleep" },
    { ...result("c", "One author"), container: "Sleep" },
  ] });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  await screen.findByText("One author");
  const rows = within(screen.getByRole("listbox", { name: "Results" })).getAllByRole("option");
  const authors = (row?: HTMLElement) => row?.querySelector(".result-authors");
  // Three or more: "García et al." on screen; screen readers still hear every name.
  expect(authors(rows[0])?.querySelector("[aria-hidden]")?.textContent).toBe("García et al.");
  expect(authors(rows[0])?.querySelector("[aria-hidden] bdi")?.textContent).toBe("et al.");
  expect(authors(rows[0])?.querySelector(".visually-hidden")?.textContent).toBe("García, Okafor, Lima, Chen, Novak, Haddad");
  expect(authors(rows[1])?.textContent).toBe("Smith, Institute, Research");
  expect(authors(rows[2])?.textContent).toBe("Smith");
  // The key and library label give way first, down to a few letters; the type
  // word and the authors (up to 60% of the byline) keep their width, and the
  // container shortens first. Below 25em the meta line may wrap instead (no
  // row gap). Chromium probe numbers for the crowded cases are in PR #502.
  expect(styles).not.toMatch(/line-clamp/);
  expect(styles).toMatch(/\.result-meta \{[^}]*flex-wrap: wrap; row-gap: 0;/);
  // Rows at least 25em wide (461px or more at normal text) never wrap.
  expect(styles).toMatch(/\[role="option"\] \{ container-type: inline-size; \}/);
  expect(styles).toMatch(/@container \(min-width: 25em\) \{ \.result-meta \{ flex-wrap: nowrap; \} \}/);
  expect(styles).toMatch(/\.result-byline \{[^}]*flex: 1 1 auto; min-width: min\(12em, 60%\)/);
  expect(styles).toMatch(/\.result-authors \{ flex: none; max-width: 60%;[^}]*text-overflow: ellipsis/);
  expect(styles).toMatch(/\.result-container \{ flex: 0 1 auto; min-width: 0;[^}]*text-overflow: ellipsis/);
  expect(styles).toMatch(/\.result-key \{ flex: 0 1000 auto; min-width: min\(6ch, 100%\);[^}]*text-overflow: ellipsis/);
  expect(styles).toMatch(/\.result-library \{ flex: 0 1000 auto; min-width: min\(5em, 100%\);[^}]*text-overflow: ellipsis/);
  expect(styles).toMatch(/\.result-type \{ flex: none; max-width: 100%;[^}]*text-overflow: ellipsis/);
  for (const name of ["key", "library"]) expect(styles).not.toMatch(new RegExp(`\\.result-${name} \\{[^}]*min-width: 0`));
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
  // The select changes when React commits the loaded choices, but density and
  // text size reach <html> in an effect that runs after that commit. The load
  // resolves outside act(), so waitFor can see the select before the effect
  // has run; wait for the attributes themselves.
  await waitFor(() => {
    expect(document.documentElement.dataset.density).toBe("compact");
    expect(document.documentElement.dataset.textSize).toBe("larger");
  });
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
  return { revision: "1", state: "loaded", has_snapshot: true, zotero: true, zotero_libraries: libraries,
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

test("removing one of two chosen references with the same key names its library", async () => {
  backend({
    library_status: () => zoteroStatus(["user", { group: 42 }]),
    zotero_groups: () => [{ id: 42, name: "Synthetic lab" }],
    search: ({ query }) => (query ? [result("garcia2021", "Mine", "zotero:user"), result("garcia2021", "Shared", "zotero:group:42"), result("other", "Other", "zotero:user")] : []),
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "x" } });
  await within((await screen.findByText("Shared")).closest("li")!).findByText("Synthetic lab");
  // Shift+click adds a row to the citation (a plain click copies it).
  fireEvent.click(screen.getByText("Other"), { shiftKey: true });
  fireEvent.click(screen.getByText("Mine"), { shiftKey: true });
  // Only one garcia2021 chosen so far: its key is enough.
  expect(screen.getByRole("button", { name: "Remove garcia2021 from the selection" })).toBeTruthy();
  fireEvent.click(screen.getByText("Shared"), { shiftKey: true });
  const chosen = within(screen.getByRole("list", { name: "Selected references" }));
  expect(chosen.getByRole("button", { name: "Remove garcia2021 (My Library) from the selection" })).toBeTruthy();
  expect(chosen.getByRole("button", { name: "Remove garcia2021 (Synthetic lab) from the selection" })).toBeTruthy();
  expect(chosen.getByRole("button", { name: "Remove other from the selection" })).toBeTruthy();
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
  await act(async () => { await emit("library:changed", { revision: "1", state: "loaded", has_snapshot: true, zotero: false,
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

test("a clean copy hides the picker, then says Copied with the copied text", async () => {
  const notices: unknown[] = [];
  const calls = backend({
    search: () => [result("a", "First")],
    copy_citation: () => ({ text: "[@a]", warnings: [] }),
    show_copied_notice: (args) => { notices.push(args); return null; },
  });
  render(<App />);
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(notices).toEqual([{ text: "[@a]" }]));
  expect(calls.indexOf("hide_picker")).toBeLessThan(calls.indexOf("show_copied_notice"));
});

test("copies with warnings, pinned copies and failed hides show no Copied notice window", async () => {
  let warnings = ["bare keys cannot carry page numbers"];
  let hideFails = false;
  const calls = backend({
    search: () => [result("a", "First")],
    copy_citation: () => ({ text: "a", warnings }),
    hide_picker: () => { if (hideFails) throw new Error("SYNTHETIC"); return null; },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Enter" });
  await screen.findByLabelText("Copy warnings");
  fireEvent.click(screen.getByRole("button", { name: "Pin Ibis" }));
  warnings = [];
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(calls.filter((c) => c === "copy_citation")).toHaveLength(2));
  fireEvent.click(screen.getByRole("button", { name: "Pin Ibis" }));
  hideFails = true;
  fireEvent.keyDown(box, { key: "Enter" });
  await screen.findByText("Ibis could not hide this window. Try again.");
  expect(calls).not.toContain("show_copied_notice");
});

test("a failed Copied notice never undoes the copy or the hide", async () => {
  const log = vi.spyOn(console, "error").mockImplementation(() => undefined);
  try {
    backend({
      search: () => [result("a", "First")],
      copy_citation: () => ({ text: "@a", warnings: [] }),
      show_copied_notice: () => { throw new Error("SYNTHETIC_PRIVATE_PAYLOAD"); },
    });
    render(<App />);
    await screen.findByText("First");
    fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
    await waitFor(() => expect(log).toHaveBeenCalledWith("The Copied notice could not be shown."));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("@a")).toBeTruthy();
    expect(log.mock.calls.flat().join(" ")).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
  } finally { log.mockRestore(); }
});

test("clicking a row copies it like Enter, and Shift+click adds it like Shift+Enter", async () => {
  const copies: unknown[] = [];
  const calls = backend({
    search: () => [result("a", "First"), result("b", "Second")],
    copy_citation: (args) => { copies.push(args); return { text: "@b", warnings: [] }; },
  });
  render(<App />);
  const second = (await screen.findByText("Second")).closest('[role="option"]')!;
  fireEvent.click(second, { shiftKey: true });
  expect(second.hasAttribute("data-citation-selected")).toBe(true);
  expect(document.activeElement).toBe(screen.getByLabelText("Search references"));
  fireEvent.click(second, { shiftKey: true });
  expect(second.hasAttribute("data-citation-selected")).toBe(false);
  expect(copies).toEqual([]);
  fireEvent.click(second);
  await waitFor(() => expect(calls).toContain("hide_picker"));
  expect(copies).toEqual([{ ids: [{ source: "refs.bib", native_key: "b" }], format: "pandoc", expectedRevision: "1" }]);
  expect(second.getAttribute("aria-selected")).toBe("true");
});

test("with sources in the citation, a click copies the citation like Enter", async () => {
  const copies: unknown[] = [];
  backend({
    search: () => [result("a", "First"), result("b", "Second")],
    copy_citation: (args) => { copies.push(args); return { text: "@a", warnings: [] }; },
  });
  render(<App />);
  fireEvent.click(await screen.findByText("First"), { shiftKey: true });
  fireEvent.click(screen.getByText("Second"));
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(copies[0]).toEqual({ ids: [{ source: "refs.bib", native_key: "a" }], format: "pandoc", expectedRevision: "1" });
});

test("a drag, a double click's second click or a modified click on a row copies nothing", async () => {
  const calls = backend({
    search: () => [result("a", "First")],
    copy_citation: () => ({ text: "@a", warnings: [] }),
  });
  render(<App />);
  const row = (await screen.findByText("First")).closest('[role="option"]')!;
  fireEvent.pointerDown(row, { clientX: 10, clientY: 10 });
  fireEvent.click(row, { clientX: 60, clientY: 12 });
  fireEvent.click(row, { detail: 2 });
  fireEvent.click(row, { ctrlKey: true });
  fireEvent.click(row, { altKey: true });
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(calls).not.toContain("copy_citation");
  expect(row.hasAttribute("data-citation-selected")).toBe(false);
  fireEvent.pointerDown(row, { clientX: 10, clientY: 10 });
  fireEvent.click(row, { clientX: 12, clientY: 11 });
  await waitFor(() => expect(calls).toContain("copy_citation"));
});

test("a click on a row waits for its formatted preview before copying it", async () => {
  const { InlineWorker } = await import("./format/inline-worker");
  vi.stubGlobal("Worker", InlineWorker);
  const copies: unknown[] = [];
  backend({
    picker_preferences: () => ({ citation_style: "apa", density: "comfortable", larger_text: false }),
    search: () => [result("a", "First"), result("b", "Second")],
    csl_items: ({ ids }) => JSON.stringify((ids as { native_key: string }[]).map((id) => ({
      id: id.native_key, type: "article-journal", title: id.native_key === "a" ? "First" : "Second",
      author: [{ family: "Smith", given: "A" }], issued: { "date-parts": [[2020]] },
    }))),
    copy_formatted: (args) => { copies.push(args); return null; },
  });
  render(<App />);
  await waitFor(() => expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe("style:apa"));
  fireEvent.click(await screen.findByText("Second"));
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(String((copies[0] as { text: string }).text)).toContain("Smith");
  expect(screen.queryByText("The preview is still being made. Try again in a moment.")).toBeNull();
});

test.each([[30_000, "first"], [61_000, ""]])(
  "a search cleared by Escape comes back selected when reopened soon after (%i ms)", async (away, kept) => {
    const calls = backend({ search: () => [result("a", "First")] });
    const now = vi.spyOn(performance, "now").mockReturnValue(1_000_000);
    try {
      render(<App />);
      const box = screen.getByLabelText("Search references") as HTMLInputElement;
      fireEvent.change(box, { target: { value: "first" } });
      await screen.findByText("First");
      fireEvent.keyDown(box, { key: "Escape" });
      expect(box.value).toBe("");
      fireEvent.keyDown(box, { key: "Escape" });
      await waitFor(() => expect(calls).toContain("hide_picker"));
      await act(async () => { await emit("picker:hidden"); });
      now.mockReturnValue(1_000_000 + away);
      await act(async () => { await emit("picker:shown"); });
      await waitFor(() => expect(box.value).toBe(kept));
      expect(document.activeElement).toBe(box);
      expect([box.selectionStart, box.selectionEnd]).toEqual([0, kept.length]);
    } finally {
      now.mockRestore();
    }
  });

test("a search cleared by Escape stays cleared once something else is typed or copied", async () => {
  backend({ search: () => [result("a", "First")], copy_citation: () => ({ text: "@a", warnings: [] }) });
  render(<App />);
  const box = screen.getByLabelText("Search references") as HTMLInputElement;
  fireEvent.change(box, { target: { value: "first" } });
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Escape" });
  fireEvent.change(box, { target: { value: "f" } });
  fireEvent.change(box, { target: { value: "" } });
  await act(async () => { await emit("picker:hidden"); });
  await act(async () => { await emit("picker:shown"); });
  expect(box.value).toBe("");
  fireEvent.change(box, { target: { value: "first" } });
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Escape" });
  // Browsing the empty search after Escape, then copying from it.
  await waitFor(() => expect(screen.getByText("References")).toBeTruthy());
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  fireEvent.keyDown(box, { key: "Enter" });
  await screen.findByText("@a");
  await act(async () => { await emit("picker:hidden"); });
  await act(async () => { await emit("picker:shown"); });
  expect(box.value).toBe("");
});

test.each(["Escape", "hidden"])("a click waiting for its preview never copies after the picker is left (%s)", async (leave) => {
  const { InlineWorker } = await import("./format/inline-worker");
  vi.stubGlobal("Worker", InlineWorker);
  let release: () => void = () => undefined;
  const ready = new Promise<void>((resolve) => { release = resolve; });
  const calls = backend({
    picker_preferences: () => ({ citation_style: "apa", density: "comfortable", larger_text: false }),
    search: () => [result("a", "First"), result("b", "Second")],
    csl_items: ({ ids }) => ready.then(() => JSON.stringify((ids as { native_key: string }[]).map((id) => ({
        id: id.native_key, type: "article-journal", title: id.native_key,
        author: [{ family: "Smith", given: "A" }], issued: { "date-parts": [[2020]] },
      })))),
    copy_formatted: () => null,
  });
  render(<App />);
  await waitFor(() => expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe("style:apa"));
  fireEvent.click(await screen.findByText("Second"));
  if (leave === "Escape") {
    fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Escape" });
    await waitFor(() => expect(calls).toContain("hide_picker"));
  }
  await act(async () => { await emit("picker:hidden"); });
  const hides = calls.filter((c) => c === "hide_picker").length;
  await act(async () => { release(); await new Promise((resolve) => setTimeout(resolve, 50)); });
  await waitFor(() => expect(document.querySelector(".format-preview")?.textContent).toContain("Smith"));
  expect(calls).not.toContain("copy_formatted");
  expect(calls.filter((c) => c === "hide_picker")).toHaveLength(hides);
  expect(calls).not.toContain("show_copied_notice");
});

const footerKeys = () => [...screen.getByRole("group", { name: "Keys" }).children]
  .filter((hint) => hint.getAttribute("aria-hidden") !== "true").map((hint) => hint.textContent).join("");

test("the footer says what Enter will copy, and holding Alt shows the other keys", async () => {
  backend({ search: ({ query }) => (query ? [result("a", "First"), result("b", "Second")] : []) });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  // Nothing to copy yet, so Enter is not advertised.
  await waitFor(() => expect(footerKeys()).toBe("Ctrl+/ All keys"));
  fireEvent.change(box, { target: { value: "x" } });
  await screen.findByText("First");
  expect(footerKeys()).toBe("Enter Copy as PandocCtrl+/ All keys");
  fireEvent.change(screen.getByLabelText("Copy as"), { target: { value: "typst" } });
  expect(footerKeys()).toContain("Enter Copy as Typst");
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  fireEvent.keyDown(box, { key: "ArrowDown" });
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  expect(footerKeys()).toContain("Enter Copy 2 as Typst");
  fireEvent.keyDown(box, { key: "Alt", altKey: true });
  expect(footerKeys()).toBe("Shift+Enter Remove from citationEsc Clear search");
  fireEvent.keyUp(box, { key: "Alt" });
  expect(footerKeys()).toContain("Copy 2 as Typst");
  fireEvent.keyDown(box, { key: "Alt", altKey: true });
  fireEvent.blur(window);
  expect(footerKeys()).not.toContain("Esc");
});

test.each([false, true])("Search hint layout survives pointer focus moving to More (Alt=%s)", async (alt) => {
  backend({ search: () => [result("a", "First")] });
  render(<App />);
  await screen.findByText("First");
  const box = screen.getByLabelText("Search references");
  if (alt) fireEvent.keyDown(box, { key: "Alt", altKey: true });
  const hints = screen.getByRole("group", { name: "Keys" });
  const searchHint = hints.firstElementChild!;
  const text = searchHint.textContent;
  expect(text).toContain(alt ? "Add to citation" : "Copy as Pandoc");
  const more = screen.getByRole("button", { name: /^More/ });
  fireEvent.pointerDown(more, { button: 0 });
  // JSDOM does not perform the browser's pointer-down focus default.
  act(() => more.focus());
  expect(searchHint.isConnected).toBe(true);
  expect(searchHint.textContent).toBe(text);
  expect(searchHint.getAttribute("aria-hidden")).toBe("true");
  fireEvent.pointerUp(more, { button: 0 });
  fireEvent.click(more);
  expect(await screen.findByRole("menu")).toBeTruthy();
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
  expect(document.activeElement).toBe(more);
  act(() => box.focus());
  expect(searchHint.getAttribute("aria-hidden")).toBe("false");
});

test("the footer names the source and count, the age only when it matters, and the detail in More", async () => {
  const plain = (text: string | null | undefined) => text?.replace(/[\u2068\u2069]/g, "");
  let status: Record<string, unknown> = { state: "loaded", has_snapshot: true, zotero: true, snapshot_zotero: true, stale: false,
    zotero_libraries: ["user", { group: 7 }], name: "2 Zotero libraries", snapshot_name: "2 Zotero libraries", references: 1204, warnings: 0,
    error: null, note: null, updated_at: null, checked_at: Date.now() - 3 * 3_600_000 };
  backend({ library_status: () => status, zotero_groups: () => [{ id: 7, name: "Lab group" }] });
  render(<App />);
  const live = await screen.findByRole("status", { name: "Library status" });
  await waitFor(() => expect(live.textContent).toBe("Zotero · 1,204 references"));
  const item = live.closest(".library-status")!;
  expect(item.textContent).toBe("Zotero · 1,204 references");
  expect(item.getAttribute("data-health")).toBe("done");
  expect(plain(item.getAttribute("title"))).toMatch(/^Zotero · 1,204 references\nMy Library, (Group 7|Lab group) · checked 3 hours ago, .+$/);
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  const group = await screen.findByRole("group", { name: /Lab group/ });
  expect(plain(group.getAttribute("aria-label"))).toMatch(/^Zotero · 1,204 references\. My Library, Lab group · checked 3 hours ago, /);
  expect(plain(group.querySelector(".menu-detail-title")?.textContent)).toBe("Zotero · 1,204 references");

  // A change just arrived: the age shows, outside the live region.
  status = { ...status, references: 1205, updated_at: Date.now(), checked_at: Date.now() };
  await emit("library:changed", status);
  await waitFor(() => expect(live.textContent).toBe("Zotero · 1,205 references"));
  expect(item.textContent).toBe("Zotero · 1,205 references · updated just now");

  // Zotero closed: the copy still searched says when it was last checked.
  status = { ...status, state: "error", stale: true, checked_at: Date.now() - 2 * 3_600_000,
    error: "Zotero is not running, or it is not accepting connections; keeping the last loaded references" };
  await emit("library:changed", status);
  await waitFor(() => expect(item.textContent).toBe("Zotero · not updated · checked 2 hours ago"));
  expect(item.getAttribute("data-health")).toBe("look");
  expect(plain(item.getAttribute("title"))).toMatch(/^Zotero · 1,205 references\nMy Library, Lab group · checked 2 hours ago, .+ · 2 Zotero libraries could not be read\. Zotero is not running, or it is not accepting connections; keeping the last loaded references\. Open Zotero, then reload\.$/);
});

test("? lists every key in a modal dialog, but a ? typed in the search is text", async () => {
  const calls = backend({ search: () => [result("a", "First")] });
  render(<App />);
  const box = screen.getByLabelText("Search references") as HTMLInputElement;
  fireEvent.keyDown(box, { key: "?" });
  fireEvent.change(box, { target: { value: "?" } });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(box.value).toBe("?");
  fireEvent.keyDown(box, { key: "/", ctrlKey: true });
  const dialog = await screen.findByRole("dialog", { name: "Keyboard shortcuts" });
  expect(dialog.getAttribute("aria-modal")).toBe("true");
  const close = within(dialog).getByRole("button", { name: "Close" });
  expect(document.activeElement).toBe(close);
  const rows = within(dialog).getAllByRole("row").slice(1).map((row) => within(row).getAllByRole("cell").map((cell) => cell.textContent));
  expect(rows).toContainEqual(["Enter", "Copy the highlighted reference, or the ones added to the citation, in the Copy as format", "Search box"]);
  expect(rows).toContainEqual(["Shift+Enter", "Add the highlighted reference to the citation, or remove it", "Search box"]);
  expect(rows).toContainEqual(["Ctrl+K", "Open or close More", "Anywhere in Ibis"]);
  expect(rows).toContainEqual(["Ctrl+/ or ?", "Show this list (? works outside the search box)", "Anywhere in Ibis"]);
  expect(rows).toContainEqual(["Home or End", "Go to the first or last item", "More menu"]);
  // The global shortcut, labelled for this platform, is listed once it works.
  await waitFor(() => expect(within(dialog).getAllByRole("row").at(-1)?.textContent).toBe("Ctrl+Super+ROpen IbisAny app"));
  // Focus stays in the dialog, and the picker's keys wait until it closes.
  fireEvent.keyDown(close, { key: "Tab" });
  expect(dialog.contains(document.activeElement)).toBe(true);
  fireEvent.keyDown(document.activeElement!, { key: "Tab", shiftKey: true });
  expect(dialog.contains(document.activeElement)).toBe(true);
  fireEvent.keyDown(document.activeElement!, { key: "k", ctrlKey: true });
  expect(screen.queryByRole("menu")).toBeNull();
  const canvas = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  try {
    const found = await axe.run(document.body, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa"] }, rules: { "color-contrast": { enabled: false } } });
    expect(found.violations.map((v) => v.id)).toEqual([]);
  } finally {
    canvas.mockRestore();
  }
  // Clicking plain text focuses the dialog itself, so Escape still reaches it.
  expect(dialog.getAttribute("tabindex")).toBe("-1");
  expect(within(dialog).getByRole("table").getAttribute("aria-labelledby")).toBeNull();
  dialog.focus();
  fireEvent.keyDown(dialog, { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(document.activeElement).toBe(box);
  expect(box.value).toBe("?");
  expect(calls).not.toContain("hide_picker");
  // Outside a text field, a bare ? opens it, and focus returns there.
  const more = screen.getByRole("button", { name: /^More/ });
  more.focus();
  fireEvent.keyDown(more, { key: "?" });
  fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Close" }));
  expect(document.activeElement).toBe(more);
  // More lists it with its key; closing returns to the search box.
  await chooseFromMore("Keyboard shortcuts");
  expect(await screen.findByRole("dialog")).toBeTruthy();
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  expect(document.activeElement).toBe(box);
});

test("when nothing matches, the picker offers to clear the search", async () => {
  backend({ search: () => [] });
  render(<App />);
  const box = screen.getByLabelText("Search references") as HTMLInputElement;
  fireEvent.change(box, { target: { value: "nothing here" } });
  expect(await screen.findByText("No matching references")).toBeTruthy();
  expect(screen.getByText("Check the spelling, or try fewer words, an author's surname or a year.")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Search all Zotero libraries" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Clear search" }));
  expect(box.value).toBe("");
  expect(document.activeElement).toBe(box);
  expect(await screen.findByText("Type to find a reference")).toBeTruthy();
});

test("when nothing matches in some Zotero libraries, the picker offers to search them all", async () => {
  const chosen: unknown[][] = [];
  backend({
    library_status: () => zoteroStatus(["user"]),
    zotero_groups: () => [{ id: 42, name: "Synthetic lab" }],
    use_zotero_libraries: ({ libraries }) => { chosen.push(libraries as unknown[]); return null; },
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "elsewhere" } });
  fireEvent.click(await screen.findByRole("button", { name: "Search all Zotero libraries" }));
  await waitFor(() => expect(chosen).toEqual([["user", { group: 42 }]]));
  expect(screen.queryByRole("button", { name: "Search all Zotero libraries" })).toBeNull();
});

test("the footer offers Enter only when Enter in the search box would copy", async () => {
  const { InlineWorker } = await import("./format/inline-worker");
  vi.stubGlobal("Worker", InlineWorker);
  let release: () => void = () => undefined;
  let fail = false;
  backend({
    search: () => [result("a", "First"), result("b", "Second")],
    csl_items: async (args) => {
      await new Promise<void>((resolve) => { release = resolve; });
      if (fail) throw new Error("synthetic: formatting failed");
      return JSON.stringify((args.ids as { native_key: string }[]).map((id) =>
        ({ id: id.native_key, type: "book", title: id.native_key, author: [{ family: "Alpha" }], issued: { "date-parts": [[2021]] } })));
    },
    copy_formatted: () => null,
  });
  try {
    render(<App />);
    await screen.findByText("First");
    const box = screen.getByLabelText("Search references");
    expect(footerKeys()).toContain("Enter Copy as Pandoc");
    // Another control has focus, so Enter would press it instead.
    const more = screen.getByRole("button", { name: /^More/ });
    act(() => more.focus());
    expect(footerKeys()).not.toContain("Enter");
    act(() => box.focus());
    expect(footerKeys()).toContain("Enter Copy as Pandoc");
    // A formatted citation still being made is not offered.
    fireEvent.change(screen.getByLabelText("Copy as"), { target: { value: "style:apa" } });
    await screen.findByText("Formatting…");
    expect(footerKeys()).not.toContain("Enter");
    await act(async () => release());
    await waitFor(() => expect(footerKeys()).toContain("Enter Copy as APA"));
    // Nor is one that failed.
    fail = true;
    fireEvent.keyDown(box, { key: "ArrowDown" });
    await waitFor(() => expect(screen.getByText("Formatting…")).toBeTruthy());
    await act(async () => release());
    expect(await screen.findByText(/could not be formatted/)).toBeTruthy();
    expect(footerKeys()).not.toContain("Enter");
  } finally {
    vi.unstubAllGlobals();
  }
});

test("More shows whether Ibis starts at login as the system reports it, and follows changes made elsewhere", async () => {
  let system = true;
  const calls = backend({
    login_item: () => ({ name: "Ibis", enabled: system, problem: null }),
    set_login_item: ({ enabled }) => { system = Boolean(enabled); return { enabled: system, problem: null }; },
  });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  const item = await screen.findByRole("menuitemcheckbox", { name: "Start Ibis when you log in" });
  await waitFor(() => expect(item.getAttribute("aria-checked")).toBe("true"));
  fireEvent.click(item);
  await waitFor(() => expect(calls).toContain("set_login_item"));
  expect(system).toBe(false);
  // Removed or added again in System Settings while Ibis was hidden.
  system = true;
  await act(async () => { await emit("picker:shown"); });
  if (!screen.queryByRole("menu")) fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  await waitFor(() => expect(screen.getByRole("menuitemcheckbox", { name: "Start Ibis when you log in" }).getAttribute("aria-checked")).toBe("true"));
  expect(screen.queryByRole("alert")).toBeNull();
});

test("a refused login item is explained kindly and never shown as on", async () => {
  backend({ set_login_item: () => ({ name: "Ibis", enabled: false, problem: "refused" }) });
  render(<App />);
  await chooseFromMore("Start Ibis when you log in", "menuitemcheckbox");
  const alert = await screen.findByText(/^The system did not let Ibis change this\. You can change it in /);
  expect(alert.getAttribute("role")).toBe("alert");
  if (!screen.queryByRole("menu")) fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  expect((await screen.findByRole("menuitemcheckbox", { name: "Start Ibis when you log in" })).getAttribute("aria-checked")).toBe("false");
});

test("a failed login item command restores the previous state and says so", async () => {
  backend({ set_login_item: () => { throw new Error("ipc"); } });
  render(<App />);
  await chooseFromMore("Start Ibis when you log in", "menuitemcheckbox");
  expect(await screen.findByText("Ibis could not change whether it starts when you log in. Try again.")).toBeTruthy();
  if (!screen.queryByRole("menu")) fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  expect((await screen.findByRole("menuitemcheckbox", { name: "Start Ibis when you log in" })).getAttribute("aria-checked")).toBe("false");
});

test("systems without login items, and pickers without the commands, offer no choice", async () => {
  backend({ login_item: () => ({ name: "Ibis", enabled: false, problem: "unsupported" }) });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  await screen.findByRole("menuitemcheckbox", { name: "Larger text" });
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  expect(screen.queryByRole("menuitemcheckbox", { name: "Start Ibis when you log in" })).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
  cleanup();
  const { loginItem: _read, setLoginItem: _set, ...withoutLoginItem } = api;
  backend();
  render(<App api={withoutLoginItem} />);
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  await screen.findByRole("menuitemcheckbox", { name: "Larger text" });
  expect(screen.queryByRole("menuitemcheckbox", { name: "Start Ibis when you log in" })).toBeNull();
});

test("first-run setup offers starting at login beside the shortcut, off until chosen", async () => {
  let system = false;
  backend({
    library_status: () => NO_LIBRARY,
    login_item: () => ({ name: "Ibis", enabled: system, problem: null }),
    set_login_item: ({ enabled }) => { system = Boolean(enabled); return { enabled: system, problem: null }; },
  });
  const { container } = render(<App />);
  const box = await screen.findByRole("checkbox", { name: "Start Ibis when you log in" }) as HTMLInputElement;
  expect(box.checked).toBe(false);
  expect(system).toBe(false);
  fireEvent.click(box);
  await waitFor(() => expect(box.checked).toBe(true));
  expect(system).toBe(true);
  const results = await axe.run(container);
  expect(results.violations.map((v) => v.id)).toEqual([]);
});

test("each system's login item problems name where to look", async () => {
  const { loginItemsPlace, loginItemText } = await import("./messages");
  expect(loginItemsPlace("MacIntel")).toBe("System Settings > General > Login Items");
  expect(loginItemsPlace("Win32")).toBe("Settings > Apps > Startup");
  expect(loginItemsPlace("Linux x86_64")).toMatch(/startup applications/);
  for (const problem of ["unsupported", "needs_approval", "refused", "unreadable"] as const) {
    expect(loginItemText.problem(problem, "PLACE", "Ibis Nightly")).toMatch(/Ibis Nightly.*PLACE\.$/);
  }
});

test("quick clicks end in the state of the last click, and the app's own name is shown", async () => {
  let system = false;
  const order: boolean[] = [];
  const pending: (() => void)[] = [];
  backend({
    login_item: () => ({ name: "Ibis Nightly", enabled: system, problem: null }),
    // The first change answers last if the calls were not kept in order.
    set_login_item: ({ enabled }) => new Promise((resolve) => {
      order.push(Boolean(enabled));
      const finish = () => { system = Boolean(enabled); resolve({ name: "Ibis Nightly", enabled: system, problem: null }); };
      if (order.length === 1) pending.push(finish); else finish();
    }),
  });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  const item = await screen.findByRole("menuitemcheckbox", { name: "Start Ibis Nightly when you log in" });
  fireEvent.click(item);
  await waitFor(() => expect(item.getAttribute("aria-checked")).toBe("true"));
  fireEvent.click(item);
  await waitFor(() => expect(item.getAttribute("aria-checked")).toBe("false"));
  // The second call waits for the first.
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  expect(order).toEqual([true]);
  await act(async () => { pending.shift()?.(); await new Promise((resolve) => setTimeout(resolve, 0)); });
  await waitFor(() => expect(order).toEqual([true, false]));
  await waitFor(() => expect(system).toBe(false));
  expect(item.getAttribute("aria-checked")).toBe("false");
});

test.each(["style:ieee", "pandoc", "style:apa"])("changing format cancels a click waiting for its preview (%s)", async (next) => {
  const { InlineWorker } = await import("./format/inline-worker");
  vi.stubGlobal("Worker", InlineWorker);
  let release: () => void = () => undefined;
  const ready = new Promise<void>((resolve) => { release = resolve; });
  const output: unknown[] = [];
  const calls = backend({
    picker_preferences: () => ({ citation_style: "apa", density: "comfortable", larger_text: false }),
    search: () => [result("a", "First"), result("b", "Second")],
    csl_items: ({ ids }) => ready.then(() => JSON.stringify((ids as { native_key: string }[]).map((id) => ({
      id: id.native_key, type: "article-journal", title: id.native_key,
      author: [{ family: "Smith" }], issued: { "date-parts": [[2020]] },
    })))),
    copy_formatted: (args) => { output.push(args); return null; },
    copy_citation: (args) => { output.push(args); return { text: "[@b]", warnings: [] }; },
  });
  render(<App />);
  const control = screen.getByLabelText("Copy as") as HTMLSelectElement;
  await waitFor(() => expect(control.value).toBe("style:apa"));
  fireEvent.click(await screen.findByText("Second"));
  // Returning to APA must not revive the earlier click either.
  if (next === "style:apa") fireEvent.change(control, { target: { value: "style:ieee" } });
  fireEvent.change(control, { target: { value: next } });
  await act(async () => { release(); await ready; });
  if (next !== "pandoc") await waitFor(() => expect(document.querySelector(".format-preview")?.textContent).toContain(next === "style:ieee" ? "[1]" : "Smith"));
  expect(output).toEqual([]);
  expect(calls).not.toContain("hide_picker");
  expect(calls).not.toContain("show_copied_notice");
  fireEvent.click(screen.getByText("Second"));
  await waitFor(() => expect(output).toHaveLength(1));
  expect(output[0]).toMatchObject(next === "pandoc"
    ? { format: "pandoc", ids: [{ source: "refs.bib", native_key: "b" }] }
    : { text: next === "style:ieee" ? "[1]" : "(Smith, 2020)" });
  expect(calls).toContain(next === "pandoc" ? "copy_citation" : "copy_formatted");
});

test.each(["click", "Enter", "Shift+Enter"])("a replacement snapshot blocks prior rows until refreshed search (%s)", async (action) => {
  let replacement = false;
  let release: (value: unknown) => void = () => undefined;
  const copied: unknown[] = [];
  const status = (revision: string) => ({ state: "loaded", has_snapshot: true, zotero: true, zotero_libraries: ["user"], snapshot_zotero: true, stale: false, updated_at: null, checked_at: null, settings_notice: null, name: "Zotero library", snapshot_name: "Zotero library", references: 1, warnings: 0, error: null, note: null, revision });
  const old = { ...result("ITEM0001", "Old title and key", "zotero:user"), key: "oldkey", revision: "snapshot-1" };
  const fresh = { ...result("ITEM0001", "Replacement title and key", "zotero:user"), key: "newkey", revision: "snapshot-2" };
  const calls = backend({
    library_status: () => status("snapshot-1"),
    search: () => replacement ? new Promise((resolve) => { release = resolve; }) : [old],
    copy_citation: (args) => { copied.push(args); return { text: replacement ? "[@newkey]" : "[@oldkey]", warnings: [] }; },
  });
  render(<App />);
  await screen.findByText("Old title and key");
  replacement = true;
  await act(async () => { await emit("library:changed", status("snapshot-2")); });
  if (action === "click") fireEvent.click(screen.getByText("Old title and key"));
  else fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter", shiftKey: action === "Shift+Enter" });
  await act(async () => { await Promise.resolve(); });
  expect(copied).toEqual([]);
  expect(screen.queryByRole("list", { name: "Selected references" })).toBeNull();
  expect(calls).not.toContain("hide_picker");
  await act(async () => { release([fresh]); });
  fireEvent.click(await screen.findByText("Replacement title and key"));
  await waitFor(() => expect(copied).toHaveLength(1));
  expect(copied[0]).toMatchObject({ expectedRevision: "snapshot-2" });
});

test("a newer search revision refreshes status even before its event arrives", async () => {
  let reads = 0;
  const copied: unknown[] = [];
  backend({
    library_status: () => ({ state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", references: 1, warnings: 0, error: null, revision: ++reads === 1 ? "1" : "2" }),
    search: () => [{ ...result("a", "Fresh before event"), revision: "2" }],
    copy_citation: (args) => { copied.push(args); return { text: "@a", warnings: [] }; },
  });
  render(<App />);
  await screen.findByText("Fresh before event");
  await waitFor(() => expect(reads).toBeGreaterThan(1));
  fireEvent.click(screen.getByText("Fresh before event"));
  await waitFor(() => expect(copied).toHaveLength(1));
  expect(copied[0]).toMatchObject({ expectedRevision: "2" });
});


test.each([false, true])("pre-event search replies recover status, including empty replies (%s)", async (empty) => {
  let reads = 0;
  backend({
    library_status: () => ({ ...zoteroStatus(["user"]), revision: ++reads === 1 ? "1" : "2", references: empty ? 0 : 1 }),
    search: () => ({ revision: "2", results: empty ? [] : [{ ...result("a", "Fresh revision"), revision: "2" }] }),
  });
  render(<App />);
  await waitFor(() => expect(reads).toBe(2));
  if (!empty) await screen.findByText("Fresh revision");
  await act(async () => { await Promise.resolve(); });
  expect(reads).toBe(2);
});

test("a pre-event copy rejection recovers without copying or hiding until a fresh click", async () => {
  let revision = "1";
  const attempts: unknown[] = [];
  const writes: string[] = [];
  const calls = backend({
    library_status: () => ({ ...zoteroStatus(["user"]), revision }),
    search: () => [{ ...result("ITEM0001", `Snapshot ${revision}`, "zotero:user"), revision }],
    copy_citation: (args) => {
      attempts.push(args.expectedRevision);
      if (args.expectedRevision !== revision) throw "library snapshot changed";
      writes.push(revision);
      return { text: "fresh copy", warnings: [] };
    },
  });
  render(<App />);
  await screen.findByText("Snapshot 1");
  revision = "2";
  fireEvent.click(screen.getByText("Snapshot 1"));
  await screen.findByText("The library changed. Search again and reselect the references before copying.");
  await screen.findByText("Snapshot 2");
  expect(attempts).toEqual(["1"]);
  expect(writes).toEqual([]);
  expect(calls).not.toContain("hide_picker");
  fireEvent.click(screen.getByText("Snapshot 2"));
  await waitFor(() => expect(writes).toEqual(["2"]));
  expect(attempts).toEqual(["1", "2"]);
});

test("mixed selected revisions retain their original tokens and cannot copy", async () => {
  let revision = "1";
  const copies: unknown[] = [];
  backend({
    library_status: () => ({ ...zoteroStatus(["user"]), revision }),
    search: () => [{ ...result(revision, `Selected ${revision}`), revision }],
    copy_citation: (args) => { copies.push(args); return { text: "unexpected", warnings: [] }; },
  });
  render(<App />);
  fireEvent.click(await screen.findByText("Selected 1"), { shiftKey: true });
  revision = "2";
  await act(async () => { await emit("library:changed", { ...zoteroStatus(["user"]), revision }); });
  fireEvent.click(await screen.findByText("Selected 2"), { shiftKey: true });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByText("The library changed. Search again and reselect the references before copying.");
  expect(copies).toEqual([]);
  expect(screen.getAllByRole("button", { name: /^Remove / })).toHaveLength(2);
});

test.each(["resolve", "reject"])("an older recovery status cannot override a newer event (%s)", async (outcome) => {
  let revision = "1";
  let reads = 0;
  let finish: (value: unknown) => void = () => undefined;
  let reject: (reason: unknown) => void = () => undefined;
  const calls = backend({
    library_status: () => ++reads === 1 ? { ...zoteroStatus(["user"]), revision } : new Promise((resolve, fail) => { finish = resolve; reject = fail; }),
    search: () => [{ ...result("a", `Recovery ${revision}`), revision }],
    copy_citation: () => { throw "library snapshot changed"; },
  });
  render(<App />);
  fireEvent.click(await screen.findByText("Recovery 1"));
  await waitFor(() => expect(reads).toBe(2));
  revision = "3";
  await act(async () => { await emit("library:changed", { ...zoteroStatus(["user"]), revision }); });
  await screen.findByText("Recovery 3");
  await act(async () => { if (outcome === "resolve") finish({ ...zoteroStatus(["user"]), revision: "2" }); else reject("synthetic failure"); });
  expect(screen.queryByText("Recovery 3")).not.toBeNull();
  expect(screen.queryByText("Ibis could not check your library. Choose the file again.")).toBeNull();
  expect(calls).not.toContain("hide_picker");
});


test("a retired formatted preview sends its own revision and requires a fresh copy", async () => {
  const { InlineWorker } = await import("./format/inline-worker");
  vi.stubGlobal("Worker", InlineWorker);
  let revision = "1";
  const attempts: unknown[] = [];
  const writes: string[] = [];
  let copiedText: unknown;
  const calls = backend({
    library_status: () => ({ ...zoteroStatus(["user"]), revision }),
    search: () => [{ ...result("ITEM0001", `Formatted ${revision}`, "zotero:user"), revision }],
    picker_preferences: () => ({ citation_style: "apa", density: "comfortable", larger_text: false }),
    csl_items: ({ expectedRevision }) => {
      if (expectedRevision !== revision) throw "library snapshot changed";
      // Each revision has its own year, so the preview shows which one it formatted.
      const year = revision === "1" ? 2020 : 2021;
      return JSON.stringify([{ id: "ITEM0001", type: "book", title: "Synthetic", author: [{ family: "Smith" }], issued: { "date-parts": [[year]] } }]);
    },
    copy_formatted: ({ text, expectedRevision }) => {
      attempts.push(expectedRevision);
      if (expectedRevision !== revision) throw "library snapshot changed";
      writes.push(revision);
      copiedText = text;
      return null;
    },
  });
  try {
    render(<App />);
    await screen.findByText("Formatted 1");
    const preview = await screen.findByRole("region", { name: "Preview" });
    await within(preview).findByText("(Smith, 2020)");
    revision = "2";
    fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
    await screen.findByText("The library changed. Search again and reselect the references before copying.");
    await screen.findByText("Formatted 2");
    // The preview is rebuilt for the new rows, and the retired one read the
    // same text, so look up the current region and wait for revision 2's year.
    await waitFor(() => within(screen.getByRole("region", { name: "Preview" })).getByText("(Smith, 2021)"));
    expect(screen.queryByText("(Smith, 2020)")).toBeNull();
    expect(attempts).toEqual(["1"]);
    expect(writes).toEqual([]);
    expect(calls).not.toContain("hide_picker");
    fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
    await waitFor(() => expect(writes).toEqual(["2"]));
    expect(attempts).toEqual(["1", "2"]);
    expect(copiedText).toBe("(Smith, 2021)");
  } finally {
    vi.unstubAllGlobals();
  }
});

test("a late search reply cannot replace rows from the newer snapshot", async () => {
  let revision = "1";
  const pending: Record<string, (value: unknown) => void> = {};
  const copied: unknown[] = [];
  backend({
    library_status: () => ({ ...zoteroStatus(["user"]), revision }),
    search: () => revision === "1" ? [result("a", "Late 1")] : new Promise((resolve) => { pending[revision] = resolve; }),
    copy_citation: (args) => { copied.push(args.expectedRevision); return { text: "@a", warnings: [] }; },
  });
  render(<App />);
  await screen.findByText("Late 1");
  for (const next of ["2", "3"]) {
    revision = next;
    await act(async () => { await emit("library:changed", { ...zoteroStatus(["user"]), revision }); });
    await waitFor(() => expect(pending[next]).toBeTypeOf("function"));
  }
  await act(async () => { pending["3"]!({ revision: "3", results: [{ ...result("a", "Late 3"), revision: "3" }] }); });
  await screen.findByText("Late 3");
  await act(async () => { pending["2"]!({ revision: "2", results: [{ ...result("a", "Late 2"), revision: "2" }] }); });
  expect(screen.queryByText("Late 2")).toBeNull();
  fireEvent.click(screen.getByText("Late 3"));
  await waitFor(() => expect(copied).toEqual(["3"]));
});


test.each([false, true])("copy completion preserves selections changed while pending (pinned=%s)", async (pinned) => {
  let release: (value: unknown) => void = () => undefined;
  const calls = backend({
    search: () => [result("a", "First"), result("b", "Second")],
    copy_citation: () => new Promise((resolve) => { release = resolve; }),
  });
  render(<App />);
  if (pinned) fireEvent.click(screen.getByRole("button", { name: "Pin Ibis" }));
  fireEvent.click(await screen.findByText("First"), { shiftKey: true });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(calls).toContain("copy_citation"));
  fireEvent.click(screen.getByText("Second"), { shiftKey: true });
  await act(async () => { release({ text: "@a", warnings: [] }); });
  expect(screen.queryByRole("button", { name: "Remove b from the selection" })).not.toBeNull();
  expect(screen.queryByRole("button", { name: "Remove a from the selection" })).not.toBeNull();
  expect(calls).not.toContain("hide_picker");
  expect(calls).not.toContain("show_copied_notice");
});


test.each(["selection ABA", "query ABA", "hide and reopen"])("copy completion ignores a newer interaction (%s)", async (change) => {
  let release: (value: unknown) => void = () => undefined;
  const calls = backend({
    search: () => [result("a", "First")],
    copy_citation: () => new Promise((resolve) => { release = resolve; }),
  });
  render(<App />);
  const first = await screen.findByText("First");
  const box = screen.getByLabelText("Search references");
  fireEvent.click(first, { shiftKey: true });
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(calls).toContain("copy_citation"));
  if (change === "selection ABA") {
    fireEvent.click(first, { shiftKey: true });
    fireEvent.click(first, { shiftKey: true });
  } else if (change === "query ABA") {
    fireEvent.change(box, { target: { value: "different" } });
    fireEvent.change(box, { target: { value: "" } });
  } else {
    await act(async () => { await emit("picker:hidden"); await emit("picker:shown"); });
  }
  await act(async () => { release({ text: "obsolete copy", warnings: [] }); });
  expect(screen.queryByRole("button", { name: "Remove a from the selection" })).not.toBeNull();
  expect(screen.queryByText("obsolete copy")).toBeNull();
  expect(calls).not.toContain("hide_picker");
  expect(calls).not.toContain("show_copied_notice");
});

test.each(["success", "failure"])("an older copy cannot overwrite a newer copy's warning (%s)", async (oldOutcome) => {
  const resolve: ((value: unknown) => void)[] = [];
  const reject: ((reason: unknown) => void)[] = [];
  backend({
    search: () => [result("a", "First")],
    copy_citation: () => new Promise((done, fail) => { resolve.push(done); reject.push(fail); }),
  });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "Pin Ibis" }));
  fireEvent.click(await screen.findByText("First"), { shiftKey: true });
  const box = screen.getByLabelText("Search references");
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(resolve).toHaveLength(1));
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(resolve).toHaveLength(2));
  await act(async () => { resolve[1]!({ text: "new copy", warnings: ["New warning"] }); });
  await screen.findByText("New warning");
  await act(async () => {
    if (oldOutcome === "success") resolve[0]!({ text: "old copy", warnings: ["Old warning"] });
    else reject[0]!("old failure");
  });
  expect(screen.queryByText("New warning")).not.toBeNull();
  expect(screen.queryByText("Old warning")).toBeNull();
  expect(screen.queryByText("Nothing was copied. Try again, or use Reload.")).toBeNull();
});

test.each([false, true])("copy completion uses the current pin choice (initially=%s)", async (initiallyPinned) => {
  let release: (value: unknown) => void = () => undefined;
  const calls = backend({ search: () => [result("a", "First")], copy_citation: () => new Promise((resolve) => { release = resolve; }) });
  render(<App />);
  const pin = screen.getByRole("button", { name: "Pin Ibis" });
  if (initiallyPinned) fireEvent.click(pin);
  fireEvent.click(await screen.findByText("First"), { shiftKey: true });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(calls).toContain("copy_citation"));
  fireEvent.click(pin);
  await act(async () => { release({ text: "@a", warnings: [] }); });
  expect(screen.queryByRole("button", { name: "Remove a from the selection" })).toBeNull();
  expect(calls.includes("hide_picker")).toBe(initiallyPinned);
});

test("a delayed hide cannot show an old copy notice after reopening", async () => {
  let releaseHide: () => void = () => undefined;
  const calls = backend({
    search: () => [result("a", "First")],
    copy_citation: () => ({ text: "@a", warnings: [] }),
    hide_picker: () => new Promise<void>((resolve) => { releaseHide = resolve; }),
  });
  render(<App />);
  fireEvent.click(await screen.findByText("First"));
  await waitFor(() => expect(calls).toContain("hide_picker"));
  await act(async () => { await emit("picker:shown"); releaseHide(); });
  expect(calls).not.toContain("show_copied_notice");
});

test("a delayed hide cannot show an old notice over a newer selection", async () => {
  let releaseHide: () => void = () => undefined;
  const calls = backend({
    search: () => [result("a", "First"), result("b", "Second")],
    copy_citation: () => ({ text: "@a", warnings: [] }),
    hide_picker: () => new Promise<void>((resolve) => { releaseHide = resolve; }),
  });
  render(<App />);
  fireEvent.click(await screen.findByText("First"));
  await waitFor(() => expect(calls).toContain("hide_picker"));
  fireEvent.click(screen.getByText("Second"), { shiftKey: true });
  await act(async () => { releaseHide(); });
  expect(screen.queryByRole("button", { name: "Remove b from the selection" })).not.toBeNull();
  expect(calls).not.toContain("show_copied_notice");
});

test.each(["selection", "copy feedback"])("an obsolete stale copy recovers data without replacing a newer %s", async (change) => {
  let revision = "1";
  let rejectOld: (reason: unknown) => void = () => undefined;
  let reads = 0;
  const attempts: unknown[] = [];
  const calls = backend({
    library_status: () => { reads += 1; return { ...zoteroStatus(["user"]), revision }; },
    search: () => [
      { ...result("a", `First revision ${revision}`), revision },
      { ...result("b", `Second revision ${revision}`), revision },
    ],
    copy_citation: (args) => {
      attempts.push(args.expectedRevision);
      if (attempts.length === 1) return new Promise((_resolve, reject) => { rejectOld = reject; });
      return { text: "newer copy", warnings: ["Keep the newer warning"] };
    },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  if (change === "copy feedback") fireEvent.click(screen.getByRole("button", { name: "Pin Ibis" }));
  fireEvent.click(await screen.findByText("First revision 1"), { shiftKey: true });
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(attempts).toHaveLength(1));
  if (change === "selection") fireEvent.click(screen.getByText("Second revision 1"), { shiftKey: true });
  else {
    fireEvent.keyDown(box, { key: "Enter" });
    await screen.findByText("Keep the newer warning");
  }
  revision = "2"; // Backend replaced the snapshot without delivering its event.
  await act(async () => { rejectOld("library snapshot changed"); });
  await screen.findByText("First revision 2");
  expect(reads).toBeGreaterThan(1);
  expect(screen.queryByText("The library changed. Search again and reselect the references before copying.")).toBeNull();
  expect(calls).not.toContain("hide_picker");
  expect(calls).not.toContain("show_copied_notice");
  if (change === "selection") {
    expect(screen.queryByRole("button", { name: "Remove a from the selection" })).not.toBeNull();
    expect(screen.queryByRole("button", { name: "Remove b from the selection" })).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Remove a from the selection" }));
    fireEvent.click(screen.getByRole("button", { name: "Remove b from the selection" }));
    fireEvent.click(screen.getByText("Second revision 2"));
    await waitFor(() => expect(attempts).toEqual(["1", "2"]));
  } else {
    expect(screen.queryByText("Keep the newer warning")).not.toBeNull();
    expect(attempts).toEqual(["1", "1"]);
  }
});

test.each([
  ...["2", "3"].flatMap((refreshedRevision) => [false, true].map((selectedCopy) => ({ refreshedRevision, selectedCopy, changedResults: false }))),
  { refreshedRevision: "2", selectedCopy: false, changedResults: true },
])("an old recovery preserves a pending copy only for the same snapshot and results ($refreshedRevision, selected=$selectedCopy, changed=$changedResults)", async ({ refreshedRevision, selectedCopy, changedResults }) => {
  let revision = "1";
  let reads = 0;
  let searches = 0;
  let rejectOld: (reason: unknown) => void = () => undefined;
  let finishNew: (value: unknown) => void = () => undefined;
  const attempts: unknown[] = [];
  const calls = backend({
    library_status: () => { reads += 1; return { ...zoteroStatus(["user"]), revision, source_counts: { "zotero:user": 2 } }; },
    search: () => {
      searches += 1;
      const keys = changedResults ? (searches >= 3 ? ["a"] : ["b", "a"]) : ["a", "b"];
      return keys.map((key) => ({ ...result(key, `Pending ${revision} ${key}`, "zotero:user"), revision }));
    },
    copy_citation: ({ expectedRevision }) => {
      attempts.push(expectedRevision);
      return attempts.length === 1
        ? new Promise((_resolve, reject) => { rejectOld = reject; })
        : new Promise((resolve) => { finishNew = resolve; });
    },
  });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "Pin Ibis" }));
  fireEvent.click(await screen.findByText("Pending 1 a"), { shiftKey: true });
  const box = screen.getByLabelText("Search references");
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(attempts).toEqual(["1"]));
  revision = "2";
  await act(async () => { await emit("library:changed", { ...zoteroStatus(["user"]), revision }); });
  await screen.findByText("Pending 2 b");
  fireEvent.click(screen.getByRole("button", { name: "Remove a from the selection" }));
  fireEvent.click(screen.getByText("Pending 2 b"), { shiftKey: selectedCopy });
  if (selectedCopy) fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(attempts).toEqual(["1", "2"]));
  revision = refreshedRevision;
  await act(async () => { rejectOld("library snapshot changed"); });
  await waitFor(() => expect(reads).toBe(2));
  await waitFor(() => expect(searches).toBe(3));
  if (changedResults) await waitFor(() => expect(screen.queryByText("Pending 2 b")).toBeNull());
  else await screen.findByText(`Pending ${refreshedRevision} b`);
  await act(async () => { finishNew({ text: "new valid copy", warnings: ["Keep newest warning"] }); });
  expect(Boolean(screen.queryByText("Keep newest warning"))).toBe(refreshedRevision === "2" && !changedResults);
  expect(Boolean(screen.queryByRole("button", { name: "Remove b from the selection" }))).toBe(selectedCopy && refreshedRevision !== "2");
  expect(screen.queryByText("The library changed. Search again and reselect the references before copying.")).toBeNull();
  expect(calls).not.toContain("hide_picker");
  expect(calls).not.toContain("show_copied_notice");
});

test.each(["warning", "notice"])("a search started before a selected copy preserves its completion (%s)", async (feedback) => {
  let releaseSearch: (value: unknown) => void = () => undefined;
  let finishCopy: (value: unknown) => void = () => undefined;
  const notices: unknown[] = [];
  const calls = backend({
    search: ({ query }) => query === "pending"
      ? new Promise((resolve) => { releaseSearch = resolve; })
      : [result("a", "Selected before search")],
    copy_citation: () => new Promise((resolve) => { finishCopy = resolve; }),
    show_copied_notice: ({ text }) => { notices.push(text); return null; },
  });
  render(<App />);
  if (feedback === "warning") fireEvent.click(screen.getByRole("button", { name: "Pin Ibis" }));
  fireEvent.click(await screen.findByText("Selected before search"), { shiftKey: true });
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "pending" } });
  await waitFor(() => expect(calls.filter((call) => call === "search")).toHaveLength(2));
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(calls).toContain("copy_citation"));
  await act(async () => { releaseSearch([result("b", "Search finished later")]); });
  await screen.findByText("Search finished later");
  await act(async () => { finishCopy({ text: "selected copy", warnings: feedback === "warning" ? ["Keep selected warning"] : [] }); });
  expect(screen.queryByRole("button", { name: "Remove a from the selection" })).toBeNull();
  if (feedback === "warning") {
    expect(screen.queryByText("Keep selected warning")).not.toBeNull();
    expect(calls).not.toContain("hide_picker");
    expect(notices).toEqual([]);
  } else {
    await waitFor(() => expect(notices).toEqual(["selected copy"]));
    expect(calls).toContain("hide_picker");
  }
});

test.each([false, true])("retained stale freshness stays in accessible footer details with retry (Zotero=%s)", async (zotero) => {
  const reason = zotero ? "Zotero is not running; keeping the last loaded references" : "The library file is unavailable; keeping the last loaded references";
  const name = zotero ? "Zotero library" : "refs.bib";
  const calls = backend({
    library_status: () => ({ ...zoteroStatus(["user"]), state: "error", zotero, snapshot_zotero: zotero, name, snapshot_name: name, stale: true, error: reason }),
    search: () => [result("kept", "Retained searchable reference")],
    reload_library: () => null,
  });
  const { container } = render(<App />);
  await screen.findByText("Retained searchable reference");
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("not updated");
  expect(container.querySelector(".status-dot")?.getAttribute("data-health")).toBe("look");
  expect(container.querySelector(".library-detail")?.textContent).toContain(reason);
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  expect(screen.getByRole("menu").textContent).toContain(reason);
  expect(screen.getByRole("group", { name: new RegExp(zotero ? "Open Zotero, then reload" : "Choose it again or reconnect") })).toBeTruthy();
  await chooseFromMore("Reload");
  await waitFor(() => expect(calls).toContain("reload_library"));
});

test("retained stale freshness does not hide settings or copy failures", async () => {
  const notice = "Synthetic settings recovery notice";
  backend({
    library_status: () => ({ ...zoteroStatus(["user"]), state: "error", stale: true, error: "Zotero is not running; keeping the last loaded references", settings_notice: notice }),
    search: () => [result("kept", "Retained reference with errors")],
    copy_citation: () => { throw new Error("synthetic clipboard failure"); },
  });
  render(<App />);
  await screen.findByText("Retained reference with errors");
  expect(screen.getAllByRole("alert")).toHaveLength(1);
  expect(screen.getByRole("alert").textContent).toContain(notice);
  fireEvent.click(screen.getByText("Retained reference with errors"));
  await screen.findByText("Nothing was copied. Try again, or use Reload.");
  expect(screen.getAllByRole("alert")).toHaveLength(2);
});

test("a retained freshness warning does not hide an actual search failure", async () => {
  backend({
    library_status: () => ({ ...zoteroStatus(["user"]), state: "error", stale: true, error: "Zotero is not running; keeping the last loaded references" }),
    search: () => { throw new Error("synthetic search failure"); },
  });
  render(<App />);
  await screen.findByText("Ibis could not search your library. Use Reload, then try again.");
  expect(screen.getAllByRole("alert")).toHaveLength(1);
});
