import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { focusLost } from "./ui/layers";
import { backend, chooseFromMore, openCitationDetails, resetApp, result, paletteRow } from "./testing/app";

// What a person sees of the layer stack: Esc from anywhere, a key and its menu
// item doing the same thing, and nothing surviving a close and reopen.
afterEach(resetApp);

const library = { revision: "1", state: "loaded", has_snapshot: true, zotero: false, snapshot_zotero: false,
  name: "thesis.bib", snapshot_name: "thesis.bib", references: 12, warnings: 0, error: null, problem: null,
  stale: false, zotero_libraries: [], source_counts: { "synthetic-file": 12 }, checked_at: null, updated_at: null };
const welcome = { ...library, state: "none", has_snapshot: false, name: null, snapshot_name: null, references: 0, source_counts: {} };
const zotero = { ...library, zotero: true, snapshot_zotero: true, zotero_libraries: ["user"], name: "Zotero library", snapshot_name: "Zotero library" };
const search = () => screen.getByLabelText("Search references") as HTMLInputElement;
const press = (key: string, modifiers = {}) => fireEvent.keyDown(document.activeElement ?? document.body, { key, ...modifiers });
const hides = (calls: string[]) => calls.filter((call) => call === "hide_picker").length;
const reopen = () => act(async () => {
  await emit("picker:hidden");
  await emit("picker:shown", 43);
});
const type = (text: string) => fireEvent.change(search(), { target: { value: text } });
const focusableControls = () => [...document.querySelectorAll<HTMLElement>("main button, main input, main [tabindex='0']")]
  .filter((node) => !node.closest("[hidden], [inert], [aria-hidden='true']") && !node.matches(":disabled") && node.tabIndex >= 0);

async function chosenCitation(overrides = {}) {
  const calls = backend({ library_status: () => library, search: () => [result("picked", "Invented reference")], ...overrides });
  render(<App />);
  await screen.findByText("Invented reference");
  act(() => search().focus());
  press("Enter", { shiftKey: true });
  const chip = await screen.findByRole("button", { name: /^Edit/ });
  await screen.findByRole("button", { name: /^Remove/ });
  return { calls, chip };
}

test("Esc from a focused citation chip steps back, with no pointer click needed", async () => {
  const { calls, chip } = await chosenCitation();
  type("invented");
  act(() => chip.focus());
  press("Escape");
  expect(search().value).toBe("");
  expect(document.activeElement).toBe(search());
  expect(hides(calls)).toBe(0);
  act(() => screen.getByRole("button", { name: /^Edit/ }).focus());
  press("Escape");
  await waitFor(() => expect(hides(calls)).toBe(1));
  expect(screen.queryByRole("button", { name: /^Edit/ })).toBeTruthy();
});

test("Esc works from every focusable control in the picker", async () => {
  const { calls } = await chosenCitation();
  type("invented");
  await screen.findByRole("button", { name: /^Clear search/i });
  const count = focusableControls().length;
  expect(count).toBeGreaterThan(5);
  for (let index = 0; index < count; index += 1) {
    type("invented");
    await screen.findByRole("button", { name: /^Clear search/i });
    const control = focusableControls()[index]!;
    const name = control.getAttribute("aria-label") ?? control.id;
    act(() => control.focus());
    press("Escape");
    expect(search().value, name).toBe("");
    expect(document.activeElement, name).toBe(search());
  }
  expect(hides(calls)).toBe(0);
});

test.each(["empty search", "choose library"])("every control of the %s screen hides or steps back on Esc", async (screenName) => {
  const calls = backend({ library_status: () => library });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("thesis.bib"));
  act(() => search().focus());
  if (screenName === "choose library") {
    press("o", { ctrlKey: true, shiftKey: true });
    await screen.findByRole("option", { name: /^Use Zotero/ });
  }
  const before = hides(calls);
  const control = focusableControls().at(-1)!;
  act(() => control.focus());
  press("Escape");
  if (screenName === "choose library") {
    expect(screen.queryByRole("option", { name: /^Use Zotero/ })).toBeNull();
    expect(hides(calls)).toBe(before);
  } else await waitFor(() => expect(hides(calls)).toBe(before + 1));
});

test("Esc walks the stack one layer at a time: palette, search, window", async () => {
  const { calls } = await chosenCitation();
  type("invented");
  await chooseFromMore("Keep Window Open");
  const more = screen.getByRole("button", { name: /^More/ });
  act(() => more.focus());
  fireEvent.click(more);
  await screen.findByRole("dialog", { name: "Actions" });
  press("Escape");
  expect(screen.queryByRole("dialog", { name: "Actions" })).toBeNull();
  expect(document.activeElement).toBe(more);
  expect(search().value).toBe("invented");
  press("Escape");
  expect(search().value).toBe("");
  expect(hides(calls)).toBe(0);
  press("Escape");
  await waitFor(() => expect(hides(calls)).toBe(1));
});

test("Esc closes the inspector back to its chip, and Search keeps focus if it was there", async () => {
  const { calls, chip } = await chosenCitation();
  act(() => chip.focus());
  fireEvent.click(chip);
  const page = await screen.findByRole("textbox", { name: "Page" });
  await waitFor(() => expect(document.activeElement).toBe(page));
  press("Escape");
  expect(screen.queryByRole("textbox", { name: "Page" })).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole("button", { name: /^Edit/ }));
  fireEvent.click(screen.getByRole("button", { name: /^Edit/ }));
  await screen.findByRole("textbox", { name: "Page" });
  act(() => search().focus());
  press("Escape");
  expect(screen.queryByRole("textbox", { name: "Page" })).toBeNull();
  expect(document.activeElement).toBe(search());
  expect(hides(calls)).toBe(0);
});

test("Esc steps out of the inspector's menu, then the inspector, then the search", async () => {
  const { calls, chip } = await chosenCitation();
  type("invented");
  fireEvent.click(chip);
  await screen.findByRole("textbox", { name: "Page" });
  const form = await screen.findByRole("button", { name: /^Show as/ });
  fireEvent.click(form);
  await screen.findByRole("menu");
  press("Escape");
  expect(screen.queryByRole("menu")).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole("button", { name: /^Show as/ }));
  press("Escape");
  expect(screen.queryByRole("textbox", { name: "Page" })).toBeNull();
  expect(search().value).toBe("invented");
  press("Escape");
  expect(search().value).toBe("");
  expect(hides(calls)).toBe(0);
});

test("the chooser comes after the sheet, and closing it leaves Search in charge", async () => {
  const { calls } = await chosenCitation();
  type("invented");
  press("o", { ctrlKey: true, shiftKey: true });
  await screen.findByRole("option", { name: /^Use Zotero/ });
  act(() => screen.getByRole("option", { name: /^Use Zotero/ }).focus());
  press("/", { ctrlKey: true });
  const sheet = await screen.findByRole("dialog");
  press("Escape");
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(sheet.isConnected).toBe(false);
  expect(screen.getByRole("option", { name: /^Use Zotero/ })).toBeTruthy();
  press("Escape");
  expect(screen.queryByRole("option", { name: /^Use Zotero/ })).toBeNull();
  expect(document.activeElement).toBe(search());
  expect(search().value).toBe("invented");
  expect(hides(calls)).toBe(0);
});

test("a held Esc takes one step, not every layer down to hiding", async () => {
  const { calls } = await chosenCitation();
  type("invented");
  press("Escape");
  press("Escape", { repeat: true });
  press("Escape", { repeat: true });
  expect(hides(calls)).toBe(0);
});

test("Esc works after a hide, when WebKitGTK calls the next press a repeat", async () => {
  // The release of the Esc that hid the window reached another window.
  const calls = backend();
  render(<App />);
  await waitFor(() => expect(document.activeElement).toBe(search()));
  press("Escape");
  await waitFor(() => expect(hides(calls)).toBe(1));
  await reopen();
  await waitFor(() => expect(document.activeElement).toBe(search()));
  press("Escape", { repeat: true });
  await waitFor(() => expect(hides(calls)).toBe(2));
});

test.each(["macos", "windows"])("on %s a key held through a reopening stays a repeat", async (system) => {
  document.documentElement.dataset.system = system;
  try {
    const calls = backend();
    render(<App />);
    await waitFor(() => expect(document.activeElement).toBe(search()));
    press("Escape");
    await waitFor(() => expect(hides(calls)).toBe(1));
    await reopen();
    await waitFor(() => expect(document.activeElement).toBe(search()));
    press("Escape", { repeat: true });
    await act(async () => { await new Promise(requestAnimationFrame); });
    expect(hides(calls)).toBe(1);
  } finally { delete document.documentElement.dataset.system; }
});

test("the footer's Esc action steps back through the same stack", async () => {
  backend({ library_status: () => library, search: () => [result("picked", "Invented reference")] });
  render(<App />);
  await screen.findByText("Invented reference");
  act(() => search().focus());
  type("invented");
  await screen.findByRole("button", { name: /^Clear search/i });
  fireEvent.keyDown(window, { key: "Alt", altKey: true });
  fireEvent.click(await within(screen.getByRole("group", { name: /keys/i })).findByRole("button", { name: "Clear Search" }));
  expect(search().value).toBe("");
});

test("Esc with focus on the page itself still hides the picker", async () => {
  const calls = backend();
  render(<App />);
  await waitFor(() => expect(document.activeElement).toBe(search()));
  act(() => search().blur());
  fireEvent.keyDown(document.body, { key: "Escape" });
  expect(document.activeElement).toBe(search());
  await waitFor(() => expect(calls).toContain("hide_picker"));
});

test("after a blank click the next key goes to Search, keeping its native effect", async () => {
  backend();
  render(<App />);
  await waitFor(() => expect(document.activeElement).toBe(search()));
  for (const key of [{ key: "s" }, { key: "v", ctrlKey: true }, { key: "v", metaKey: true }, { key: "Backspace" }, { key: "Home" }, { key: "ArrowLeft", altKey: true }, { key: "å", altKey: true }, { key: "Process" }, { key: "Enter", keyCode: 229 }, { key: "?" }]) {
    act(() => search().blur());
    expect(document.activeElement).toBe(document.body);
    expect(fireEvent.keyDown(document.body, key)).toBe(true); // not cancelled
    expect(document.activeElement).toBe(search());
    expect(screen.queryByRole("dialog")).toBeNull();
  }
});

test("focus left on a hidden field moves to a visible control, so Esc and keys still work", async () => {
  const calls = backend({ library_status: () => welcome });
  render(<App />);
  await screen.findByRole("option", { name: /^Use Zotero/ });
  const more = await screen.findByRole("button", { name: /^More/ });
  await waitFor(() => expect(search().closest("[hidden]")).not.toBeNull());
  act(() => search().focus());
  press("k", { ctrlKey: true });
  expect(await screen.findByRole("dialog", { name: "Actions" })).toBeTruthy();
  press("Escape");
  // Back to a visible control, so the next key still works.
  expect(document.activeElement?.closest("[hidden]")).toBeNull();
  expect([more, ...document.querySelectorAll(".choice-row")]).toContain(document.activeElement);
  press("Escape");
  await waitFor(() => expect(calls).toContain("hide_picker"));
});

test("Esc from Choose Library over the welcome screen returns to a control that is on screen", async () => {
  backend({ library_status: () => welcome });
  render(<App />);
  await screen.findByRole("option", { name: /^Use Zotero/ });
  const more = await screen.findByRole("button", { name: /^More/ });
  act(() => more.focus());
  press("o", { ctrlKey: true, shiftKey: true });
  await act(async () => { await new Promise(requestAnimationFrame); });
  const row = screen.getAllByRole("option")[0]!;
  act(() => row.focus());
  press("Escape");
  expect(screen.queryByRole("dialog")).toBeNull();
  await waitFor(() => expect(focusLost()).toBe(false));
  expect(search().closest("[hidden]")).not.toBeNull();
});

const commands = [
  ["Settings…", "menuitem", ",", {}, "open_settings"],
  ["Keyboard Shortcuts", "menuitem", "/", {}, null],
  ["Keep Window Open", "menuitemcheckbox", "p", { shiftKey: true }, "set_pinned"],
  ["Quit Ibis", "menuitem", "q", {}, "quit"],
  ["Open .*thesis.bib", "menuitem", "o", {}, "open_library_source"],
] as const;

test.each(commands)("%s does the same by key and from More", async (name, role, key, modifiers, native) => {
  const outcome = async (activate: () => Promise<void> | void) => {
    const calls = backend({ library_status: () => library, search: () => [result("picked", "Invented reference")],
      open_settings: () => null, quit: () => null, open_library_source: () => null, set_pinned: () => null });
    const view = render(<App />);
    await screen.findByText("Invented reference");
    act(() => search().focus());
    await activate();
    if (native) await waitFor(() => expect(calls).toContain(native));
    const seen = {
      native: native ? calls.filter((call) => call === native).length : 0,
      menu: screen.queryAllByRole("dialog", { name: "Actions" }).length, sheet: document.querySelectorAll(".ui-sheet-body").length,
    };
    view.unmount();
    await resetApp();
    return seen;
  };
  const byKey = await outcome(() => { press(key, { ctrlKey: true, ...modifiers }); });
  const byMenu = await outcome(() => chooseFromMore(new RegExp(`^${name}`)));
  // A toggle leaves its menu open so several can be changed; the key opened none.
  expect({ ...byMenu, menu: 0 }).toEqual({ ...byKey, menu: 0 });
  if (role === "menuitem") expect(byMenu.menu).toBe(0);
});

test.each(["the key", "More"])("Settings from %s keeps a pinned picker's pin for its next opening", async (route) => {
  const opened = vi.fn(() => null);
  const pins: unknown[] = [];
  backend({ library_status: () => library, open_settings: opened, set_pinned: ({ on }) => { pins.push(on); return null; } });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("thesis.bib"));
  act(() => search().focus());
  press("p", { ctrlKey: true, shiftKey: true });
  await waitFor(() => expect(pins).toEqual([true]));
  if (route === "the key") press(",", { ctrlKey: true });
  else await chooseFromMore("Settings…");
  await waitFor(() => expect(opened).toHaveBeenCalledOnce());
  expect(screen.queryByRole("menu")).toBeNull();
  // Native hides it even pinned; nothing here may undo the pin.
  await reopen();
  expect(pins).toEqual([true]);
  expect(screen.getByRole("button", { name: "Keep Window Open", pressed: true })).toBeTruthy();
  expect(document.activeElement).toBe(search());
});

test("More names Keep Window Open the same way on and off, with a check", async () => {
  backend({ library_status: () => library });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("thesis.bib"));
  const keep = async () => paletteRow(await screen.findByRole("dialog", { name: "Actions" }), "Keep Window Open");
  await chooseFromMore("Keep Window Open");
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  expect((await keep()).getAttribute("aria-current")).toBe("true");
  fireEvent.click(await keep());
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  await waitFor(async () => expect((await keep()).getAttribute("aria-current")).toBeNull());
});

test("Settings and a source name the opening that asked, so a late one cannot hide a newer one", async () => {
  const asked: unknown[] = [];
  backend({ library_status: () => library, search: () => [result("picked", "Invented reference")],
    open_settings: ({ presentation }) => { asked.push(presentation); return null; },
    open_library_source: ({ presentation }) => { asked.push(presentation); return null; } });
  render(<App />);
  await screen.findByText("Invented reference");
  await reopen();
  press(",", { ctrlKey: true });
  press("o", { ctrlKey: true });
  await waitFor(() => expect(asked).toEqual([43, 43]));
});

test("a command clears every layer in its way, and the keys for an open layer close it", async () => {
  await chosenCitation();
  press("k", { ctrlKey: true });
  await screen.findByRole("dialog", { name: "Actions" });
  press("/", { ctrlKey: true });
  await waitFor(() => expect(document.querySelector(".ui-sheet-body")).not.toBeNull());
  expect(screen.queryByRole("dialog", { name: "Actions" })).toBeNull();
  press("k", { ctrlKey: true });
  await screen.findByRole("dialog", { name: "Actions" });
  expect(document.querySelector(".ui-sheet-body")).toBeNull();
  press("k", { ctrlKey: true });
  expect(screen.queryByRole("dialog", { name: "Actions" })).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole("button", { name: /^More/ }));
  press("f", { ctrlKey: true, shiftKey: true });
  await screen.findByRole("dialog", { name: "Format" });
  press("f", { ctrlKey: true, shiftKey: true });
  expect(screen.queryByRole("dialog", { name: "Format" })).toBeNull();
  expect(document.activeElement).toBe(search());
});

test("the inspector stays live: a command opens over it and Esc comes back through it", async () => {
  const opened = vi.fn(() => null);
  backend({ library_status: () => library, search: () => [result("picked", "Invented reference")], open_settings: opened });
  render(<App />);
  await screen.findByText("Invented reference");
  const page = await openCitationDetails();
  await waitFor(() => expect(document.activeElement).toBe(page));
  fireEvent.change(page, { target: { value: "12" } });
  press("k", { ctrlKey: true });
  await screen.findByRole("dialog", { name: "Actions" });
  expect(page.isConnected).toBe(true);
  press(",", { ctrlKey: true });
  await waitFor(() => expect(opened).toHaveBeenCalledOnce());
  expect(screen.queryByRole("menu")).toBeNull();
  expect(document.activeElement).toBe(search());
  await reopen();
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Page" })).toBeNull());
});

test("refused: a hide the native side declines leaves the stack working", async () => {
  const calls = backend({ hide_picker: () => false });
  render(<App />);
  await waitFor(() => expect(document.activeElement).toBe(search()));
  press("Escape");
  await waitFor(() => expect(hides(calls)).toBe(1));
  await reopen();
  press("k", { ctrlKey: true });
  await screen.findByRole("dialog", { name: "Actions" });
  press("Escape");
  expect(screen.queryByRole("menu")).toBeNull();
});

test("failed I/O: Settings that cannot open says so and leaves the keyboard in Search", async () => {
  backend({ library_status: () => library, open_settings: () => { throw new Error("no window"); } });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("thesis.bib"));
  act(() => search().focus());
  press("k", { ctrlKey: true });
  await screen.findByRole("dialog", { name: "Actions" });
  press(",", { ctrlKey: true });
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  expect(await screen.findByText(/settings/i)).toBeTruthy();
  expect(document.activeElement).toBe(search());
  press("Escape");
});

test("delayed: a source open that fails after the picker reopened neither reports nor moves focus", async () => {
  let fail!: (reason: unknown) => void;
  backend({ library_status: () => library, search: () => [result("picked", "Invented reference")],
    open_library_source: () => new Promise((_, reject) => { fail = reject; }) });
  render(<App />);
  await screen.findByText("Invented reference");
  act(() => search().focus());
  press("o", { ctrlKey: true });
  await waitFor(() => expect(fail).toBeTypeOf("function"));
  await reopen();
  press("k", { ctrlKey: true });
  const menu = await screen.findByRole("dialog", { name: "Actions" });
  await act(async () => { fail({ code: "failed" }); });
  expect(menu.isConnected).toBe(true);
  expect(screen.queryByText(/could not be opened/i)).toBeNull();
});

test("delayed: a key pressed while a menu closes acts on what is on screen now", async () => {
  await chosenCitation();
  type("invented");
  press("k", { ctrlKey: true });
  await screen.findByRole("dialog", { name: "Actions" });
  press("Escape");
  press("Escape");
  expect(search().value).toBe("");
  expect(document.activeElement).toBe(search());
});

test.each(["More", "Library", "Format", "Keys", "Details"])("close and reopen: %s does not come back", async (layer) => {
  const { calls, chip } = await chosenCitation({ library_status: () => library });
  if (layer === "More") press("k", { ctrlKey: true });
  if (layer === "Keys") press("/", { ctrlKey: true });
  if (layer === "Format") press("f", { ctrlKey: true, shiftKey: true });
  if (layer === "Library") fireEvent.click(document.querySelector(".library-status-trigger")!);
  if (layer === "Details") fireEvent.click(chip);
  const open = () => screen.queryAllByRole("menu").length + screen.queryAllByRole("dialog").length + document.querySelectorAll(".inspector").length;
  await waitFor(() => expect(open()).toBeGreaterThan(0));
  await reopen();
  await waitFor(() => expect(open()).toBe(0));
  expect(document.activeElement).toBe(search());
  const before = hides(calls);
  press("Escape");
  await waitFor(() => expect(hides(calls)).toBe(before + 1));
});

test("the chooser owns the middle area: the sample library's block leaves while it is up", async () => {
  backend({ library_status: () => ({ ...library, sample: true }) });
  render(<App />);
  const exit = await screen.findByText(/your own library/i);
  act(() => search().focus());
  press("o", { ctrlKey: true, shiftKey: true });
  await screen.findByRole("option", { name: /^Use Zotero/ });
  expect(exit.isConnected).toBe(false);
  press("Escape");
  expect(await screen.findByText(/your own library/i)).toBeTruthy();
});

test("the status dot stops being green when the status menu reports a failed group list", async () => {
  backend({ library_status: () => zotero, zotero_groups: () => { throw new Error("offline"); } });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("Zotero"));
  const dot = () => document.querySelector(".library-status-controls")!.getAttribute("data-health");
  expect(dot()).toBe("done");
  fireEvent.click(document.querySelector(".library-status-trigger")!);
  const menu = await screen.findByRole("menu");
  // The section label repeats the heading for screen readers, so the words appear twice.
  expect((await within(menu).findAllByText(/could not list your Zotero groups/i)).length).toBeGreaterThan(0);
  await waitFor(() => expect(dot()).toBe("look"));
});
