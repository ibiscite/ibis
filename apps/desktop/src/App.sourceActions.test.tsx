import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, resetApp, result } from "./testing/app";

afterEach(resetApp);
const file = { source_generation: "source-1", source_is_requested: true, revision: "1", state: "loaded", has_snapshot: true, zotero: false, snapshot_zotero: false,
  name: "thesis.bib", snapshot_name: "thesis.bib", references: 12, warnings: 0, error: null, problem: null, stale: false,
  zotero_libraries: [], source_counts: { "synthetic-file": 12 }, checked_at: null, updated_at: null };
const footer = () => within(document.querySelector("footer")!);
const commandO = (altKey = false) => fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "o", ctrlKey: true, altKey });
const icon = { mime: "image/png", bytes: [137, 80, 78, 71, 13, 10, 26, 10] };

test("one quiet footer fix has the runtime icon, label and keys, with a short status and full tooltip", async () => {
  backend({ library_status: () => ({ ...file, zotero: true, snapshot_zotero: true, name: "Zotero", snapshot_name: "Zotero",
    stale: true, state: "error", error: "Zotero could not be refreshed", problem: "zotero_closed", checked_at: Date.now() - 7_200_000 }),
    library_source: () => ({ kind: "zotero", name: "Zotero", icon, status_icon: icon }) });
  render(<App />);
  const fix = await screen.findByRole("button", { name: "Open Zotero" });
  await waitFor(() => expect(fix.querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,iVBORw0KGgo="));
  expect(fix.getAttribute("data-variant")).toBe("quiet");
  expect(fix.getAttribute("aria-keyshortcuts")).toBe("Control+O");
  expect([...fix.children].map((node) => node.className)).toEqual(["ui-button-icon", "key-hint"]);
  expect(fix.querySelector("kbd")?.textContent).toBe("Ctrl+O");
  expect(screen.getByRole("status", { name: "Library status" }).textContent).toBe("Zotero not updated");
  expect(document.querySelector(".library-age")?.textContent).toBe(" · 2 h");
  expect(document.querySelector(".library-status-trigger")?.getAttribute("title")).toContain("last read 2 hours ago");
  expect(footer().queryByRole("button", { name: "Shortcuts" })).toBeNull();
  expect(document.querySelector(".notices")?.textContent).toBe("");
});

test("a missing file's Find recovery keeps its file icon in the footer and More", async () => {
  const fileIcon = { mime: "image/png", bytes: [1, 2, 3] };
  const choose = vi.fn(() => null);
  backend({ library_status: () => ({ ...file, state: "error", stale: true, problem: "missing", error: "The file moved" }),
    library_source: () => ({ kind: "file", name: "thesis.bib", icon: fileIcon, status_icon: fileIcon }), choose_library: choose });
  render(<App />);
  const fix = await screen.findByRole("button", { name: "Find ⁨thesis.bib⁩…" });
  await waitFor(() => expect(fix.querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,AQID"));
  expect(fix.getAttribute("aria-keyshortcuts")).toBe("Control+O");
  fireEvent.click(footer().getByRole("button", { name: "More" }));
  const source = await screen.findByRole("menuitem", { name: "Find ⁨thesis.bib⁩…" });
  expect(source.querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,AQID");
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
  fireEvent.click(await screen.findByRole("button", { name: "Find ⁨thesis.bib⁩…" }));
  await waitFor(() => expect(choose).toHaveBeenCalledOnce());
});

test("Command-O follows the selected reference; Option reveals the file without a shell path from the webview", async () => {
  const open = vi.fn(() => null);
  const picked = result("picked", "Selected source reference");
  backend({ library_status: () => file, search: () => [picked], open_library_source: open,
    library_source: () => ({ kind: "file", name: "thesis.bib", icon: null }) });
  render(<App />);
  await screen.findByText("Selected source reference");
  commandO();
  await waitFor(() => expect(open).toHaveBeenCalledTimes(1));
  expect(open.mock.calls[0]).toEqual([{ item: picked.id, expectedRevision: "1", expectedSourceGeneration: "source-1", zoteroOnly: false, reveal: false }]);
  fireEvent.keyDown(window, { key: "Alt", altKey: true });
  fireEvent.click(footer().getByRole("button", { name: "More" }));
  expect(await screen.findByRole("menuitem", { name: "Show in folder" })).toBeTruthy();
  fireEvent.keyDown(screen.getByRole("menu"), { key: "o", ctrlKey: true, altKey: true });
  await waitFor(() => expect(open).toHaveBeenCalledTimes(2));
  expect(open.mock.calls[1]).toEqual([{ item: picked.id, expectedRevision: "1", expectedSourceGeneration: "source-1", zoteroOnly: false, reveal: true }]);
});

test("with no reference selected Command-O opens the current file source", async () => {
  const open = vi.fn(() => null);
  backend({ library_status: () => file, open_library_source: open, library_source: () => ({ kind: "file", name: "thesis.bib", icon: null }) });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("thesis.bib"));
  commandO();
  await waitFor(() => expect(open).toHaveBeenCalledOnce());
  expect(open.mock.calls[0]).toEqual([{ item: null, expectedRevision: "1", expectedSourceGeneration: "source-1", zoteroOnly: false, reveal: false }]);
});

test("several Zotero libraries follow the highlighted reference rather than the first library", async () => {
  const open = vi.fn(() => null);
  const group = result("GROUP001", "Group reference", "zotero:group:7");
  backend({ library_status: () => ({ ...file, zotero: true, snapshot_zotero: true, zotero_libraries: ["user", { group: 7 }] }),
    search: () => [result("USER0001", "Personal reference", "zotero:user"), group], open_library_source: open,
    library_source: () => ({ kind: "zotero", name: "Zotero", icon: null }), zotero_groups: () => [] });
  render(<App />);
  await screen.findByText("Group reference");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "ArrowDown" });
  commandO();
  await waitFor(() => expect(open).toHaveBeenCalledOnce());
  expect(open.mock.calls[0]).toEqual([{ item: group.id, expectedRevision: "1", expectedSourceGeneration: "source-1", zoteroOnly: false, reveal: false }]);
});

test("Shift-Command-O offers files and Zotero, while Command-O there opens the current source", async () => {
  const open = vi.fn(() => null);
  backend({ library_status: () => file, search: () => [result("a", "Invented file reference")], open_library_source: open });
  render(<App />);
  await screen.findByText("Invented file reference");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "o", ctrlKey: true, shiftKey: true });
  expect(await screen.findByRole("option", { name: /^Use Zotero/ })).toBeTruthy();
  expect(screen.getByRole("option", { name: /^Choose a Library File…/ })).toBeTruthy();
  commandO();
  await waitFor(() => expect(open).toHaveBeenCalledOnce());
  expect(open.mock.calls[0]).toEqual([{ item: null, expectedRevision: "1", expectedSourceGeneration: "source-1", zoteroOnly: false, reveal: false }]);
});

test("AltGr typing cannot trigger source opening", async () => {
  const open = vi.fn(() => null);
  backend({ library_status: () => file, search: () => [result("a", "Invented file reference")], open_library_source: open });
  render(<App />);
  await screen.findByText("Invented file reference");
  const event = new KeyboardEvent("keydown", { key: "ø", code: "KeyO", ctrlKey: true, altKey: true, bubbles: true });
  Object.defineProperty(event, "getModifierState", { value: (modifier: string) => modifier === "AltGraph" });
  fireEvent(screen.getByLabelText("Search references"), event);
  expect(open).not.toHaveBeenCalled();
});

test("hidden pickers do not read icons; reopening rejects an old icon completion", async () => {
  const pending: ((value: unknown) => void)[] = [];
  const read = vi.fn(() => new Promise((resolve) => { pending.push(resolve); }));
  backend({ library_status: () => file, library_source: read, search: () => [result("a", "Invented file reference")] });
  render(<App />);
  await screen.findByText("Invented file reference");
  await waitFor(() => expect(pending.length).toBeGreaterThan(0));
  await act(async () => { await emit("picker:hidden"); });
  const before = read.mock.calls.length;
  await act(async () => { await emit("library:changed", { ...file, note: "Invented hidden refresh" }); });
  expect(read.mock.calls).toHaveLength(before);
  const old = [...pending];
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(read.mock.calls.length).toBeGreaterThan(before));
  await act(async () => { pending.at(-1)?.({ kind: "file", name: "thesis.bib", icon }); });
  await act(async () => { old.forEach((finish) => finish({ kind: "file", name: "obsolete.bib", icon })); });
  fireEvent.click(footer().getByRole("button", { name: "More" }));
  expect(await screen.findByRole("menuitem", { name: "Open ⁨thesis.bib⁩" })).toBeTruthy();
  expect(screen.queryByRole("menuitem", { name: /obsolete/ })).toBeNull();
});

test("failed opening replaces the same footer action with Find, and Command-O uses that recovery", async () => {
  const choose = vi.fn(() => null);
  backend({ library_status: () => ({ ...file, state: "error", stale: true, problem: "unreadable", error: "The file could not be read" }),
    search: () => [result("picked", "Selected source reference")], library_source: () => ({ kind: "file", name: "thesis.bib", icon: null }),
    open_library_source: () => { throw new Error("invented private payload"); }, choose_library: choose });
  render(<App />);
  await screen.findByText("Selected source reference");
  const open = await screen.findByRole("button", { name: "Open ⁨thesis.bib⁩" });
  fireEvent.click(open);
  const find = await screen.findByRole("button", { name: "Find ⁨thesis.bib⁩…" });
  expect(find).toBe(open);
  expect(footer().getByRole("alert").textContent).toContain("could not open");
  commandO();
  await waitFor(() => expect(choose).toHaveBeenCalledOnce());
  expect(document.body.textContent).not.toContain("invented private payload");
});

test("a snapshot refusal refreshes the source without claiming that its file failed to open", async () => {
  let revision = "1";
  const calls = backend({ library_status: () => ({ ...file, revision, references: revision === "1" ? 12 : 14 }),
    search: () => [{ ...result("a", "Invented file reference"), revision }],
    open_library_source: () => { revision = "2"; throw { code: "snapshot_changed" }; } });
  render(<App />);
  await screen.findByText("Invented file reference");
  commandO();
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("14 references"));
  expect(calls.filter((call) => call === "library_status")).toHaveLength(2);
  expect(calls.filter((call) => call === "open_library_source")).toHaveLength(1);
  expect(screen.queryByRole("button", { name: /^Find/ })).toBeNull();
  expect(document.body.textContent).not.toContain("The source file could not open");
});

test("an unavailable source presentation uses its fallback without starting a refresh loop", async () => {
  const calls = backend({ library_status: () => file, search: () => [result("a", "Invented file reference")],
    library_source: () => { throw { code: "snapshot_changed" }; } });
  render(<App />);
  await screen.findByText("Invented file reference");
  await waitFor(() => expect(calls).toContain("library_source"));
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(calls.filter((call) => call === "library_status")).toHaveLength(1);
  expect(screen.queryByRole("alert")).toBeNull();
});

test("a delayed icon from source A cannot replace source B, and image failures retain the action", async () => {
  let finish!: (value: unknown) => void;
  let current = file;
  let first = true;
  backend({ library_status: () => current, search: () => [result("picked", "Selected source reference")],
    library_source: () => first ? (first = false, new Promise((resolve) => { finish = resolve; })) : { kind: "file", name: "second.bib", icon },
  });
  render(<App />);
  await waitFor(() => expect(finish).toBeTypeOf("function"));
  current = { ...file, name: "second.bib", snapshot_name: "second.bib" };
  await act(async () => { await emit("library:changed", current); });
  fireEvent.click(footer().getByRole("button", { name: "More" }));
  const source = await screen.findByRole("menuitem", { name: "Open ⁨second.bib⁩" });
  await act(async () => { finish({ kind: "file", name: "first.bib", icon }); });
  expect(source.textContent).not.toContain("first.bib");
  fireEvent.error(source.querySelector("img")!);
  expect(source.querySelector("img")).toBeNull();
  expect(source.textContent).toContain("second.bib");
});

test.each(["file", "zotero-running", "zotero-closed"])("empty choices show each goal once and Command-O keeps the current source (%s)", async (kind) => {
  const zotero = kind !== "file";
  const open = vi.fn(() => null);
  backend({ library_status: () => ({ ...file, references: 0, zotero, snapshot_zotero: zotero, name: zotero ? "Zotero" : "thesis.bib", snapshot_name: zotero ? "Zotero" : "thesis.bib",
    problem: kind === "zotero-closed" ? "zotero_closed" : null }), open_library_source: open,
    library_source: () => ({ kind: zotero ? "zotero" : "file", name: zotero ? "Zotero" : "thesis.bib", icon: null }) });
  render(<App />);
  await screen.findByRole("heading", { name: /^No references/ });
  const goals = screen.getAllByRole("option").map((node) => node.querySelector(".result-title")?.textContent);
  expect(goals).toEqual(["Use Zotero", "Choose a Library File…", ...kind === "file" ? ["Open ⁨thesis.bib⁩"] : kind === "zotero-closed" ? ["Open Zotero"] : []]);
  expect(footer().queryByRole("button", { name: /Open Zotero|Open .*thesis/ })).toBeNull();
  commandO();
  await waitFor(() => expect(open).toHaveBeenCalledExactlyOnceWith({ item: null, expectedRevision: "1", expectedSourceGeneration: "source-1", zoteroOnly: false, reveal: false }));
});

test("an empty file's failed open leaves one file-choice goal and keeps cancellation usable", async () => {
  const choose = vi.fn(() => null);
  const open = vi.fn(() => { throw new Error("invented private failure"); });
  backend({ library_status: () => ({ ...file, references: 0 }), open_library_source: open, choose_library: choose,
    library_source: () => ({ kind: "file", name: "thesis.bib", icon: null }) });
  render(<App />);
  fireEvent.click(await screen.findByRole("option", { name: /^Open .*thesis/ }));
  await screen.findByRole("alert");
  expect(screen.getAllByRole("option").map((node) => node.querySelector(".result-title")?.textContent))
    .toEqual(["Use Zotero", "Choose a Library File…"]);
  expect(document.body.textContent).not.toContain("invented private failure");
  fireEvent.click(screen.getByRole("option", { name: /^Choose a Library File/ }));
  await waitFor(() => expect(choose).toHaveBeenCalledOnce());
  expect(screen.getByRole("option", { name: /^Choose a Library File/ })).toBeTruthy();
  expect(open).toHaveBeenCalledOnce();
});

test("a source action shown in More hides the matching footer button until the menu closes", async () => {
  backend({ library_status: () => ({ ...file, state: "error", stale: true, problem: "unreadable", error: "Invented refresh failure" }),
    library_source: () => ({ kind: "file", name: "thesis.bib", icon: null }) });
  render(<App />);
  await screen.findByRole("button", { name: "Open ⁨thesis.bib⁩" });
  fireEvent.click(footer().getByRole("button", { name: "More" }));
  const row = await screen.findByRole("menuitem", { name: "Open ⁨thesis.bib⁩" });
  expect(document.querySelector("footer .source-action")).toBeNull();
  fireEvent.keyDown(row, { key: "Escape" });
  expect(await screen.findByRole("button", { name: "Open ⁨thesis.bib⁩" })).toBeTruthy();
});

test("itemless Command-O uses the missing-file Find recovery without attempting an open", async () => {
  const choose = vi.fn(() => null), open = vi.fn(() => null);
  backend({ library_status: () => ({ ...file, state: "error", stale: true, problem: "missing", error: "Invented file moved" }),
    choose_library: choose, open_library_source: open });
  render(<App />);
  const find = await screen.findByRole("button", { name: "Find ⁨thesis.bib⁩…" });
  expect(find.getAttribute("aria-keyshortcuts")).toBe("Control+O");
  commandO();
  await waitFor(() => expect(choose).toHaveBeenCalledOnce());
  expect(open).not.toHaveBeenCalled();
});

test.each([false, true])("Command-O finds a missing current file without stale references (snapshot: %s)", async (hasSnapshot) => {
  const choose = vi.fn(() => null), open = vi.fn(() => null);
  backend({ library_status: () => ({ ...file, state: "error", has_snapshot: hasSnapshot, references: 0,
    snapshot_name: hasSnapshot ? file.name : null, stale: false, problem: "missing", error: "Invented file moved" }),
    choose_library: choose, open_library_source: open });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("thesis.bib"));
  commandO();
  await waitFor(() => expect(choose).toHaveBeenCalledOnce());
  expect(open).not.toHaveBeenCalled();
  fireEvent.click(footer().getByRole("button", { name: "More" }));
  expect(await screen.findByRole("menuitem", { name: "Find ⁨thesis.bib⁩…" })).toBeTruthy();
});

test.each([0, 12])("Command-O opens the retained file after a same-named missing replacement fails (%s references)", async (references) => {
  const choose = vi.fn(() => null), open = vi.fn(() => null);
  backend({ library_status: () => ({ ...file, references, state: "error", stale: false, source_is_requested: false,
    problem: "missing", error: "Invented replacement file moved" }),
    library_source: () => ({ kind: "file", name: "thesis.bib", icon: null }), choose_library: choose, open_library_source: open });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("thesis.bib"));
  commandO();
  await waitFor(() => expect(open).toHaveBeenCalledExactlyOnceWith({ item: null, expectedRevision: "1", expectedSourceGeneration: "source-1", zoteroOnly: false, reveal: false }));
  expect(choose).not.toHaveBeenCalled();
});

test("More keeps a different footer recovery visible after a failed source switch", async () => {
  backend({ library_status: () => ({ ...file, state: "error", stale: false, name: "missing.bib", problem: "missing", error: "Invented replacement file moved" }),
    library_source: () => ({ kind: "file", name: "thesis.bib", icon: null }) });
  render(<App />);
  await screen.findByRole("button", { name: "Choose Library…" });
  fireEvent.click(footer().getByRole("button", { name: "More" }));
  await screen.findByRole("menuitem", { name: "Open ⁨thesis.bib⁩" });
  expect(document.querySelector("footer .source-action")?.textContent).toContain("Choose Library…");
});
