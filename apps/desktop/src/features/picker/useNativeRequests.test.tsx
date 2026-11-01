import { act, renderHook } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { PickerApi } from "../../App";
import { useNativeRequests } from "./useNativeRequests";

const scene = (texts: (string | null)[]) => {
  let requested = () => {};
  const unlisten = vi.fn();
  const api = {
    takeSearchRequest: vi.fn(async () => texts.shift() ?? null),
    onSearchRequested: vi.fn(async (handler: () => void) => { requested = handler; return unlisten; }),
    alignmentTick: vi.fn(async () => null),
  } as unknown as PickerApi;
  return { api, unlisten, request: () => requested() };
};

test("text waiting from a launch and text sent later both become the query", async () => {
  const { api, request } = scene(["invented lantern", "invented garden"]);
  const setQuery = vi.fn();
  renderHook(() => useNativeRequests(api, setQuery, 0));
  await act(async () => {});
  expect(setQuery).toHaveBeenLastCalledWith("invented lantern");
  await act(async () => { request(); });
  expect(setQuery).toHaveBeenLastCalledWith("invented garden");
});

test("no waiting text leaves the query alone, and a closed picker ignores a late answer", async () => {
  const { api, unlisten } = scene([]);
  const setQuery = vi.fn();
  const { unmount } = renderHook(() => useNativeRequests(api, setQuery, 0));
  unmount();
  await act(async () => {});
  expect(setQuery).not.toHaveBeenCalled();
  expect(unlisten).toHaveBeenCalled();
});

test("only a growing citation ticks", () => {
  const { api } = scene([]);
  const { rerender } = renderHook(({ shelf }) => useNativeRequests(api, vi.fn(), shelf), { initialProps: { shelf: 1 } });
  expect(api.alignmentTick).not.toHaveBeenCalled();
  rerender({ shelf: 2 });
  expect(api.alignmentTick).toHaveBeenCalledTimes(1);
  rerender({ shelf: 1 });
  expect(api.alignmentTick).toHaveBeenCalledTimes(1);
});
