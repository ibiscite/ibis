import { act, renderHook } from "@testing-library/react";
import { useState } from "react";
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
  const input = document.createElement("input");
  const select = vi.spyOn(input, "select");
  const { result, rerender } = renderHook(() => {
    const [query, setQuery] = useState("");
    useNativeRequests(api, query, (text) => setQuery(text), { current: input }, 0);
    return query;
  });
  await act(async () => {});
  expect(result.current).toBe("invented lantern");
  expect(select).toHaveBeenCalledTimes(1);
  rerender();
  await act(async () => { request(); });
  expect(result.current).toBe("invented garden");
  expect(select).toHaveBeenCalledTimes(2);
});

test("a rerender while waiting keeps the answer", async () => {
  let answer = (_: string) => {};
  const api = { ...scene([]).api, takeSearchRequest: () => new Promise((resolve) => { answer = resolve; }) } as unknown as PickerApi;
  const { result, rerender } = renderHook(() => {
    const [query, setQuery] = useState("");
    useNativeRequests(api, query, (text) => setQuery(text), { current: null }, 0);
    return query;
  });
  rerender();
  await act(async () => { answer("invented orchard"); });
  expect(result.current).toBe("invented orchard");
});

test("an older reply cannot replace a newer one, or what was typed meanwhile", async () => {
  const answers: ((text: string) => void)[] = [];
  let requested = () => {};
  const api = {
    takeSearchRequest: () => new Promise((resolve) => { answers.push(resolve); }),
    onSearchRequested: async (handler: () => void) => { requested = handler; return () => {}; },
  } as unknown as PickerApi;
  const { result } = renderHook(() => {
    const [query, setQuery] = useState("");
    useNativeRequests(api, query, (text) => setQuery(text), { current: null }, 0);
    return { query, setQuery };
  });
  await act(async () => {});
  await act(async () => { requested(); });
  await act(async () => { answers[1]!("invented newer"); });
  await act(async () => { answers[0]!("invented older"); });
  expect(result.current.query).toBe("invented newer");
  await act(async () => { requested(); });
  act(() => result.current.setQuery("typed"));
  await act(async () => { answers[2]!("invented late"); });
  expect(result.current.query).toBe("typed");
});

test("no waiting text leaves the query alone, and a closed picker ignores a late answer", async () => {
  const { api, unlisten } = scene([]);
  const setQuery = vi.fn();
  const { unmount } = renderHook(() => useNativeRequests(api, "", setQuery, { current: null }, 0));
  unmount();
  await act(async () => {});
  expect(setQuery).not.toHaveBeenCalled();
  expect(unlisten).toHaveBeenCalled();
});

test("only a growing citation ticks", () => {
  const { api } = scene([]);
  const { rerender } = renderHook(({ shelf }) => useNativeRequests(api, "", vi.fn(), { current: null }, shelf), { initialProps: { shelf: 1 } });
  expect(api.alignmentTick).not.toHaveBeenCalled();
  rerender({ shelf: 2 });
  expect(api.alignmentTick).toHaveBeenCalledTimes(1);
  rerender({ shelf: 1 });
  expect(api.alignmentTick).toHaveBeenCalledTimes(1);
});
