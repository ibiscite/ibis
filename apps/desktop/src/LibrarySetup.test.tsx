import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import axe from "axe-core";
import { LibrarySetup, WritingSetup } from "./LibrarySetup";

afterEach(cleanup);
const perform = async (_action: string, run: () => Promise<unknown>) => { await run(); };
const option = (name: RegExp) => screen.getByRole("option", { name });
const findOption = (name: RegExp) => screen.findByRole("option", { name });
const press = (row: HTMLElement) => {
  row.focus();
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
  press(option(/^BibTeX or BibLaTeX file/));
  press(option(/^Zotero/));
  expect(chooseLibrary).toHaveBeenCalledOnce();
  expect(useZotero).toHaveBeenCalledOnce();
  expect(findSources).not.toHaveBeenCalled();
  expect(option(/^Look for libraries on this computer/).getAttribute("aria-disabled")).toBeNull();
});

test("arrow keys move between rows and Enter chooses the focused one", async () => {
  const chooseLibrary = vi.fn(async () => undefined), useZotero = vi.fn(async () => undefined);
  render(<LibrarySetup api={{ chooseLibrary, useZotero }} perform={perform} />);
  const zotero = option(/^Zotero/);
  act(() => zotero.focus());
  fireEvent.keyDown(zotero, { key: "ArrowDown", code: "ArrowDown" });
  const file = option(/^BibTeX or BibLaTeX file/);
  expect(document.activeElement).toBe(file);
  fireEvent.keyDown(file, { key: "Enter", code: "Enter" });
  fireEvent.keyUp(file, { key: "Enter", code: "Enter" });
  expect(chooseLibrary).toHaveBeenCalledOnce();
  expect(useZotero).not.toHaveBeenCalled();
});

test.each(["available", "absent", "access_off"] as const)("Zotero %s is told in its row or a note, never as a second suggestion", async (zotero) => {
  render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined,
    findSources: async () => ({ zotero, list: 1, files: [], cut_short: false }) }} perform={perform} />);
  press(option(/^Look for libraries on this computer/));
  await waitFor(() => expect(option(/^Look again/).textContent).not.toContain("Looking"));
  expect(option(/^Zotero/).textContent?.includes("Found on this")).toBe(zotero === "available");
  expect(screen.queryByText(/does not let other apps read it/) !== null).toBe(zotero === "access_off");
  expect(document.querySelectorAll("[data-suggested]")).toHaveLength(0);
});

const found = { zotero: "available" as const, list: 7, files: [{ name: "研究<literal>.bib", folder: "مراجع" }], cut_short: false };

test("looking for libraries keeps focus while pending and ignores repeated presses", async () => {
  let resolve!: (value: typeof found) => void;
  const findSources = vi.fn().mockResolvedValue(found)
    .mockImplementationOnce(() => new Promise<typeof found>((done) => { resolve = done; }));
  const { unmount } = render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined, findSources }} perform={perform} />);
  press(option(/^Look for libraries on this computer/));
  await waitFor(() => expect(findSources).toHaveBeenCalledTimes(1));
  const pending = option(/^Look again/);
  expect(document.activeElement).toBe(pending);
  expect(pending.textContent).toContain("Looking for Zotero and .bib files…");
  expect(option(/^Zotero/).getAttribute("aria-disabled")).toBeNull();
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
  press(option(/^Look for libraries on this computer/));
  const file = await findOption(/研究<literal>\.bib/);
  expect(file.querySelector(".choice-tag")?.textContent).toBe("مراجع");
  press(file);
  await waitFor(() => expect(openFoundLibrary).toHaveBeenCalledExactlyOnceWith(7, 0));
  const next = await findOption(/^new\.bib/);
  press(next);
  await waitFor(() => expect(openFoundLibrary).toHaveBeenLastCalledWith(8, 0));
});

test("a refused discovery leaves manual choices and retry available", async () => {
  const chooseLibrary = vi.fn(async () => undefined), useZotero = vi.fn(async () => undefined);
  const findSources = vi.fn().mockRejectedValue(new Error("synthetic refusal"));
  const failed = vi.fn();
  const reporting = async (_action: string, run: () => Promise<unknown>) => { try { await run(); } catch { failed(); } };
  render(<LibrarySetup api={{ chooseLibrary, useZotero, findSources }} perform={reporting} />);
  press(option(/^Look for libraries on this computer/));
  await waitFor(() => expect(failed).toHaveBeenCalledOnce());
  expect(option(/^Look again/).getAttribute("aria-disabled")).toBeNull();
  press(option(/^BibTeX or BibLaTeX file/));
  press(option(/^Zotero/));
  expect(chooseLibrary).toHaveBeenCalledOnce();
  expect(useZotero).toHaveBeenCalledOnce();
});

test("a superseded discovery cannot replace the current list", async () => {
  let finish!: (value: typeof found) => void;
  const findSources = vi.fn(() => new Promise<typeof found>((resolve) => { finish = resolve; }));
  const api = { chooseLibrary: async () => undefined, useZotero: async () => undefined, findSources, openFoundLibrary: async () => null };
  const { rerender } = render(<LibrarySetup api={api} perform={perform} />);
  press(option(/^Look for libraries on this computer/));
  await waitFor(() => expect(findSources).toHaveBeenCalledOnce());
  rerender(<LibrarySetup api={{ ...api, findSources: async () => ({ ...found, list: 8, files: [{ name: "current.bib", folder: "" }] }) }} perform={perform} />);
  await findOption(/^current\.bib/);
  await act(async () => finish(found));
  expect(screen.queryByRole("option", { name: /研究<literal>\.bib/ })).toBeNull();
  expect(option(/^current\.bib/)).toBeTruthy();
});

test("the writing step offers three places, marks the current format and reports the choice", () => {
  const choose = vi.fn();
  const { rerender } = render(<WritingSetup format="pandoc" choose={choose} shortcut="⌥Space" />);
  const rows = screen.getAllByRole("option");
  expect(rows.map((row) => row.querySelector(".result-title")?.textContent)).toEqual(["Markdown (Pandoc)", "LaTeX", "Typst"]);
  expect(rows[0]!.textContent).toContain("Now");
  expect(rows[1]!.textContent).toContain("\\cite{smith2020}");
  expect(screen.getByText("Open Ibis from any app with ⌥Space.")).toBeTruthy();
  press(rows[0]!);
  expect(choose).toHaveBeenCalledExactlyOnceWith("pandoc");
  press(rows[1]!);
  expect(choose).toHaveBeenLastCalledWith("cite");
  rerender(<WritingSetup format="typst" choose={choose} />);
  expect(option(/^Typst/).textContent).toContain("Now");
  expect(screen.queryByText(/Open Ibis from any app/)).toBeNull();
});


test("empty-library choices show a known Zotero tag without searching or adding source types", () => {
  const findSources = vi.fn();
  render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined, findSources }}
    perform={perform} empty={{ title: "No references in My Library", zotero: true }} />);
  expect(screen.getByRole("heading", { name: "No references in My Library" })).toBeTruthy();
  expect(screen.getAllByRole("option")).toHaveLength(2);
  expect(option(/^Zotero/).textContent).toMatch(/Found on this (Mac|PC|computer)/);
  expect(screen.getByText("You can drop a file here too.")).toBeTruthy();
  expect(findSources).not.toHaveBeenCalled();
});
