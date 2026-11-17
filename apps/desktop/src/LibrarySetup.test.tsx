import { currentPlatform } from "./platform";
import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { LibrarySetup } from "./LibrarySetup";

afterEach(() => { cleanup(); vi.restoreAllMocks(); delete document.documentElement.dataset.system; });
const perform = async (_action: string, run: () => Promise<unknown>) => { await run(); };
const option = (name: RegExp) => screen.getByRole("option", { name });
const findOption = (name: RegExp) => screen.findByRole("option", { name });
const press = (row: HTMLElement) => {
  act(() => row.focus());
  fireEvent.keyDown(row, { key: "Enter", code: "Enter" });
  fireEvent.keyUp(row, { key: "Enter", code: "Enter" });
};

test("first run teaches the basics, focuses Zotero and moves through four tiles in two dimensions", async () => {
  // jsdom has no grid geometry. React Aria uses actual row rectangles to move
  // vertically; the browser scenario checks this against the real CSS layout.
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const rows = [...(this.closest('.choice-list[data-layout="grid"]')?.querySelectorAll('[role="option"]') ?? [])];
    const index = rows.indexOf(this);
    return index < 0 ? new DOMRect() : new DOMRect((index % 2) * 200, Math.floor(index / 2) * 80, 200, 80);
  });
  const chooseLibrary = vi.fn(async () => undefined), useSampleLibrary = vi.fn(async () => undefined);
  const findSources = vi.fn(async () => ({ zotero: "absent" as const, list: 1, files: [], cut_short: false }));
  const { container } = render(<LibrarySetup api={{ chooseLibrary, useZotero: async () => undefined, findSources, useSampleLibrary }} perform={perform}
    shortcut={{ shortcut: "Control+Shift+Y", registered: true, error: null }} />);
  // Linux copies rather than pastes, and says so.
  expect(screen.getByText(currentPlatform() === "linux"
    ? "Ibis finds a reference and copies its citation for you to paste where you’re typing. It only reads your library, never changes it."
    : "Ibis finds a reference and pastes its citation where you’re typing. It only reads your library, never changes it.")).toBeTruthy();
  expect(screen.getByText(/After this, open Ibis from any app with/)).toBeTruthy();
  expect(container.querySelector('.welcome-mark[aria-hidden="true"]')).toBeTruthy();
  expect(screen.getAllByRole("option")).toHaveLength(4);
  await waitFor(() => expect(document.activeElement).toBe(option(/^Use Zotero/)));
  fireEvent.keyDown(document.activeElement!, { key: "ArrowRight" });
  expect(document.activeElement).toBe(option(/^Choose a Library File…/));
  fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
  expect(document.activeElement).toBe(option(/^Try Sample References/));
  fireEvent.keyDown(document.activeElement!, { key: "ArrowLeft" });
  expect(document.activeElement).toBe(option(/^Find Them on This Computer/));
  fireEvent.keyDown(document.activeElement!, { key: "ArrowUp" });
  expect(document.activeElement).toBe(option(/^Use Zotero/));
  press(option(/^Try Sample References/));
  expect(useSampleLibrary).toHaveBeenCalledExactlyOnceWith(true);
  expect(findSources).not.toHaveBeenCalled();
});

test("one found library replaces Find, keeps its focus, and leaves samples reachable", async () => {
  const openFoundLibrary = vi.fn(async () => undefined);
  render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined,
    useSampleLibrary: async () => undefined, openFoundLibrary,
    findSources: async () => ({ ...found, files: [{ name: "example.bib", folder: "Documents" }] }) }} perform={perform} />);
  press(option(/^Find Them on This Computer/));
  const file = await findOption(/^Use example\.bib/);
  expect(file.textContent).toContain("Found in Documents");
  expect(document.activeElement).toBe(file);
  expect(screen.getAllByRole("option")).toHaveLength(4);
  expect(screen.queryByRole("option", { name: /^Look Again/ })).toBeNull();
  fireEvent.keyDown(file, { key: "ArrowRight" });
  expect(document.activeElement).toBe(option(/^Try Sample References/));
  press(file);
  expect(openFoundLibrary).toHaveBeenCalledExactlyOnceWith(7, 0);
});

test("several found libraries open a short list and return to the same tile", async () => {
  const openFoundLibrary = vi.fn(async () => undefined);
  render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined,
    useSampleLibrary: async () => undefined, openFoundLibrary,
    findSources: async () => ({ ...found, files: [{ name: "one.bib", folder: "Documents" }, { name: "two.bib", folder: "Downloads" }] }) }} perform={perform} />);
  press(option(/^Find Them on This Computer/));
  const files = await findOption(/^Choose a Found Library/);
  expect(screen.getAllByRole("option")).toHaveLength(4);
  press(files);
  await waitFor(() => expect(document.activeElement).toBe(option(/^Use one\.bib/)));
  press(option(/^Use two\.bib/));
  expect(openFoundLibrary).toHaveBeenCalledExactlyOnceWith(7, 1);
  press(option(/^Choose Another Source/));
  await waitFor(() => expect(document.activeElement).toBe(option(/^Choose a Found Library/)));
  expect(option(/^Try Sample References/)).toBeTruthy();
});

const found = { zotero: "available" as const, list: 7, files: [{ name: "研究<literal>.bib", folder: "مراجع" }], cut_short: false };

test("a refused discovery leaves manual choices and retry available", async () => {
  const chooseLibrary = vi.fn(async () => undefined), useZotero = vi.fn(async () => undefined);
  const findSources = vi.fn().mockRejectedValue(new Error("synthetic refusal"));
  const failed = vi.fn();
  const reporting = async (_action: string, run: () => Promise<unknown>) => { try { await run(); } catch { failed(); } };
  render(<LibrarySetup api={{ chooseLibrary, useZotero, findSources }} perform={reporting} />);
  press(option(/^Find Them on This Computer/));
  await waitFor(() => expect(failed).toHaveBeenCalledOnce());
  expect(option(/^Look Again/).getAttribute("aria-disabled")).toBeNull();
  press(option(/^Choose a Library File…/));
  press(option(/^Use Zotero/));
  expect(chooseLibrary).toHaveBeenCalledOnce();
  expect(useZotero).toHaveBeenCalledOnce();
});

test("finding nothing explains the result and keeps retry and samples reachable", async () => {
  const findSources = vi.fn(async () => ({ zotero: "absent" as const, list: 1, files: [], cut_short: false }));
  const useSampleLibrary = vi.fn(async () => undefined);
  render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined,
    findSources, useSampleLibrary }} perform={perform} />);
  press(option(/^Find Them on This Computer/));
  const again = await findOption(/^Look Again/);
  await waitFor(() => expect(again.textContent).toContain("No Zotero or .bib files found nearby. Choose the file yourself."));
  expect(document.activeElement).toBe(again);
  press(again);
  await waitFor(() => expect(findSources).toHaveBeenCalledTimes(2));
  press(option(/^Try Sample References/));
  expect(useSampleLibrary).toHaveBeenCalledExactlyOnceWith(true);
});
