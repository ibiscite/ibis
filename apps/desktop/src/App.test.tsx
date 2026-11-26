import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import axe from "axe-core";
import capability from "../src-tauri/capabilities/default.json";
import config from "../src-tauri/tauri.conf.json";
import { OUTPUTS } from "./bindings";
import { outputName } from "./format/outputs";
import { missingKey, recordId } from "./test-fixtures";
import { backend, paletteRow, chooseFromLibrary, libraryButton, prepared, receipt, resetApp, result, type PreviewRequest, type SyntheticId, citationKeys, chooseForm, openCitationDetails, textField, inCitation, citationDetails, removeFromCitation, toggleKeepOpen } from "./testing/app";
import { fixtureDestination, formatAnswer } from "./testing/format";
import { isMac } from "./platform";
import type { PickerDestination, WritingsView } from "./api";

afterEach(resetApp);

test("a pasted list previews and copies every result in order, with a safe missing-key notice", async () => {
  const copied = vi.fn((args: unknown) => {
    const { preview } = prepared(args);
    return { outcome: "retained", copied: { text: preview.text, warnings: [] }, reason: "pinned" };
  });
  backend({
    search: ({ query }) => query.text ? { revision: "1", results: [result("b", "Second"), result("a", "First")], list: { missing: ["<script>invented</script>"] } } : [],
    deliver_citation: copied,
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.paste(box, { clipboardData: { getData: () => "b\na\nmissing" } });
  expect((box as HTMLInputElement).value).toBe("b\ta\tmissing");
  await screen.findByText("Pasted list");
  expect(await screen.findByText("Copy 2 Citations")).toBeTruthy();
  expect(await screen.findByText("Not in your library: <script>invented</script>")).toBeTruthy();
  expect(document.querySelector("script")).toBeNull();
  await waitFor(() => expect(screen.getByRole("region", { name: "Preview" }).textContent).toContain("@b; @a"));
  fireEvent.keyDown(box, { key: "ArrowDown" });
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(copied).toHaveBeenCalledOnce());
  const request = prepared(copied.mock.calls[0]![0]).request;
  expect(request.items.map(({ id }) => id.key)).toEqual(["b", "a"]);
});

const selections = (ids: (SyntheticId | { library: string; key: string })[]) => ids.map((id) => ({ id: { library: id.library, key: typeof id.key === "string" ? recordId(id.key) : id.key }, cite: `synthetic-cite-${id.key}`, locator: null }));

function pressRow(row: HTMLElement) {
  act(() => row.focus());
  fireEvent.keyDown(row, { key: "Enter", code: "Enter" });
  fireEvent.keyUp(row, { key: "Enter", code: "Enter" });
}
const FILE_ROW = /^Choose a Library File…/;
const formatButton = (name: string | RegExp = /^Citation format /) => within(screen.getByLabelText("Search references").closest("header")!).getByRole("button", { name });

/** The format the header's format menu shows, as its value. */
function formatValue() {
  const shown = formatButton().getAttribute("aria-label")!.slice("Citation format ".length);
  const choices = OUTPUTS.map((output) => output.id);
  return choices.find((choice) => shown === outputName(choice));
}

const selectedKeys = citationKeys;

test("C5 click selects without copying and double-click copies the highlighted citation", async () => {
  const copied = vi.fn(() => ({ outcome: "retained", copied: { text: "[@b]", warnings: [] }, reason: "pinned" }));
  const calls = backend({ search: () => [result("a", "First"), result("b", "Second")], deliver_citation: copied });
  render(<App />);
  fireEvent.click(await screen.findByText("Second"), { detail: 1 });
  expect(screen.getByRole("option", { name: /Second/ }).getAttribute("aria-selected")).toBe("true");
  await waitFor(() => expect(document.querySelector(".shelf-output")?.textContent).toContain("@b"));
  expect(copied).not.toHaveBeenCalled();
  expect(calls).not.toContain("hide_picker");
  expect(document.activeElement).toBe(screen.getByLabelText("Search references"));
  fireEvent.click(screen.getByText("Second"), { detail: 2 });
  await waitFor(() => expect(copied).toHaveBeenCalledOnce());
});

test("typing a query shows results from the search command", async () => {
  backend({
    search: ({ query: { text: query } }) =>
      query ? [result("smith2020", "Sleep and memory")] : [],
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  expect(await screen.findByText("Sleep and memory")).toBeTruthy();
  expect(screen.getByText("smith2020", { selector: ".result-key" })).toBeTruthy();
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
  await act(async () => { status({ state: "none", has_snapshot: false, zotero: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, problem: null, note: null }); });
  pressRow(await screen.findByRole("option", { name: FILE_ROW }));
  await waitFor(() => expect(calls).toContain("choose_library"));
  expect(screen.queryByRole("alert")).toBeNull();
  expect(calls).not.toContain("search");
});

test("Enter submits the highlighted result in the remembered format", async () => {
  const copies: unknown[] = [];
  const calls = backend({
    picker_preferences: () => ({ output: "typst" }),
    search: ({ query: { text: query } }) => (query ? [result("a", "First"), result("b", "Second")] : []),
    deliver_citation: (nativeReceipt) => { const args = prepared(nativeReceipt).request;
      copies.push(args);
      return { outcome: "retained", copied: { text: "@b", warnings: [] }, reason: "pinned" };
    },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  await waitFor(() => expect(formatValue()).toBe("typst"));
  fireEvent.change(box, { target: { value: "s" } });
  await screen.findByText("Second");
  fireEvent.keyDown(box, { key: "ArrowDown" });
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(calls).toContain("deliver_citation"));
  expect(copies).toMatchObject([{ items: selections([{ library: "refs.bib", key: "b" }]), output: "typst", expectedRevision: "1" }]);
  expect(screen.getByText("Copied")).toBeTruthy();
});

test("the selection is kept by record identity across queries and copied in order", async () => {
  const copies: unknown[] = [];
  backend({
    search: ({ query: { text: query } }) =>
      query === "one" ? [result("same", "One", "a.bib")] : query === "two" ? [result("same", "Two", "b.bib")] : [],
    deliver_citation: (nativeReceipt) => { const args = prepared(nativeReceipt).request;
      copies.push(args);
      return { outcome: "retained", copied: { text: "[@same; @same]", warnings: [] }, reason: "pinned" };
    },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "one" } });
  await screen.findByText("One");
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  fireEvent.change(box, { target: { value: "two" } });
  fireEvent.click(await screen.findByText("Two"), { shiftKey: true });
  await waitFor(() => expect(selectedKeys()).toEqual(["same", "same"]));
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(copies[0]).toMatchObject({
    items: selections([
      { library: "a.bib", key: "same" },
      { library: "b.bib", key: "same" },
    ]),
    output: "pandoc",
    expectedRevision: "1",
  });
});

test("warnings keep the picker open and failures say nothing was copied", async () => {
  let fail = false;
  const calls = backend({
    search: ({ query: { text: query } }) => (query ? [result("a", "First")] : []),
    deliver_citation: () => {
      if (fail) throw new Error("SYNTHETIC_PRIVATE_PAYLOAD");
      return { outcome: "retained", copied: { text: "a", warnings: [missingKey("bare2026")] }, reason: "pinned" };
    },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "f" } });
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Enter" });
  expect((await screen.findByLabelText("Copy warnings")).textContent).toContain("bare2026");
  expect(calls).not.toContain("hide_picker");
  fail = true;
  fireEvent.keyDown(box, { key: "Enter" });
  expect((await screen.findByRole("alert")).textContent).toContain("Nothing was copied");
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_PAYLOAD");
});

test("Enter while composing text with an input method copies nothing", async () => {
  const calls = backend({ search: ({ query: { text: query } }) => (query ? [result("a", "First")] : []) });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "f" } });
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Enter", isComposing: true });
  fireEvent.keyDown(box, { key: "Enter", keyCode: 229 });
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(calls).not.toContain("deliver_citation");
});

test("the library is shown, can be chosen and reloaded, and failures are explained", async () => {
  const calls = backend({
    library_status: () => ({ state: "none", has_snapshot: false, zotero: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, problem: null, note: null }),
    choose_library: () => null,
    reload_library: () => null,
  });
  render(<App />);
  const bar = screen.getByRole("region", { name: "Picker actions" });
  await waitFor(() => expect(bar.textContent).toContain("No library chosen"));
  await chooseFromLibrary("Choose Library…");
  pressRow(await screen.findByRole("option", { name: FILE_ROW }));
  await waitFor(() => expect(calls).toContain("choose_library"));
  await emit("library:changed", { revision: "1", state: "loaded", has_snapshot: true, zotero: false, name: "refs.bib", snapshot_name: "refs.bib", references: 3, warnings: 1, reading: [{ code: "field", line: 2, key: null }], error: null, problem: null, note: null });
  await waitFor(() => expect(bar.textContent).toContain("refs.bib · 1 to check"));
  await chooseFromLibrary("Reload Library");
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
  await waitFor(() => expect(screen.getByRole("status", { name: "Library status" }).textContent).toContain("not opened"));
  expect(document.querySelector(".ui-line-notice, .notice-slot")).toBeNull();
  expect(document.querySelector(".library-status-trigger")?.getAttribute("title")).toContain("could not be opened");
});

test("web plugin permissions grant only event subscriptions and drag-region bootstrap", () => {
  expect(capability.permissions.filter((permission) => permission.includes(":")).sort()).toEqual([
    "core:event:allow-listen", "core:event:allow-unlisten",
    "core:window:allow-internal-toggle-maximize", "core:window:allow-start-dragging",
  ]);
  expect(config.app.windows[0]?.maximizable).toBe(false);
});

test.each([false, true])("axe checks picker structure (library open: %s) and detects a broken combobox", async (open) => {
  backend({
    library_status: () => ({ state: open ? "loaded" : "none", has_snapshot: open, zotero: false, name: open ? "refs.bib" : null, snapshot_name: open ? "refs.bib" : null, references: open ? 1 : 0, warnings: 0, error: null, problem: null, note: null }),
    search: () => [result("synthetic", "Synthetic reference")],
  });
  render(<App />);
  if (open) await screen.findByText("Synthetic reference");
  else await screen.findByRole("option", { name: FILE_ROW });
  const options: axe.RunOptions = {
    runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] },
    // JSDOM has no layout/canvas: theme contrast has its own numeric test above.
    rules: { "color-contrast": { enabled: false } },
  };
  // Report the actual lack of canvas support; do not emulate rendered pixels.
  const canvas = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  try {
    expect((await axe.run(document.body, options)).violations.map((v) => v.id)).toEqual([]);
    // Welcome has no search yet, so only an open library can break it.
    if (!open) { expect(document.querySelector<HTMLElement>(".search-bar")!.hidden).toBe(true); return; }
    screen.getByRole("combobox", { name: "Search references" }).removeAttribute("aria-expanded");
    const broken = await axe.run(document.body, options);
    expect(broken.violations.map((v) => v.id)).toContain("aria-required-attr");
  } finally {
    canvas.mockRestore();
  }
});

test("Ctrl+K opens More's actions with their keys, Escape closes it before hiding the picker", async () => {
  const calls = backend({ hide_picker: () => null, reload_library: () => null });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  await waitFor(() => expect(screen.getByRole("region", { name: "Picker actions" }).textContent).toContain("refs.bib"));
  fireEvent.keyDown(box, { key: "k", ctrlKey: true });
  const palette = await screen.findByRole("dialog", { name: "Actions" });
  const field = within(palette).getByRole("combobox", { name: "Search actions" });
  expect(document.activeElement).toBe(field);
  const label = (row: HTMLElement) => row.querySelector(".ui-palette-label")?.textContent;
  expect(within(palette).getAllByRole("option").map(label)).toEqual(["Open ⁨refs.bib⁩", "Show in Folder", "Choose Library…", "Check Against a .bib File…", "Change Format…", "Peek at Reference", "Go to Citation", "Show Citation Details", "Move Left", "Move Right", "Remove from Citation", "Cited Since Opening…", "Forget Recent Citations", "Keep Window Open", "Keyboard Shortcuts", "Settings…", "Quit Ibis"].filter((name) => within(palette).queryAllByRole("option").some((row) => label(row) === name)));
  expect(paletteRow(palette, "Keep Window Open").getAttribute("aria-current")).toBeNull();
  expect(paletteRow(palette, "Quit Ibis").getAttribute("aria-keyshortcuts")).toBe("Control+Q");
  // A common word finds an action by what it does.
  fireEvent.change(field, { target: { value: "preferences" } });
  expect(within(palette).getAllByRole("option").map(label)).toEqual(["Settings…"]);
  fireEvent.change(field, { target: { value: "" } });
  const canvas = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  try {
    const found = await axe.run(document.body, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa"] }, rules: { "color-contrast": { enabled: false } } });
    expect(found.violations.map((v) => v.id)).toEqual([]);
  } finally {
    canvas.mockRestore();
  }
  fireEvent.keyDown(field, { key: "Escape" });
  expect(screen.queryByRole("dialog", { name: "Actions" })).toBeNull();
  // Escape returns to More, as a menu returns to its button, even when its key opened it.
  const more = screen.getByRole("button", { name: /^More/ });
  expect(document.activeElement).toBe(more);
  expect(calls).not.toContain("hide_picker");
  fireEvent.keyDown(more, { key: "Escape" });
  await waitFor(() => expect(calls).toContain("hide_picker"));
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  fireEvent.click(within(screen.getByRole("dialog", { name: "Actions" })).getByRole("button", { name: "Back" }));
  expect(screen.queryByRole("dialog", { name: "Actions" })).toBeNull();
  expect(screen.getByRole("button", { name: /^More/ }).getAttribute("aria-keyshortcuts")).toBe("Control+K");
});

const savedOutput = (preferences: unknown) => (preferences as { output?: string; key_output?: string }).key_output ?? (preferences as { output?: string }).output ?? null;

const NO_LIBRARY = { state: "none", has_snapshot: false, zotero: false, name: null, snapshot_name: null, references: 0, warnings: 0, error: null, problem: null, note: null };

test("a pinned picker stays open after copying, and Escape never unpins it", async () => {
  const pins: unknown[] = [];
  const calls = backend({
    search: () => [result("a", "First")],
    deliver_citation: () => ({ outcome: "retained", copied: { text: "@a", warnings: [] }, reason: "pinned" }),
    set_pinned: ({ on }) => { pins.push(on); return null; },
  });
  render(<App />);
  // Its own state: wait for this picker's More, then turn the pin on and wait for it.
  await screen.findByRole("button", { name: /^More/ });
  await toggleKeepOpen();
  expect(screen.getByRole("button", { name: "Keep Window Open", pressed: true })).toBeTruthy();
  await waitFor(() => expect(pins).toEqual([true]));
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "first" } });
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Enter" });
  await screen.findByText("Copied");
  fireEvent.keyDown(box, { key: "Escape" });
  expect(screen.getByRole("button", { name: "Keep Window Open", pressed: true })).toBeTruthy();
  expect(calls).not.toContain("hide_picker");
  await toggleKeepOpen();
  await waitFor(() => expect(pins).toEqual([true, false]));
  fireEvent.change(box, { target: { value: "first" } });
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(calls.filter((command) => command === "deliver_citation")).toHaveLength(2));
});

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
  pressRow(await screen.findByRole("option", { name: /^Find Them on This Computer/ }));
  pressRow(await screen.findByRole("option", { name: /^Choose a Found Library/ }));
  await screen.findByRole("option", { name: /^Use thesis\.bib/ });
  const found = [...document.querySelectorAll<HTMLElement>("[data-key^='found-']")];
  expect(found.map((row) => row.querySelector(".choice-tag")?.textContent)).toEqual(["PhD", "Downloads"]);
  expect(screen.getByText(/turn on “Allow other applications on this computer to communicate with Zotero”/)).toBeTruthy();
  expect(screen.getByText(/Some folders were too large/)).toBeTruthy();
  pressRow(found[1]!);
  await waitFor(() => expect(opened).toEqual([{ list: 7, index: 1 }]));
  expect(screen.queryByRole("alert")).toBeNull();
});

function zoteroStatus(libraries: unknown[]) {
  return { revision: "1", state: "loaded", has_snapshot: true, zotero: true, zotero_libraries: libraries,
    name: libraries.length > 1 ? `${libraries.length} Zotero libraries` : "Zotero library",
    snapshot_name: "Zotero library", snapshot_zotero: true, references: 2, warnings: 0, error: null, problem: null, note: null };
}

test("The library menu lists Zotero libraries as checkboxes and turning one on reads them together", async () => {
  const chosen: unknown[][] = [];
  backend({
    library_status: () => zoteroStatus(["user"]),
    zotero_groups: () => [{ id: 42, name: "Synthetic lab" }, { id: 7, name: "Reading group" }],
    use_zotero_libraries: ({ libraries }) => { chosen.push(libraries as unknown[]); return null; },
  });
  render(<App />);
  await waitFor(() => expect(screen.getByRole("region", { name: "Picker actions" }).textContent).toMatch(/Zotero/));
  fireEvent.click(libraryButton());
  const group = await screen.findByRole("group", { name: "Zotero libraries" });
  await within(group).findByRole("menuitemcheckbox", { name: "Synthetic lab" });
  const [all, mine, lab, reading] = within(group).getAllByRole("menuitemcheckbox");
  const boxes = [all!, mine!, lab!, reading!];
  expect(boxes.map((box) => [box.textContent, box.getAttribute("aria-checked")])).toEqual([
    ["All Libraries", "false"],
    ["My Library", "true"], ["Synthetic lab", "false"], ["Reading group", "false"],
  ]);
  // The only library on cannot be turned off.
  expect(mine!.getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(mine!);
  fireEvent.click(lab!);
  await waitFor(() => expect(chosen).toEqual([["user", { group: 42 }]]));
  expect(screen.getByRole("menu")).toBeTruthy();
});

test("format toolbar menu exposes both sections and saves a choice without copying", async () => {
  const saved: unknown[] = [];
  const calls = backend({ picker_preferences_saved: (args) => { saved.push(savedOutput(args.preferences)); return null; } });
  render(<App />);
  fireEvent.click(await waitFor(() => formatButton("Citation format Pandoc")));
  expect(screen.getByRole("group", { name: "Key formats" })).toBeTruthy();
  expect(screen.getByRole("group", { name: "Citation styles" })).toBeTruthy();
  // Exactly the engine's list, each by its one name.
  const formats = within(screen.getByRole("dialog", { name: "Format" })).getAllByRole("option")
    .filter((item) => item.closest('[aria-label="Recent"], [aria-label="Key formats"], [aria-label="Citation styles"], [aria-label="Share references"]')).map((item) => item.querySelector(".ui-palette-label")?.textContent);
  expect([...formats].sort()).toEqual(OUTPUTS.map((output) => output.name).sort());
  expect(screen.getByRole("option", { name: "Pandoc" }).getAttribute("aria-current")).toBe("true");
  fireEvent.click(screen.getByRole("option", { name: "BibLaTeX \\autocite" }));
  await waitFor(() => expect(saved).toContain("autocite"));
  expect(calls).not.toContain("deliver_citation");
  expect(formatButton("Citation format BibLaTeX \\autocite")).toBeTruthy();
  await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText("Search references")));
});

test("typed shorthand adds the reference with its prefix and page, and the token says what will be cited", async () => {
  const previews: PreviewRequest[] = [];
  backend({
    search: ({ query }) => ({ revision: "1", results: [result("vale", "Lanterns at the edge of the sea")],
      interpretation: query.interpret_locators ? { search: "vale", prefix: "see", locator: { term: "page", value: "417" }, typed: "417" } : null }),
    preview_file_citation: (request) => { previews.push(request); return receipt("[see @vale, p. 417]"); },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "see vale 417" } });
  await screen.findByRole("group", { name: "Using see · p. 417" });
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  await waitFor(() => expect(previews.at(-1)?.items).toEqual([expect.objectContaining({ prefix: "see", locator: { term: "page", value: "417" } })]));
  // Searching as typed forgets what was read.
  fireEvent.click(screen.getByRole("button", { name: "Search as typed" }));
  expect(screen.queryByRole("group", { name: /^Using / })).toBeNull();
});

test("Ctrl+, opens Settings from the picker, and a change made there reaches the picker", async () => {
  let largerText = false;
  const pages: unknown[] = [];
  backend({
    open_settings: ({ page }) => { pages.push(page); return null; },
    picker_preferences: () => ({ output: "pandoc", density: "comfortable", larger_text: largerText, paste: true, menu_bar: true }),
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  await screen.findByRole("button", { name: /^Citation format/ });
  fireEvent.keyDown(box, { key: ",", ctrlKey: true });
  await waitFor(() => expect(pages).toEqual([null]));
  largerText = true;
  await act(async () => { await emit("preferences:changed", {}); });
  await waitFor(() => expect(document.documentElement.dataset.textSize).toBe("larger"));
});

// Citation details
test("removing a middle reference and Undo restore its position and fields", async () => {
  backend({ search: () => [result("a", "First work"), result("b", "Second work"), result("c", "Third work")] });
  render(<App />);
  fireEvent.click(await screen.findByText("First work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Second work"), { ctrlKey: true });
  fireEvent.click(screen.getByText("Third work"), { ctrlKey: true });
  await waitFor(() => expect(citationKeys()).toEqual(["a", "b", "c"]));
  fireEvent.click(inCitation("b")!);
  fireEvent.change(await screen.findByRole("textbox", { name: "Page" }), { target: { value: "iv–vi" } });
  fireEvent.change(textField("Text before"), { target: { value: "see" } });
  fireEvent.change(textField("Text after"), { target: { value: "for details" } });
  removeFromCitation(inCitation("b"));
  expect(citationDetails()).toBeNull();
  await waitFor(() => expect(citationKeys()).toEqual(["a", "c"]));
  fireEvent.click(screen.getByRole("button", { name: /^Undo / }));
  await waitFor(() => expect(citationKeys()).toEqual(["a", "b", "c"]));
  fireEvent.click(inCitation("b")!);
  expect((await screen.findByRole<HTMLInputElement>("textbox", { name: "Page" })).value).toBe("iv–vi");
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Text before" }).value).toBe("see");
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Text after" }).value).toBe("for details");
});

test("editing the highlighted reference freezes its page and form for preview and canonical copy", async () => {
  const previews: PreviewRequest[] = [];
  const copies: PreviewRequest[] = [];
  backend({
    search: ({ query }) => query.text ? [result("b", "Second work")] : [result("a", "First work")],
    preview_file_citation: (request) => { previews.push(request); return receipt(`preview ${previews.length}`); },
    deliver_citation: (args) => { const saved = prepared(args); copies.push(saved.request); return { outcome: "retained", copied: { text: saved.preview.text, warnings: [] }, reason: "pinned" }; },
  });
  render(<App />);
  const page = await openCitationDetails();
  act(() => page.focus());
  fireEvent.change(page, { target: { value: "12–15" } });
  expect(document.activeElement).toBe(page);
  await chooseForm("Name in the sentence");
  await waitFor(() => expect(previews.at(-1)?.items).toMatchObject([
    { id: result("a", "").id, locator: null, locator_input: { label: "page", text: "12–15" }, form: "narrative" },
  ]));
  const search = screen.getByLabelText("Search references");
  fireEvent.change(search, { target: { value: "second" } });
  await screen.findByText("Second work");
  expect((page as HTMLInputElement).value).toBe("12–15");
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(copies[0]?.items).toMatchObject([{ id: result("a", "").id, locator: null, locator_input: { label: "page", text: "12–15" }, form: "narrative" }]);
});

test("the details open beside a live list; Esc steps back one layer and returns to the token", async () => {
  backend({ search: () => ({ revision: "1", results: [result("a", "Invented work")] }), hide_picker: () => true });
  render(<App />);
  const search = screen.getByLabelText("Search references");
  await screen.findByText("Invented work");
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  const token = await screen.findByRole("button", { name: "Edit Smith 2020" });
  act(() => token.focus());
  fireEvent.keyDown(token, { key: "ArrowDown" });
  const page = await screen.findByRole("textbox", { name: "Page" });
  expect(document.activeElement).toBe(page);
  expect(screen.getByRole("option")).toBeTruthy();
  expect(token.getAttribute("aria-expanded")).toBe("true");
  expect(document.getElementById(token.getAttribute("aria-controls")!)?.contains(page)).toBe(true);
  expect(screen.getByRole("dialog", { name: "Citation details for Invented work" }).contains(page)).toBe(true);
  fireEvent.keyDown(page, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Page" })).toBeNull());
  expect(document.activeElement).toBe(token);
  // React Aria saw the focus, so the keyboard ring draws.
  expect(token.hasAttribute("data-focus-visible")).toBe(true);
  expect(token.getAttribute("aria-expanded")).toBe("false");
});

// One list of commands
test('">" is searched like any other text, and More finds a command by what it does', async () => {
  const asked: string[] = [];
  backend({ search: ({ query }) => { asked.push(query.text); return query.text ? [result("vale2021", "The quiet garden")] : []; } });
  render(<App />);
  const search = screen.getByLabelText("Search references");
  fireEvent.change(search, { target: { value: "> quiet" } });
  await screen.findByRole("option", { name: /The quiet garden/ });
  expect(asked).toContain("> quiet");
  expect(screen.queryByRole("listbox", { name: "Commands" })).toBeNull();
  fireEvent.keyDown(search, { key: "k", ctrlKey: true });
  const palette = await screen.findByRole("dialog", { name: "Actions" });
  // A key's description in the keys sheet ("Open Settings") finds its action, as ">" once did.
  fireEvent.change(within(palette).getByRole("combobox", { name: "Search actions" }), { target: { value: "open settings" } });
  expect(within(palette).getAllByRole("option").map((row) => row.querySelector(".ui-palette-label")?.textContent)).toEqual(["Settings…"]);
});

// Opening from Zotero
test("inside Zotero Enter opens the PDF or item, Command-O shows it, and Alt-Enter copies", async () => {
  const open = vi.fn(async (_args: Record<string, unknown>) => null);
  const calls = backend({ library_status: () => ({ state: "loaded", has_snapshot: true, zotero: true, snapshot_zotero: true, zotero_libraries: ["user"], name: "Zotero", references: 1, warnings: 0, error: null, problem: null, note: null }), search: () => [result("a", "Synthetic reference")],
    "destination": () => fixtureDestination({ app: "Zotero", app_id: "org.zotero.zotero", find_open: true }),
    open_library_source: open, zotero_groups: () => [],
  });
  render(<App />);
  await screen.findByText("Synthetic reference");
  const search = screen.getByLabelText("Search references");
  await screen.findByText("Open PDF, or show item in Zotero");
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(open).toHaveBeenCalledOnce());
  expect(open.mock.calls[0]?.[0]).toMatchObject({ reveal: false });
  expect(calls).not.toContain("deliver_citation");
  fireEvent.keyDown(search, { key: "o", ctrlKey: !isMac, metaKey: isMac });
  await waitFor(() => expect(open).toHaveBeenCalledTimes(2));
  expect(open.mock.calls[1]?.[0]).toMatchObject({ reveal: true });
  fireEvent.keyDown(search, { key: "1", code: "Digit1", ctrlKey: !isMac, metaKey: isMac });
  await waitFor(() => expect(open).toHaveBeenCalledTimes(3));
  expect(open.mock.calls[2]?.[0]).toMatchObject({ reveal: false });
  expect(calls).not.toContain("deliver_citation");
  // The bar's action does what it says, as Enter does: it opens, never copies.
  fireEvent.click(screen.getByRole("button", { name: "Open PDF, or show item in Zotero" }));
  await waitFor(() => expect(open).toHaveBeenCalledTimes(4));
  expect(calls).not.toContain("deliver_citation");
  fireEvent.keyDown(search, { key: "Enter", altKey: true });
  await waitFor(() => expect(calls).toContain("deliver_citation"));
});

// Project bibliography
test("a known wrong-work key is refused and never reaches native copy, by Enter or by dragging", async () => {
  const called = backend({ search: () => [result("a", "Invented work")],
    preview_file_citation: () => ({ ...receipt("[@a]"), project_refusal: "A key in thesis.bib identifies a different work." }),
  });
  render(<App />);
  await screen.findByText("A key in thesis.bib identifies a different work.");
  // The action is off but still focusable and spoken, described by its reason in the bar.
  const action = document.querySelector<HTMLButtonElement>(".footer-action")!;
  expect(action.getAttribute("aria-disabled")).toBe("true");
  expect(document.getElementById(action.getAttribute("aria-describedby")!)?.textContent).toBe("A key in thesis.bib identifies a different work.");
  // Each Enter on the refusal is said again: a new announcement per press, the same words.
  const said = [...document.querySelectorAll('.delivery-bar [role="alert"]')].find((node) => !node.closest(".bar-reason"))!;
  const announced: string[] = [];
  const observer = new MutationObserver((records) => { for (const record of records) for (const node of record.addedNodes) announced.push(node.textContent ?? ""); });
  observer.observe(said, { childList: true, subtree: true });
  for (let press = 0; press < 2; press++) {
    await act(async () => { fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" }); });
  }
  observer.disconnect();
  expect(announced).toEqual(Array(2).fill("Not pasted or copied. A key in thesis.bib identifies a different work."));
  expect(called).not.toContain("deliver_citation");
  // Dragging delivers what Enter pastes, so it delivers nothing here either.
  const beak = document.querySelector<HTMLElement>(".shelf-leading")!;
  expect(beak.getAttribute("draggable")).toBeNull();
  const data = new Map<string, string>();
  fireEvent.dragStart(beak, { dataTransfer: { setData: (type: string, value: string) => data.set(type, value), effectAllowed: "" } });
  expect([...data.keys()]).toEqual([]);
});

// Writings
test("leaving a writing restores the app's output and copies a fresh preview", async () => {
  const writing = { id: "synthetic-project", name: "Field Guide", output: "cite", bibliography: null };
  let generation = 1;
  let selected: string | null = writing.id;
  const view = (): WritingsView => ({ writings: [writing], selected, context: `opening-${generation}`, session_only: false });
  const destination = (): PickerDestination => (fixtureDestination({ app: "Example Editor", app_id: "example.editor", app_identity: null,     unreadable: false, can_paste: false, paste: null, paste_question: false, can_choose_paste: false, writing: selected ? writing : null,
    writing_context: `opening-${generation}`, context_version: [generation, 0], offers_projects: true, format: formatAnswer(selected ? writing.output : "typst", selected ? "writing" : "app") }));
  const copied: string[] = [];
  backend({
    search: () => [result("a", "Synthetic source")], "destination": destination, writings: view,
    "edit-writing": async ({ context, action }) => {
      expect(context).toBe(`opening-${generation}`);
      expect(action).toEqual({ kind: "select", id: null });
      selected = null;
      generation += 1;
      await emit("project:changed", [generation, 0]);
      return view();
    },
    preview_file_citation: (request) => receipt(`${request.output}:${JSON.stringify(request.intent.context)}`),
    deliver_citation: (args) => { const text = prepared(args).preview.text; copied.push(text); return { outcome: "retained", copied: { text, warnings: [] }, reason: "pinned" }; },
  });
  render(<App />);
  await screen.findByText("Synthetic source");
  const formatButton = () => within(screen.getByLabelText("Search references").closest("header")!).getByRole("button", { name: /^Citation format / });
  // The pop-up names the format alone; whose it is, the project, is its spoken status.
  const formatStatus = () => screen.getByRole("status", { name: "Citation format" }).textContent;
  await waitFor(() => expect(formatStatus()).toContain("Field Guide"));
  expect(formatButton().getAttribute("aria-label")).not.toContain("Field Guide");
  fireEvent.click(formatButton());
  fireEvent.click(await screen.findByRole("option", { name: "New project…" }));
  const dialog = await screen.findByRole("dialog", { name: "Projects" });
  fireEvent.click(await within(dialog).findByRole("button", { name: "No Project" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Projects" })).toBeNull());
  await waitFor(() => expect(formatButton().getAttribute("aria-label")).toContain("Typst"));
  expect(formatStatus()).not.toContain("Field Guide");
  await act(async () => { fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" }); });
  await waitFor(() => expect(copied).toEqual(["typst:[2,0]"]));
});

test("the format palette offers Projects only when the engine does", async () => {
  let offers = false;
  backend({ search: () => [result("a", "Synthetic source")], destination: () => fixtureDestination({ app: "Example Editor", app_id: "example.editor", offers_projects: offers, format: formatAnswer("apa", "app") }) });
  const { unmount } = render(<App />);
  await screen.findByText("Synthetic source");
  const formatButton = () => within(screen.getByLabelText("Search references").closest("header")!).getByRole("button", { name: /^Citation format / });
  fireEvent.click(formatButton());
  await screen.findByRole("dialog", { name: "Format" });
  expect(screen.queryByRole("option", { name: "New project…" })).toBeNull();
  unmount();
  // A second format saved for this app, or a saved project: the engine offers them.
  offers = true;
  render(<App />);
  await screen.findByText("Synthetic source");
  fireEvent.click(formatButton());
  expect(await screen.findByRole("option", { name: "New project…" })).toBeTruthy();
});

// Editor requests
const plain = (text: string | null | undefined) => (text ?? "").replace(/[⁨⁩]/g, "");

test("a waiting editor's format is named once, in the header, and its end asks for a fresh preview", async () => {
  let waiting: { request: number; format: string } | null = { request: 1, format: "Pandoc" };
  let previews = 0;
  backend({
    editor_request: () => waiting,
    search: () => [result("taylor2026", "Synthetic signals")],
    preview_file_citation: () => { previews += 1; return receipt(waiting ? "@taylor2026" : "[@taylor2026]"); },
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "signals" } });
  await waitFor(() => expect(plain(document.body.textContent)).toContain("Citing for your editor. Esc cancels."));
  expect(document.querySelector(".search-format-label")?.textContent).toBe("Pandoc");
  expect(plain(document.body.textContent).match(/Pandoc/g)).toHaveLength(1);
  expect(screen.getByRole("button", { name: "Send to Editor" })).toBeTruthy();
  await waitFor(() => expect(document.body.textContent).toContain("@taylor2026"));
  const before = previews;
  waiting = null;
  await act(() => emit("editor:request"));
  await waitFor(() => expect(plain(document.body.textContent)).not.toContain("Citing for your editor"));
  await waitFor(() => expect(previews).toBeGreaterThan(before));
  await waitFor(() => expect(document.body.textContent).toContain("[@taylor2026]"));
});

// Shortcut status
test("a shortcut that could not be registered is explained", async () => {
  backend({
    shortcut_status: () => ({ shortcut: "Control+Command+R", registered: false, error: "another app may be using it" }),
  });
  render(<App />);
  expect((await screen.findByText(/another app may be using it/)).getAttribute("role")).toBe("status");
});

// First paste and typed words (maintainer, 2026-11-18)
test("the first screen's one Enter pastes into a practice line inside Ibis, then Enter delivers", async () => {
  const calls = backend({
    search: () => [result("aster2026", "Invented gardens")],
    delight_view: () => ({ status: null, peck: false, anniversary: false, build: "0.0.0 · invented · 2026-11-01", fun: false, fact: null, revision: 1, practice: true }),
  });
  render(<App />);
  const search = screen.getByLabelText("Search references");
  await screen.findByText("Invented gardens");
  expect(await screen.findByText("Try a paste here first: press Enter.")).toBeTruthy();
  await act(async () => { fireEvent.keyDown(search, { key: "Enter" }); });
  expect(await screen.findByText(/pasted here\. Next, Enter pastes in your app\./)).toBeTruthy();
  // Nothing left Ibis and no question was asked.
  expect(calls).not.toContain("deliver_citation");
  await act(async () => { fireEvent.keyDown(search, { key: "Enter" }); });
  await waitFor(() => expect(calls).toContain("deliver_citation"));
});

test("words the engine kept as words say so in one notice line", async () => {
  backend({ search: ({ query }) => query.text ? { revision: "1", results: [result("room22", "Room 22")], kept_words: true } : [] });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "room 22" } });
  expect(await screen.findByText("Kept as words. For a page, add a comma: “room, 22”.")).toBeTruthy();
});

test("the drawer is read and tabbed to where it is seen: after the list, before the bar", async () => {
  backend({ search: () => [result("a", "Invented work")], preview_file_citation: () => receipt("[@a]") });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: /^Edit / }));
  const drawer = await screen.findByRole("dialog", { name: /Citation details|Invented work/ });
  const list = document.getElementById("results")!, bar = document.querySelector("footer.delivery-bar")!;
  expect(list.compareDocumentPosition(drawer) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(drawer.compareDocumentPosition(bar) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  // Tab order follows: the details' fields come before the bar's Library, action and More.
  const order = [...document.querySelectorAll<HTMLElement>("button, input, [tabindex='0']")].filter((node) => !node.closest("[inert], [aria-hidden='true']"));
  const page = order.findIndex((node) => drawer.contains(node) && node.matches("input"));
  const more = order.findIndex((node) => bar.contains(node) && /^More/.test(node.getAttribute("aria-label") ?? node.textContent ?? ""));
  expect(page).toBeGreaterThanOrEqual(0);
  expect(more).toBeGreaterThan(page);
});
