import { act, renderHook } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { LibraryStatus, ZoteroLibrary } from "../../api";
import { useZoteroScope } from "./useZoteroScope";

type Options = Parameters<typeof useZoteroScope>[0];

function library(zotero_libraries: ZoteroLibrary[], source_generation = "1"): LibraryStatus {
  return { source_generation, source_is_requested: true, revision: "1", state: "loaded", has_snapshot: true, name: "Zotero", snapshot_name: "Zotero", references: 1,
    zotero: true, zotero_libraries, snapshot_zotero: true, source_counts: {}, warnings: 0, error: null } as unknown as LibraryStatus;
}

function deferred() {
  let resolve!: () => void, reject!: (error: unknown) => void;
  const promise = new Promise<void>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function setup(calls: ReturnType<typeof deferred>[]) {
  const report = vi.fn();
  const perform = vi.fn(async (operation: string, action: () => Promise<unknown>, isCurrent?: () => boolean) => {
    let failed = false;
    try { await action(); } catch { failed = true; }
    if (!isCurrent || isCurrent()) report(operation, failed);
  });
  const chooseZoteroLibraries = vi.fn(() => calls.shift()!.promise);
  const initial = { library: library(["user"]), libraryVersion: 0, menuOpen: true, nothingMatched: false, chooseZoteroLibraries, report, perform, focusSearch: vi.fn() } as unknown as Options;
  return { report, initial, ...renderHook(useZoteroScope, { initialProps: initial }) };
}

const group = (id: number): ZoteroLibrary => ({ group: id });
const failures = (report: ReturnType<typeof vi.fn>) => report.mock.calls.filter(([name, failed]) => name === "zotero" && failed !== false);

test("an older failed choice cannot clear a newer pending one or report a failure", async () => {
  const [a, b] = [deferred(), deferred()];
  const { result, report } = setup([a, b]);
  act(() => result.current.toggleLibrary(group(1)));
  act(() => result.current.toggleLibrary(group(2)));
  expect(result.current.chosenLibraries).toEqual(["user", group(1), group(2)]);
  await act(async () => { a.reject(new Error("synthetic")); await a.promise.catch(() => undefined); });
  expect(result.current.chosenLibraries).toEqual(["user", group(1), group(2)]);
  expect(failures(report)).toEqual([]);
});

test("the newest choice failing restores what the library reports and says so", async () => {
  const a = deferred();
  const { result, report } = setup([a]);
  act(() => result.current.toggleLibrary(group(1)));
  await act(async () => { a.reject(new Error("synthetic")); await a.promise.catch(() => undefined); });
  expect(result.current.chosenLibraries).toEqual(["user"]);
  expect(failures(report)).toHaveLength(1);
});

test("an older failure after a newer success keeps the choice the library reported", async () => {
  const [a, b] = [deferred(), deferred()];
  const { result, rerender, initial, report } = setup([a, b]);
  act(() => result.current.toggleLibrary(group(1)));
  act(() => result.current.toggleLibrary(group(2)));
  await act(async () => { b.resolve(); await b.promise; });
  rerender({ ...initial, library: library(["user", group(1), group(2)]), libraryVersion: 1 });
  await act(async () => { a.reject(new Error("synthetic")); await a.promise.catch(() => undefined); });
  expect(result.current.chosenLibraries).toEqual(["user", group(1), group(2)]);
  expect(failures(report)).toEqual([]);
});

test("changing source retires a pending choice and its late result", async () => {
  const a = deferred();
  const { result, rerender, initial, report } = setup([a]);
  act(() => result.current.toggleLibrary(group(1)));
  rerender({ ...initial, library: library(["user"], "2"), libraryVersion: 1 });
  expect(result.current.chosenLibraries).toEqual(["user"]);
  await act(async () => { a.reject(new Error("synthetic")); await a.promise.catch(() => undefined); });
  expect(failures(report)).toEqual([]);
});
