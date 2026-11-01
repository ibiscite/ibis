import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import axe from "axe-core";
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

test("initial setup never searches and manual source choices are immediately usable", async () => {
  const chooseLibrary = vi.fn(async () => undefined), useZotero = vi.fn(async () => undefined);
  const findSources = vi.fn(async () => ({ zotero: "absent" as const, list: 1, files: [], cut_short: false }));
  render(<LibrarySetup api={{ chooseLibrary, useZotero, findSources }} perform={perform} />);
  await act(async () => {});
  expect(screen.getByRole("listbox", { name: "Where do your references live?" })).toBeTruthy();
  expect(findSources).not.toHaveBeenCalled();
  expect(option(/^Use Zotero/).textContent).toContain("Your Zotero library, open or closed");
  expect(option(/^Choose a Library File…/).textContent).toContain("BibTeX, BibLaTeX or CSL-JSON");
  press(option(/^Choose a Library File…/));
  press(option(/^Use Zotero/));
  expect(chooseLibrary).toHaveBeenCalledOnce();
  expect(useZotero).toHaveBeenCalledOnce();
  expect(findSources).not.toHaveBeenCalled();
  expect(option(/^Find them on this computer/).getAttribute("aria-disabled")).toBeNull();
});

test("two-row recovery stays a list: arrows move and Enter chooses the focused row", async () => {
  const chooseLibrary = vi.fn(async () => undefined), useZotero = vi.fn(async () => undefined);
  render(<LibrarySetup api={{ chooseLibrary, useZotero }} perform={perform} empty={{ title: "No references in example.bib", zotero: false }} />);
  const zotero = option(/^Use Zotero/);
  act(() => zotero.focus());
  fireEvent.keyDown(zotero, { key: "ArrowDown", code: "ArrowDown" });
  const file = option(/^Choose a Library File…/);
  expect(document.activeElement).toBe(file);
  fireEvent.keyDown(file, { key: "Enter", code: "Enter" });
  fireEvent.keyUp(file, { key: "Enter", code: "Enter" });
  expect(chooseLibrary).toHaveBeenCalledOnce();
  expect(useZotero).not.toHaveBeenCalled();
});

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
  expect(screen.getByText("Ibis finds a reference and pastes its citation where you're typing. It only reads your library, never changes it.")).toBeTruthy();
  expect(screen.getByText(/After this, open Ibis from any app with/)).toBeTruthy();
  expect(container.querySelector('.welcome-mark[aria-hidden="true"]')).toBeTruthy();
  expect(screen.getAllByRole("option")).toHaveLength(4);
  await waitFor(() => expect(document.activeElement).toBe(option(/^Use Zotero/)));
  fireEvent.keyDown(document.activeElement!, { key: "ArrowRight" });
  expect(document.activeElement).toBe(option(/^Choose a Library File…/));
  fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
  expect(document.activeElement).toBe(option(/^Try sample references/));
  fireEvent.keyDown(document.activeElement!, { key: "ArrowLeft" });
  expect(document.activeElement).toBe(option(/^Find them on this computer/));
  fireEvent.keyDown(document.activeElement!, { key: "ArrowUp" });
  expect(document.activeElement).toBe(option(/^Use Zotero/));
  press(option(/^Try sample references/));
  expect(useSampleLibrary).toHaveBeenCalledExactlyOnceWith(true);
  expect(findSources).not.toHaveBeenCalled();
});

test("one found library replaces Find, keeps its focus, and leaves samples reachable", async () => {
  const openFoundLibrary = vi.fn(async () => undefined);
  render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined,
    useSampleLibrary: async () => undefined, openFoundLibrary,
    findSources: async () => ({ ...found, files: [{ name: "example.bib", folder: "Documents" }] }) }} perform={perform} />);
  press(option(/^Find them on this computer/));
  const file = await findOption(/^Use example\.bib/);
  expect(file.textContent).toContain("Found in Documents");
  expect(document.activeElement).toBe(file);
  expect(screen.getAllByRole("option")).toHaveLength(4);
  expect(screen.queryByRole("option", { name: /^Look again/ })).toBeNull();
  fireEvent.keyDown(file, { key: "ArrowRight" });
  expect(document.activeElement).toBe(option(/^Try sample references/));
  press(file);
  expect(openFoundLibrary).toHaveBeenCalledExactlyOnceWith(7, 0);
});

test.each(["macos", "windows", "linux"])("welcome formats the current shortcut for %s", (system) => {
  document.documentElement.dataset.system = system;
  render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined }} perform={perform}
    shortcut={{ shortcut: "Alt+K", registered: true, error: null }} />);
  expect(screen.getByText(`After this, open Ibis from any app with ${system === "macos" ? "⌥K" : "Alt+K"}.`)).toBeTruthy();
});

test("several found libraries open a short list and return to the same tile", async () => {
  const openFoundLibrary = vi.fn(async () => undefined);
  render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined,
    useSampleLibrary: async () => undefined, openFoundLibrary,
    findSources: async () => ({ ...found, files: [{ name: "one.bib", folder: "Documents" }, { name: "two.bib", folder: "Downloads" }] }) }} perform={perform} />);
  press(option(/^Find them on this computer/));
  const files = await findOption(/^Choose a found library/);
  expect(screen.getAllByRole("option")).toHaveLength(4);
  press(files);
  await waitFor(() => expect(document.activeElement).toBe(option(/^Use one\.bib/)));
  press(option(/^Use two\.bib/));
  expect(openFoundLibrary).toHaveBeenCalledExactlyOnceWith(7, 1);
  press(option(/^Choose Another Source/));
  await waitFor(() => expect(document.activeElement).toBe(option(/^Choose a found library/)));
  expect(option(/^Try sample references/)).toBeTruthy();
});

test("closing and reopening drops a pending discovery and focuses the new welcome", async () => {
  let finish!: (value: typeof found) => void;
  const api = { chooseLibrary: async () => undefined, useZotero: async () => undefined,
    findSources: () => new Promise<typeof found>((resolve) => { finish = resolve; }), openFoundLibrary: async () => undefined };
  const first = render(<LibrarySetup api={api} perform={perform} />);
  press(option(/^Find them on this computer/));
  first.unmount();
  render(<LibrarySetup api={api} perform={perform} />);
  await act(async () => finish(found));
  expect(screen.queryByRole("option", { name: /研究/ })).toBeNull();
  expect(option(/^Find them on this computer/)).toBeTruthy();
  expect(document.activeElement).toBe(option(/^Use Zotero/));
});

test.each(["available", "absent", "access_off"] as const)("Zotero %s is told in its row or a note, never as a second suggestion", async (zotero) => {
  render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined,
    findSources: async () => ({ zotero, list: 1, files: [], cut_short: false }) }} perform={perform} />);
  press(option(/^Find them on this computer/));
  await waitFor(() => expect(option(/^Look again/).textContent).not.toContain("Looking"));
  expect(option(/^Use Zotero/).textContent?.includes("Found on this")).toBe(zotero === "available");
  expect(screen.queryByText(/To read your references, in Zotero/) !== null).toBe(zotero === "access_off");
  expect(document.querySelectorAll("[data-suggested]")).toHaveLength(0);
});

const found = { zotero: "available" as const, list: 7, files: [{ name: "研究<literal>.bib", folder: "مراجع" }], cut_short: false };

test("looking for libraries keeps focus while pending and ignores repeated presses", async () => {
  let resolve!: (value: typeof found) => void;
  const findSources = vi.fn().mockResolvedValue(found)
    .mockImplementationOnce(() => new Promise<typeof found>((done) => { resolve = done; }));
  const { unmount } = render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined, findSources }} perform={perform} />);
  press(option(/^Find them on this computer/));
  await waitFor(() => expect(findSources).toHaveBeenCalledTimes(1));
  const pending = option(/^Look again/);
  expect(document.activeElement).toBe(pending);
  expect(pending.textContent).toContain("Looking for Zotero and .bib files…");
  expect(option(/^Use Zotero/).getAttribute("aria-disabled")).toBeNull();
  press(pending);
  expect(findSources).toHaveBeenCalledTimes(1);
  await act(async () => resolve(found));
  const again = option(/^Look again/);
  expect(again.getAttribute("aria-disabled")).toBeNull();
  press(again);
  await waitFor(() => expect(findSources).toHaveBeenCalledTimes(2));
  unmount();
  const accessibility = await axe.run(document.body, { runOnly: { type: "rule", values: ["role-img-alt"] } });
  expect(accessibility.violations.map((violation) => violation.id)).toEqual([]);
});

test("found-file rows preserve list identity and rescan a replaced list", async () => {
  const findSources = vi.fn().mockResolvedValueOnce(found).mockResolvedValue({ ...found, list: 8, files: [{ name: "new.bib", folder: "" }] });
  const openFoundLibrary = vi.fn().mockRejectedValueOnce(new Error("synthetic replaced list")).mockResolvedValue(null);
  const reporting = async (_action: string, run: () => Promise<unknown>) => { try { await run(); } catch { /* Reported by the caller. */ } };
  render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined, findSources, openFoundLibrary }} perform={reporting} />);
  press(option(/^Find them on this computer/));
  const file = await findOption(/研究<literal>\.bib/);
  expect(file.textContent).toContain("Found in مراجع");
  press(file);
  await waitFor(() => expect(openFoundLibrary).toHaveBeenCalledExactlyOnceWith(7, 0));
  const next = await findOption(/^Use new\.bib/);
  press(next);
  await waitFor(() => expect(openFoundLibrary).toHaveBeenLastCalledWith(8, 0));
});

test("a refused discovery leaves manual choices and retry available", async () => {
  const chooseLibrary = vi.fn(async () => undefined), useZotero = vi.fn(async () => undefined);
  const findSources = vi.fn().mockRejectedValue(new Error("synthetic refusal"));
  const failed = vi.fn();
  const reporting = async (_action: string, run: () => Promise<unknown>) => { try { await run(); } catch { failed(); } };
  render(<LibrarySetup api={{ chooseLibrary, useZotero, findSources }} perform={reporting} />);
  press(option(/^Find them on this computer/));
  await waitFor(() => expect(failed).toHaveBeenCalledOnce());
  expect(option(/^Look again/).getAttribute("aria-disabled")).toBeNull();
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
  press(option(/^Find them on this computer/));
  const again = await findOption(/^Look again/);
  await waitFor(() => expect(again.textContent).toContain("No Zotero or .bib files found nearby. Choose the file yourself."));
  expect(document.activeElement).toBe(again);
  press(again);
  await waitFor(() => expect(findSources).toHaveBeenCalledTimes(2));
  press(option(/^Try sample references/));
  expect(useSampleLibrary).toHaveBeenCalledExactlyOnceWith(true);
});

test("a superseded discovery cannot replace the current list", async () => {
  let finish!: (value: typeof found) => void;
  const findSources = vi.fn(() => new Promise<typeof found>((resolve) => { finish = resolve; }));
  const api = { chooseLibrary: async () => undefined, useZotero: async () => undefined, findSources, openFoundLibrary: async () => null };
  const { rerender } = render(<LibrarySetup api={api} perform={perform} />);
  press(option(/^Find them on this computer/));
  await waitFor(() => expect(findSources).toHaveBeenCalledOnce());
  rerender(<LibrarySetup api={{ ...api, findSources: async () => ({ ...found, list: 8, files: [{ name: "current.bib", folder: "" }] }) }} perform={perform} />);
  await findOption(/^Use current\.bib/);
  await act(async () => finish(found));
  expect(screen.queryByRole("option", { name: /研究<literal>\.bib/ })).toBeNull();
  expect(option(/^Use current\.bib/)).toBeTruthy();
});

test("a delayed refusal from an older file choice cannot rescan or replace a newer list", async () => {
  let refuse!: (error: Error) => void;
  const firstSearch = vi.fn(async () => found);
  const nextSearch = vi.fn(async () => ({ ...found, list: 8, files: [{ name: "current.bib", folder: "Documents" }] }));
  const openFoundLibrary = vi.fn().mockImplementationOnce(() => new Promise((_, reject) => { refuse = reject; })).mockResolvedValue(null);
  const api = { chooseLibrary: async () => undefined, useZotero: async () => undefined, findSources: firstSearch, openFoundLibrary };
  const reporting = async (_action: string, run: () => Promise<unknown>) => { try { await run(); } catch { /* The current failure is reported by the caller. */ } };
  const view = render(<LibrarySetup api={api} perform={reporting} />);
  press(option(/^Find them on this computer/));
  press(await findOption(/研究<literal>\.bib/));
  view.rerender(<LibrarySetup api={{ ...api, findSources: nextSearch }} perform={reporting} />);
  const current = await findOption(/^Use current\.bib/);
  press(current);
  await act(async () => refuse(new Error("synthetic delayed refusal")));
  expect(option(/^Use current\.bib/)).toBe(current);
  expect(nextSearch).toHaveBeenCalledOnce();
  expect(openFoundLibrary).toHaveBeenLastCalledWith(8, 0);
});

test("empty-library choices show a known Zotero tag without searching or adding source types", () => {
  const findSources = vi.fn();
  render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined, findSources }}
    perform={perform} empty={{ title: "No references in My Library", zotero: true }} />);
  expect(screen.getByRole("heading", { name: "No references in My Library" })).toBeTruthy();
  expect(screen.getAllByRole("option")).toHaveLength(2);
  expect(option(/^Use Zotero/).textContent).toMatch(/Found on this (Mac|PC|computer)/);
  expect(screen.getByText("Or drag a .bib or CSL-JSON file into this window.")).toBeTruthy();
  expect(findSources).not.toHaveBeenCalled();
});
