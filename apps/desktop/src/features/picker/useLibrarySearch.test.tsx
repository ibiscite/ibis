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
    libraryVersion: 0, query: "same query", interpretLocators: true, setActive, refreshSnapshot }), { initialProps: { library } });
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
    libraryVersion: 0, query: "same query", interpretLocators: true, setActive, refreshSnapshot }), { initialProps: { library } });
  await act(async () => { await Promise.resolve(); });
  expect(refreshSnapshot).toHaveBeenCalledOnce();
  expect(result.current.current).toEqual([]);
  expect(result.current.pending).toBe(true);
  rerender({ library: { ...library, revision: "2" } });
  expect(result.current.pending).toBe(true);
  await act(async () => { finish(reply); });
  expect(result.current.pending).toBe(false);
  expect(result.current.current).toEqual(reply.results);
  unmount();
});
