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
