import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { LibraryStatus } from "./api";
import { backend, resetApp, result } from "./testing/app";

afterEach(resetApp);
const hour = 3_600_000;
const library = (overrides: Partial<LibraryStatus> = {}): LibraryStatus => ({
  revision: "1", state: "loaded", has_snapshot: true, zotero: true, snapshot_zotero: true,
  zotero_libraries: ["user"], name: "Zotero library", snapshot_name: "Zotero library",
  references: 12, source_counts: { "zotero:user": 12 }, warnings: 0, error: null, problem: null,
  note: null, settings_notice: null, stale: false, checked_at: Date.now() - 2 * hour, updated_at: null, ...overrides,
});
const stale = (overrides: Partial<LibraryStatus> = {}) => library({ state: "error", stale: true,
  error: "The library could not be refreshed; keeping the last loaded references.", problem: "zotero_closed", ...overrides });
const status = () => screen.getByRole("status", { name: "Library status" });
const footer = () => document.querySelector("footer")!;
const openKey = () => fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "o", ctrlKey: true });

test("a fresh closed Zotero snapshot stays quiet and the footer opens its library menu", async () => {
  const calls = backend({ library_status: () => library(), reload_library: () => null });
  render(<App />);
  await waitFor(() => expect(status().textContent).toBe("Zotero · 12 references"));
  expect(document.querySelector(".notices")?.textContent).toBe("");
  fireEvent.click(within(footer()).getByRole("button", { name: "Library" }));
  const menu = await screen.findByRole("menu", { name: "Library" });
  expect(within(menu).getByRole("menuitem", { name: "Choose Library…" })).toBeTruthy();
  fireEvent.click(within(menu).getByRole("menuitem", { name: "Reload Library" }));
  await waitFor(() => expect(calls).toContain("reload_library"));
});

test("a stale snapshot has one footer problem and action, and a successful read clears it", async () => {
  const opened = vi.fn(() => null);
  let current = stale();
  backend({ library_status: () => current, open_library_source: opened, search: () => [{ ...result("a", "Invented reference"), revision: current.revision }] });
  render(<App />);
  await screen.findByText("Invented reference");
  expect(status().textContent).toBe("Zotero · not updated");
  expect(status().closest("button")).toBeNull();
  expect(footer().textContent).toContain("last read 2 hours ago");
  expect(document.querySelector(".status-dot")?.getAttribute("data-health")).toBe("stop");
  expect(document.querySelector(".notices")?.textContent).toBe("");
  fireEvent.click(within(footer()).getByRole("button", { name: "Open Zotero" }));
  await waitFor(() => expect(opened).toHaveBeenCalledOnce());
  expect(opened.mock.calls[0]).toEqual([{ item: null, expectedRevision: "1", zoteroOnly: true }]);
  expect(status().textContent).toContain("not updated");
  current = library({ revision: "2", checked_at: Date.now() });
  await act(async () => { await emit("library:changed", current); });
  expect(status().textContent).toBe("Zotero · 12 references");
  expect(screen.getAllByRole("status", { name: "Library status" })).toHaveLength(1);
  expect(within(footer()).getByRole("button", { name: "Library" })).toBeTruthy();
  expect(within(footer()).queryByRole("button", { name: "Open Zotero" })).toBeNull();
});

test("a moved file keeps its usable references and offers Find once in the footer", async () => {
  const choose = vi.fn(() => null);
  backend({ library_status: () => stale({ zotero: false, snapshot_zotero: false, name: "thesis.bib", snapshot_name: "thesis.bib", problem: "missing" }),
    search: () => [result("a", "Retained reference")], choose_library: choose });
  render(<App />);
  await screen.findByText("Retained reference");
  expect(status().textContent).toBe("thesis.bib · moved");
  expect(document.querySelector(".notices")?.textContent).toBe("");
  fireEvent.click(within(footer()).getByRole("button", { name: "Find…" }));
  await waitFor(() => expect(choose).toHaveBeenCalledOnce());
});

test("the library menu preserves switching from a file to Zotero", async () => {
  const calls = backend({ library_status: () => library({ zotero: false, snapshot_zotero: false,
    name: "thesis.bib", snapshot_name: "thesis.bib" }), use_zotero: () => null });
  render(<App />);
  await waitFor(() => expect(status().textContent).toContain("thesis.bib"));
  fireEvent.click(within(footer()).getByRole("button", { name: "Library" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Use Zotero" }));
  await waitFor(() => expect(calls).toContain("use_zotero"));
  expect(screen.queryByRole("menu")).toBeNull();
});

test("a failed source open says so beside its footer action without exposing native details", async () => {
  backend({ library_status: () => stale(), open_library_source: () => { throw new Error("invented private path"); } });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "Open Zotero" }));
  const failure = await screen.findByRole("alert");
  expect(footer().contains(failure)).toBe(true);
  expect(failure.textContent).toBe("Zotero could not open. Try again.");
  expect(document.body.textContent).not.toContain("invented private path");
});

test("Command-O opens the searched source and More lists it first", async () => {
  const opened = vi.fn(() => null);
  const picked = result("a", "Selected invented reference", "zotero:user");
  backend({ library_status: () => library(), search: () => [picked], open_library_source: opened });
  render(<App />);
  await screen.findByText("Selected invented reference");
  openKey();
  await waitFor(() => expect(opened).toHaveBeenCalledOnce());
  expect(opened.mock.calls[0]).toEqual([{ item: picked.id, expectedRevision: "1", zoteroOnly: false }]);
  fireEvent.click(screen.getByRole("button", { name: "More" }));
  const menu = await screen.findByRole("menu");
  expect(within(menu).getAllByRole("menuitem")[0]?.textContent).toContain("Show in Zotero");
});

test("Command-O works on a recovery screen and a missing source refuses with a visible outcome", async () => {
  const opened = vi.fn(() => { throw { code: "failed" }; });
  backend({ library_status: () => library({ state: "error", has_snapshot: false, problem: "zotero_closed", error: "Zotero is closed" }), open_library_source: opened });
  render(<App />);
  await screen.findByText("Zotero isn't open");
  const sourceRow = screen.getByRole("option", { name: "Show in Zotero" });
  expect(sourceRow.textContent).toContain("Ctrl+O");
  expect(status().getAttribute("aria-live")).toBe("off");
  openKey();
  await waitFor(() => expect(opened).toHaveBeenCalledOnce());
  expect((await screen.findAllByRole("alert")).some((node) => node.textContent === "Zotero could not open. Try again.")).toBe(true);
});

test("Command-O opens a file from every picker overlay, without passing a file path", async () => {
  const opened = vi.fn(() => null);
  backend({ library_status: () => library({ zotero: false, snapshot_zotero: false, name: "مراجع.bib", snapshot_name: "مراجع.bib" }),
    open_library_source: opened });
  render(<App />);
  await waitFor(() => expect(status().textContent).toContain("مراجع.bib"));
  fireEvent.click(screen.getByRole("button", { name: "More" }));
  const menu = await screen.findByRole("menu");
  expect(within(menu).getAllByRole("menuitem")[0]?.textContent).toBe("Open ⁨مراجع.bib⁩Ctrl+O");
  fireEvent.keyDown(menu, { key: "o", ctrlKey: true });
  await waitFor(() => expect(opened).toHaveBeenCalledTimes(1));
  expect(screen.queryByRole("menu")).toBeNull();
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "/", ctrlKey: true });
  fireEvent.keyDown(await screen.findByRole("dialog"), { key: "o", ctrlKey: true });
  await waitFor(() => expect(opened).toHaveBeenCalledTimes(2));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(opened.mock.calls).toEqual(Array(2).fill([{ item: null, expectedRevision: "1", zoteroOnly: false }]));
});

test("an older failed open cannot overwrite a later successful open or a reopened picker", async () => {
  const rejected: ((reason: unknown) => void)[] = [];
  let pending = true;
  backend({ library_status: () => library(), open_library_source: () => pending
    ? new Promise((_, reject) => { rejected.push(reject); }) : null });
  render(<App />);
  await waitFor(() => expect(status().textContent).toContain("12 references"));
  openKey();
  await waitFor(() => expect(rejected).toHaveLength(1));
  pending = false;
  openKey();
  await act(async () => { rejected[0]?.(new Error("invented obsolete open")); });
  expect(screen.queryByRole("alert")).toBeNull();
  pending = true;
  openKey();
  await waitFor(() => expect(rejected).toHaveLength(2));
  await act(async () => { await emit("picker:hidden"); });
  await act(async () => { await emit("picker:shown"); });
  await act(async () => { rejected[1]?.(new Error("invented retired presentation")); });
  expect(screen.queryByRole("alert")).toBeNull();
});

test("source opening waits for composition and ignores repeated or differently modified keys", async () => {
  const opened = vi.fn(() => null);
  backend({ library_status: () => library(), open_library_source: opened });
  render(<App />);
  await waitFor(() => expect(status().textContent).toContain("12 references"));
  const box = screen.getByLabelText("Search references");
  for (const modifiers of [{ repeat: true }, { shiftKey: true }, { altKey: true }, { isComposing: true }]) {
    fireEvent.keyDown(box, { key: "o", ctrlKey: true, ...modifiers });
  }
  expect(opened).not.toHaveBeenCalled();
  openKey();
  await waitFor(() => expect(opened).toHaveBeenCalledOnce());
});

test("a delayed status A cannot replace event B, including across hide and reopen", async () => {
  let finish!: (value: LibraryStatus) => void;
  const read = vi.fn().mockImplementationOnce(() => new Promise<LibraryStatus>((resolve) => { finish = resolve; }))
    .mockResolvedValue(library({ references: 27, revision: "B" }));
  backend({ library_status: read, search: () => ({ revision: "B", results: [] }) });
  render(<App />);
  await waitFor(() => expect(read).toHaveBeenCalledOnce());
  await act(async () => { await emit("library:changed", library({ references: 26, revision: "B" })); });
  expect(status().textContent).toContain("26 references");
  await act(async () => { await emit("picker:hidden"); });
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(status().textContent).toContain("27 references"));
  await act(async () => finish(stale({ references: 12, revision: "A" })));
  expect(status().textContent).toContain("27 references");
  expect(within(footer()).queryByRole("button", { name: "Open Zotero" })).toBeNull();
});
