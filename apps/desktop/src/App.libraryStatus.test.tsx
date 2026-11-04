import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { getRoles } from "@testing-library/dom";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { LibraryStatus } from "./api";
import { backend, resetApp, result } from "./testing/app";

afterEach(resetApp);
const hour = 3_600_000;
const library = (overrides: Partial<LibraryStatus> = {}): LibraryStatus => ({
  source_generation: "1", source_is_requested: true, revision: "1", state: "loaded", has_snapshot: true, zotero: true, snapshot_zotero: true,
  zotero_libraries: ["user"], name: "Zotero library", snapshot_name: "Zotero library",
  references: 12, source_counts: { "zotero:user": 12 }, warnings: 0, error: null, problem: null,
  note: null, settings_notice: null, stale: false, checked_at: Date.now() - 2 * hour, updated_at: null, ...overrides,
});
const stale = (overrides: Partial<LibraryStatus> = {}) => library({ state: "error", stale: true,
  error: "The library could not be refreshed; keeping the last loaded references.", problem: "zotero_closed", ...overrides });
const status = () => screen.getByRole("status", { name: "Library status" });
const footer = () => document.querySelector("footer")!;
const openKey = () => fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "o", ctrlKey: true });

test.each([
  ["loaded", {}, "thesis.bib · 12 references"],
  ["missing with retained references", { state: "error", stale: true, problem: "missing", error: "the library file is unavailable; keeping the last loaded references" }, "thesis.bib moved"],
  ["empty after recovery", { references: 0 }, "thesis.bib · no references"],
] satisfies [string, Partial<LibraryStatus>, string][])("%s: Library uniquely names a non-empty menu button like More", async (_, overrides, summary) => {
  backend({ library_status: () => library({ zotero: false, snapshot_zotero: false, name: "thesis.bib", snapshot_name: "thesis.bib", ...overrides }),
    reload_library: () => null });
  render(<App />);
  await waitFor(() => expect(status().textContent).toBe(summary));
  const button = screen.getByRole("button", { name: "Library" });
  const more = screen.getByRole("button", { name: "More" });
  const named = Object.keys(getRoles(document.body)).flatMap((role) => screen.queryAllByRole(role, { name: "Library" }));
  expect([...new Set(named)]).toEqual([button]);
  expect(button.firstElementChild?.tagName).toBe(more.firstElementChild?.tagName);
  expect(button.firstElementChild?.hasAttribute("aria-hidden")).toBe(false);
  expect(button.firstElementChild?.textContent).toContain(summary);
  for (const attribute of ["type", "aria-haspopup", "aria-expanded", "tabindex", "data-variant", "data-size", "data-react-aria-pressable"]) {
    expect(button.getAttribute(attribute)).toBe(more.getAttribute(attribute));
  }
  expect(button.closest('[aria-hidden="true"], [hidden], [inert], [role="presentation"], [role="none"]')).toBeNull();
  for (let node: HTMLElement | null = button; node; node = node.parentElement) {
    expect(getComputedStyle(node).display).not.toBe("none");
    expect(getComputedStyle(node).visibility).not.toBe("hidden");
  }
  expect(button.contains(status())).toBe(false);
  expect(screen.getAllByRole("status", { name: "Library status" })).toHaveLength(1);
  act(() => button.click());
  expect(within(await screen.findByRole("menu", { name: "Library" })).getByRole("menuitem", { name: "Reload Library" })).toBeTruthy();
});

test("a fresh closed Zotero snapshot stays quiet and the footer opens its library menu", async () => {
  const calls = backend({ library_status: () => library(), reload_library: () => null });
  render(<App />);
  await waitFor(() => expect(status().textContent).toBe("Zotero · 12 references"));
  expect(document.querySelector(".notices")?.textContent).toBe("");
  fireEvent.click(within(footer()).getByRole("button", { name: "Library" }));
  const menu = await screen.findByRole("menu", { name: "Library" });
  // The spec's order: the library with its count and age, a separator, then Choose Library… ⇧⌘O and Reload Library.
  const heading = menu.querySelector(".menu-detail")!;
  expect(heading.textContent).toMatch(/^\u2068?Zotero\u2069? · 12 references.*checked 2 hours ago/);
  expect(heading.closest("section")?.nextElementSibling?.getAttribute("role")).toBe("separator");
  expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Choose Library…Ctrl+Shift+O", "Reload Library"]);
  // A keyboard user entering the actions hears what the menu is about.
  const actions = within(menu).getAllByRole("group").find((group) => group.contains(within(menu).getByRole("menuitem", { name: "Reload Library" })))!;
  expect(actions.getAttribute("aria-label")).toMatch(/^\u2068Zotero\u2069 · 12 references\. .*checked 2 hours ago/);
  fireEvent.click(within(menu).getByRole("menuitem", { name: "Reload Library" }));
  await waitFor(() => expect(calls).toContain("reload_library"));
});

test.each(["virtual", "Enter", " "])("%s activation reloads missing, empty and restored libraries after retiring the Library menu", async (activation) => {
  const file = library({ zotero: false, snapshot_zotero: false, name: "thesis.bib", snapshot_name: "thesis.bib", references: 4 });
  let current = file;
  const replies = [
    { ...file, state: "error" as const, stale: true, problem: "missing" as const,
      error: "the library file is unavailable; keeping the last loaded references" },
    { ...file, references: 0 },
    { ...file, references: 3 },
  ];
  const menusAtDispatch: boolean[] = [];
  const choose = vi.fn(() => null);
  backend({ library_status: () => current, choose_library: choose, reload_library: async () => {
    menusAtDispatch.push(document.getElementById("library-menu") !== null);
    current = replies.shift()!;
    await emit("library:changed", current);
  } });
  render(<App />);
  await waitFor(() => expect(status().textContent).toContain("4 references"));
  const libraryButton = within(footer()).getByRole("button", { name: "Library" });
  let finishExit!: () => void;
  const finished = new Promise<void>((resolve) => { finishExit = resolve; });
  try {
    for (const [index, expected] of ["thesis.bib moved", "thesis.bib · no references", "thesis.bib · 3 references"].entries()) {
      // Native name lookup must find the opener, never a same-named footer
      // region whose accessibility action can hit the neighbouring Find button.
      expect(screen.getAllByLabelText("Library")).toEqual([libraryButton]);
      expect(libraryButton.tagName).toBe("BUTTON");
      expect(libraryButton.getAttribute("type")).toBe("button");
      expect(libraryButton.tabIndex).toBe(0);
      expect(libraryButton.hasAttribute("disabled")).toBe(false);
      expect(typeof libraryButton.onclick).toBe("function");
      expect(libraryButton.getAttribute("aria-haspopup")).toBe("true");
      act(() => libraryButton.focus());
      if (activation === "virtual") act(() => libraryButton.click());
      else {
        fireEvent.keyDown(libraryButton, { key: activation });
        fireEvent.keyUp(libraryButton, { key: activation });
      }
      const menu = await screen.findByRole("menu", { name: "Library" });
      const popover = menu.closest(".menu-popover")!;
      // Model a native exit still in progress when the next recovery step runs.
      // Its modal scope and same-named menu must be gone before the reload I/O.
      Object.defineProperty(popover, "getAnimations", { value: () => popover.hasAttribute("data-exiting")
        ? [{ playState: "running", finished }] : [] });
      fireEvent.click(within(menu).getByRole("menuitem", { name: "Reload Library" }));
      await waitFor(() => expect(status().textContent).toBe(expected));
      expect(menusAtDispatch).toEqual(Array(index + 1).fill(false));
      expect(popover.isConnected).toBe(false);
      expect(within(footer()).getByRole("button", { name: "Library" }).getAttribute("aria-expanded")).toBe("false");
      expect(choose).not.toHaveBeenCalled();
    }
  } finally {
    await act(async () => { finishExit(); });
  }
});

test.each([
  ["empty library", { references: 0 }, "thesis.bib · no references"],
  ["unreadable without a snapshot", { state: "error", has_snapshot: false, references: 0, problem: "unreadable", error: "The library file could not be read" }, "thesis.bib · not opened"],
  ["damaged file with usable references", { warnings: 1 }, "thesis.bib · 12 references · 1 reading notice"],
] satisfies [string, Partial<LibraryStatus>, string][])("Library remains a named footer button with Reload in %s", async (_, overrides, expected) => {
  const reload = vi.fn(() => null);
  const choose = vi.fn(() => null);
  backend({ library_status: () => library({ zotero: false, snapshot_zotero: false, name: "thesis.bib", snapshot_name: "thesis.bib", ...overrides }),
    reload_library: reload, choose_library: choose });
  render(<App />);
  await waitFor(() => expect(status().textContent).toBe(expected));
  const opener = screen.getByLabelText("Library");
  expect(opener.tagName).toBe("BUTTON");
  expect(footer().contains(opener)).toBe(true);
  fireEvent.click(opener);
  const menu = await screen.findByRole("menu", { name: "Library" });
  fireEvent.click(within(menu).getByRole("menuitem", { name: "Reload Library" }));
  await waitFor(() => expect(reload).toHaveBeenCalledOnce());
  expect(choose).not.toHaveBeenCalled();
});

test.each([
  ["an older copy kept after a failed refresh", stale()],
  ["a failed switch that keeps the previous library", library({ state: "error", zotero: false, name: "missing.bib", error: "the library file is unavailable" })],
])("%s still says when a search finds nothing there, and recovers on the next answer", async (_, kept) => {
  let current = kept;
  backend({ library_status: () => current, search: ({ query }) => query.text === "invented" ? [{ ...result("a", "Invented reference"), revision: current.revision }] : [] });
  render(<App />);
  const box = await screen.findByLabelText("Search references");
  fireEvent.change(box, { target: { value: "nothing here" } });
  await screen.findByText("No matching references");
  expect(screen.getByText("Try a surname, year or fewer words.")).toBeTruthy();
  // Its fix, beside the search field's own clear button.
  expect(screen.getAllByRole("button", { name: "Clear Search" })).toHaveLength(2);
  // The newer answer replaces it, and a later read keeps the screen honest.
  fireEvent.change(box, { target: { value: "invented" } });
  await screen.findByText("Invented reference");
  expect(screen.queryByText("No matching references")).toBeNull();
  current = library({ revision: "2", checked_at: Date.now() });
  await act(async () => { await emit("library:changed", current); });
  fireEvent.change(box, { target: { value: "nothing again" } });
  await screen.findByText("No matching references");
});

test("a stale snapshot has one footer problem and action, and a successful read clears it", async () => {
  const opened = vi.fn(() => null);
  let current = stale();
  backend({ library_status: () => current, open_library_source: opened, search: () => [{ ...result("a", "Invented reference"), revision: current.revision }] });
  render(<App />);
  await screen.findByText("Invented reference");
  expect(status().textContent).toBe("Zotero not updated");
  expect(status().closest("button")).toBeNull();
  expect(footer().textContent).toContain("2 h");
  // The older copy is still searched: a warning, never the stop of nothing to search.
  expect(document.querySelector(".status-dot")?.getAttribute("data-health")).toBe("look");
  expect(document.querySelector(".notices")?.textContent).toBe("");
  // The fix shows its key, as More shows ⌘K, and does what its words say: open Zotero, not the row.
  const fix = within(footer()).getByRole("button", { name: "Open Zotero" });
  expect(fix.getAttribute("aria-keyshortcuts")).toBe("Control+O");
  fireEvent.click(fix);
  await waitFor(() => expect(opened).toHaveBeenCalledOnce());
  expect(opened.mock.calls[0]).toEqual([{ item: null, expectedRevision: "1", expectedSourceGeneration: "1", zoteroOnly: true, reveal: false, presentation: 0 }]);
  openKey();
  await waitFor(() => expect(opened).toHaveBeenCalledTimes(2));
  expect(opened.mock.calls[1]).toEqual(opened.mock.calls[0]);
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
  expect(status().textContent).toBe("thesis.bib moved");
  expect(document.querySelector(".notices")?.textContent).toBe("");
  fireEvent.click(within(footer()).getByRole("button", { name: "Find ⁨thesis.bib⁩…" }));
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

test("a refused switch keeps the file searchable and the engine's recovery instructions in the footer detail", async () => {
  const reason = 'Zotero refused local access. In Zotero Settings > Advanced, enable "Allow other applications on this computer to communicate with Zotero"; keeping the last loaded references';
  const kept = library({ zotero: false, snapshot_zotero: false, name: "thesis.bib", snapshot_name: "thesis.bib" });
  backend({ library_status: () => kept, search: () => [result("a", "Retained file reference")],
    use_zotero: async () => { await emit("library:changed", { ...kept, state: "error", zotero: true, name: "Zotero library", error: reason, problem: "zotero_access_off" }); return null; } });
  render(<App />);
  await screen.findByText("Retained file reference");
  fireEvent.click(within(footer()).getByRole("button", { name: "Library" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Use Zotero" }));
  await waitFor(() => expect(status().textContent).toContain("Zotero⁩ not opened"));
  expect(await screen.findByText("Retained file reference")).toBeTruthy();
  const tooltip = within(footer()).getByRole("button", { name: "Library" }).getAttribute("title");
  expect(tooltip).toContain(reason);
  expect(tooltip).toContain("Settings › Advanced");
  expect(tooltip).not.toContain("Open Zotero");
  expect(footer().querySelector(".library-detail")?.textContent).toContain(reason);
  expect(document.querySelector(".notices")?.textContent).toBe("");
  expect(within(footer()).getByRole("button", { name: "Choose Library…" })).toBeTruthy();
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
  expect(opened.mock.calls[0]).toEqual([{ item: picked.id, expectedRevision: "1", expectedSourceGeneration: "1", zoteroOnly: false, reveal: false, presentation: 0 }]);
  fireEvent.click(screen.getByRole("button", { name: "More" }));
  const menu = await screen.findByRole("menu");
  expect(within(menu).getAllByRole("menuitem")[0]?.textContent).toContain("Show in Zotero");
});

test("Command-O works on a recovery screen and a missing source refuses with a visible outcome", async () => {
  const opened = vi.fn(() => { throw { code: "failed" }; });
  backend({ library_status: () => library({ state: "error", has_snapshot: false, problem: "zotero_closed", error: "Zotero is closed" }), open_library_source: opened });
  render(<App />);
  await screen.findByText("Open Zotero to read your library");
  const sourceRow = screen.getByRole("option", { name: /^Open Zotero/ });
  expect(sourceRow.textContent).toContain("Ctrl+O");
  expect(status().getAttribute("aria-live")).toBe("off");
  openKey();
  await waitFor(() => expect(opened).toHaveBeenCalledOnce());
  expect((await screen.findAllByRole("alert")).some((node) => node.textContent === "Zotero could not open. Try again.")).toBe(true);
});

test("Command-O opens a file from every picker overlay, without passing a file path", async () => {
  const opened = vi.fn(() => null);
  backend({ library_status: () => library({ zotero: false, snapshot_zotero: false, name: "مراجع.bib", snapshot_name: "مراجع.bib" }),
    open_library_source: opened, search: () => [result("a", "Invented file reference")] });
  render(<App />);
  await screen.findByText("Invented file reference");
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
  expect(opened.mock.calls).toEqual(Array(2).fill([{ item: result("a", "Invented file reference").id, expectedRevision: "1", expectedSourceGeneration: "1", zoteroOnly: false, reveal: false, presentation: 0 }]));
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
  for (const modifiers of [{ repeat: true }, { isComposing: true }]) {
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

test("kept references stay on screen while a profile is chosen from the library menu", async () => {
  const chosen = vi.fn(() => null);
  backend({
    library_status: () => stale({ problem: "zotero_profiles", error: "choose which Zotero profile to read; keeping the last loaded references" }),
    zotero_profiles: () => ({ names: ["Home", "Work"], chosen: "Home", running: false }), use_zotero_profile: chosen,
  });
  render(<App />);
  fireEvent.click(await within(footer()).findByRole("button", { name: "Choose Zotero Profile…" }));
  const menu = await screen.findByRole("menu", { name: "Library" });
  expect(within(menu).getByRole("menuitemradio", { name: "Home" }).getAttribute("aria-checked")).toBe("true");
  fireEvent.click(within(menu).getByRole("menuitemradio", { name: "Work" }));
  await waitFor(() => expect(chosen).toHaveBeenCalledOnce());
  expect(chosen.mock.calls[0]).toEqual([{ name: "Work" }]);
  expect(screen.queryByText("Which Zotero profile?")).toBeNull();
});

test.each([
  ["one profile", { names: ["Home"], chosen: null, running: false }],
  ["a running Zotero", { names: ["Home", "Work"], chosen: "Home", running: true }],
])("%s offers no profile choice", async (_, profiles) => {
  backend({ library_status: () => library(), zotero_profiles: () => profiles });
  render(<App />);
  fireEvent.click(await within(footer()).findByRole("button", { name: "Library" }));
  const menu = await screen.findByRole("menu", { name: "Library" });
  await act(async () => { await Promise.resolve(); });
  expect(within(menu).queryByRole("menuitemradio", { name: "Work" })).toBeNull();
  expect(within(menu).queryByRole("menuitemradio", { name: "Home" })).toBeNull();
});

test("a list that cannot be read leaves a healthy library alone and is said when it was asked for", async () => {
  backend({ library_status: () => library(), zotero_groups: () => [], zotero_profiles: () => { throw new Error("synthetic"); } });
  const healthy = render(<App />);
  fireEvent.click(await within(footer()).findByRole("button", { name: "Library" }));
  await screen.findByRole("menu", { name: "Library" });
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByText(/could not list your Zotero profiles/)).toBeNull();
  expect(document.querySelector(".status-dot")?.getAttribute("data-health")).toBe("done");
  healthy.unmount();
  backend({ library_status: () => stale({ problem: "zotero_profiles", error: "choose which Zotero profile to read" }), zotero_profiles: () => { throw new Error("synthetic"); } });
  render(<App />);
  fireEvent.click(await within(footer()).findByRole("button", { name: "Choose Zotero Profile…" }));
  expect(await screen.findAllByText(/could not list your Zotero profiles/)).toBeTruthy();
});

test("one profile left is still a way forward when a choice was asked for", async () => {
  const chosen = vi.fn(() => null);
  backend({ library_status: () => stale({ problem: "zotero_profiles", error: "choose which Zotero profile to read" }),
    zotero_profiles: () => ({ names: ["Home"], chosen: "Gone", running: false }), use_zotero_profile: chosen });
  render(<App />);
  fireEvent.click(await within(footer()).findByRole("button", { name: "Choose Zotero Profile…" }));
  fireEvent.click(await screen.findByRole("menuitemradio", { name: "Home" }));
  await waitFor(() => expect(chosen).toHaveBeenCalledOnce());
});

test("Zotero recovering by itself clears the failure that came before it", async () => {
  backend({ library_status: () => library({ state: "error", has_snapshot: false, problem: "zotero_closed", error: "closed" }),
    use_zotero: () => { throw new Error("synthetic"); } });
  render(<App />);
  await screen.findByText("Open Zotero to read your library");
  await act(async () => { await emit("library:changed", library()); });
  await waitFor(() => expect(status().textContent).toBe("Zotero · 12 references"));
  expect(document.querySelector(".status-dot")?.getAttribute("data-health")).toBe("done");
});
