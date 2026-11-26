import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { testStates } from "../../test-fixtures";
import type { LibraryStatus } from "../../api";
import { PickerShown } from "./pickerShown";
import { EmptyState, type ZoteroRecovery } from "./EmptyState";
import { LibrarySetup, type EmptyLibraryChoices } from "../../LibrarySetup";

afterEach(() => { cleanup(); vi.useRealTimers(); });
const setup = (focus: boolean, empty?: EmptyLibraryChoices) => <LibrarySetup
  api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined }}
  perform={async (_action, run) => { await run(); }} autoFocus={focus} empty={empty} />;
const loaded = testStates.find((state) => state.id === "no-results")!.library;

function mount(query = "", empty = false, narrowed = false) {
  const choose = vi.fn(), clear = vi.fn(), searchAll = vi.fn();
  const { container } = render(<EmptyState library={empty ? { ...loaded, references: 0 } : loaded} query={query}
    narrowed={narrowed} onChooseLibrary={choose} onClearSearch={clear} onSearchAll={searchAll} onReload={vi.fn()} setup={() => <p>Setup preserved</p>} />);
  return { container, choose, clear, searchAll };
}

const waiting = (problem: "zotero_access_off" | "zotero_closed" | "zotero_profiles" | "zotero_missing") =>
  ({ ...loaded, state: "error" as const, has_snapshot: false, zotero: true, problem });
const empty = (library: LibraryStatus, zotero: ZoteroRecovery, setupNode = setup, shown = true) => <PickerShown value={shown}><EmptyState library={library} query="" narrowed={false}
  onChooseLibrary={vi.fn()} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()} setup={setupNode} zotero={zotero} /></PickerShown>;
const recovery = (library: LibraryStatus, zotero: ZoteroRecovery, setupNode = setup) => render(empty(library, zotero, setupNode));

test("several profiles are offered by name and a choice names only that", async () => {
  const chooseProfile = vi.fn();
  recovery(waiting("zotero_profiles"), { listProfiles: async () => ({ names: ["Home", "Work"], chosen: null, running: false }), chooseProfile });
  expect(screen.getByText("Which Zotero profile?")).toBeTruthy();
  fireEvent.click(await screen.findByRole("option", { name: /Work/ }));
  expect(chooseProfile).toHaveBeenCalledExactlyOnceWith("Work");
});

const obstacle = (props: Partial<Parameters<typeof EmptyState>[0]>) => {
  const handlers = { onClearSearch: vi.fn(), onSearchAll: vi.fn(), onRemoveFilter: vi.fn() };
  const { container } = render(<EmptyState library={loaded} query="smith #sleep" narrowed={false} onChooseLibrary={vi.fn()} onReload={vi.fn()} setup={() => null} {...handlers} {...props} />);
  return { ...handlers, title: container.querySelector(".empty-title")?.textContent, buttons: screen.getAllByRole("button").map((button) => button.textContent) };
};

test("left-out libraries first say nothing matched, then offer searching them all", () => {
  const { title, buttons } = obstacle({ narrowed: true, query: "qxzq" });
  expect(title).toBe("No matching references");
  expect(document.querySelector(".empty-description")?.textContent).toBe("Some of your Zotero libraries are left out of this search.");
  expect(buttons).toEqual(["Search All Zotero Libraries"]);
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

test.each([false, true])("C12 zero-reference choices offer Zotero and a file, never no-match actions (Zotero=%s)", (zotero) => {
  const choose = vi.fn(async () => undefined), useZotero = vi.fn(async () => undefined);
  render(<EmptyState library={{ ...loaded, state: "loaded", references: 0, zotero }} query="unmatched" narrowed
    onChooseLibrary={choose} onClearSearch={vi.fn()} onSearchAll={vi.fn()} onReload={vi.fn()}
    setup={(focus, empty) => <LibrarySetup api={{ chooseLibrary: choose, useZotero }}
      perform={async (_action, run) => { await run(); }} autoFocus={focus} empty={empty} />} />);
  expect(screen.getAllByRole("option").map((row) => row.querySelector(".choice-name")?.textContent))
    .toEqual(["Use Zotero", "Choose a Library File…"]);
  expect(screen.getByText("Or drag a BibTeX, BibLaTeX, CSL-JSON, RIS or CSV file into this window.")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Clear Search" })).toBeNull();
});
