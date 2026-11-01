import { act, renderHook } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type * as nativeApi from "../../api";
import { testStates } from "../../test-fixtures";
import { useLibrarySearch } from "./useLibrarySearch";

test("a same-query retry after failure is pending until its own response arrives", async () => {
  let finish!: (value: Awaited<ReturnType<typeof nativeApi.search>>) => void;
  const search = vi.fn<typeof nativeApi.search>().mockRejectedValueOnce(new Error("invented failure"))
    .mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
  const library = { ...testStates[0]!.library, revision: "1" };
  const setActive = vi.fn(), refreshSnapshot = vi.fn();
  const { result, rerender, unmount } = renderHook(({ library }) => useLibrarySearch({ search, library,
    statusFailed: false, libraryVersion: 0, query: "same query", interpretLocators: true, setActive, refreshSnapshot }), { initialProps: { library } });
  await act(async () => { await Promise.resolve(); });
  expect(result.current.error).toBeTruthy();
  expect(result.current.pending).toBe(false);
  rerender({ library: { ...library, revision: "2" } });
  expect(result.current.pending).toBe(true);
  await act(async () => { finish({ revision: "2", results: [] }); });
  expect(result.current.pending).toBe(false);
  expect(result.current.error).toBeNull();
  unmount();
});

test.each([false, true])("a successful reply stays pending until its revision is reconciled (empty=%s)", async (empty) => {
  const reply = { revision: "2", results: empty ? [] : [{ ...testStates[1]!.results[0]!, revision: "2" }] };
  let finish!: (value: typeof reply) => void;
  const search = vi.fn<typeof nativeApi.search>().mockResolvedValueOnce(reply)
    .mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
  const library = { ...testStates[0]!.library, revision: "1" };
  const refreshSnapshot = vi.fn(), setActive = vi.fn();
  const { result, rerender, unmount } = renderHook(({ library }) => useLibrarySearch({ search, library,
    statusFailed: false, libraryVersion: 0, query: "same query", interpretLocators: true, setActive, refreshSnapshot }), { initialProps: { library } });
  await act(async () => { await Promise.resolve(); });
  expect(refreshSnapshot).toHaveBeenCalledOnce();
  expect(result.current.ready).toBe(false);
  expect(result.current.pending).toBe(true);
  rerender({ library: { ...library, revision: "2" } });
  expect(result.current.pending).toBe(true);
  await act(async () => { finish(reply); });
  expect(result.current.pending).toBe(false);
  expect(result.current.ready).toBe(true);
  expect(result.current.results).toEqual(reply.results);
  unmount();
});

test("a failed status read ends reconciliation until a later update recovers the search", async () => {
  const reply = { revision: "2", results: [{ ...testStates[1]!.results[0]!, revision: "2" }] };
  const search = vi.fn<typeof nativeApi.search>().mockResolvedValue(reply);
  const library = { ...testStates[0]!.library, revision: "1" };
  const refreshSnapshot = vi.fn(), setActive = vi.fn();
  const { result, rerender, unmount } = renderHook(({ library, statusFailed }) => useLibrarySearch({ search, library, statusFailed,
    libraryVersion: 0, query: "same query", interpretLocators: true, setActive, refreshSnapshot }), { initialProps: { library, statusFailed: false } });
  await act(async () => { await Promise.resolve(); });
  expect(result.current.pending).toBe(true);
  rerender({ library, statusFailed: true });
  expect(result.current.pending).toBe(false);
  expect(result.current.ready).toBe(false);
  rerender({ library: { ...library, revision: "2" }, statusFailed: false });
  await act(async () => { await Promise.resolve(); });
  expect(result.current.ready).toBe(true);
  expect(result.current.results).toEqual(reply.results);
  expect(result.current.pending).toBe(false);
  unmount();
});

test("reopening replaces an unfinished search and ignores its late reply", async () => {
  const finishes: ((value: Awaited<ReturnType<typeof nativeApi.search>>) => void)[] = [];
  const search = vi.fn<typeof nativeApi.search>(() => new Promise((resolve) => finishes.push(resolve)));
  const library = { ...testStates[0]!.library, revision: "1" };
  const { result, rerender, unmount } = renderHook(({ sessionId }) => useLibrarySearch({ search, library,
    sessionId, query: "same query", interpretLocators: true, statusFailed: false, libraryVersion: 0,
    setActive: vi.fn(), refreshSnapshot: vi.fn() }), { initialProps: { sessionId: 0 } });
  rerender({ sessionId: 1 });
  await act(async () => { finishes[1]!({ revision: "1", results: [{ ...testStates[1]!.results[0]!, revision: "1" }] }); });
  const shown = result.current.results;
  await act(async () => { finishes[0]!({ revision: "1", results: [] }); });
  expect(result.current.results).toBe(shown);
  expect(result.current.ready).toBe(true);
  expect(result.current.pending).toBe(false);
  unmount();
});
