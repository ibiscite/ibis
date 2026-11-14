import type * as nativeApi from "../../api";
import { act, renderHook } from "@testing-library/react";
import { useState } from "react";
import { expect, test, vi } from "vitest";
import { useNativeRequests } from "./useNativeRequests";

const deferred = () => {
  let resolve = () => {};
  const promise = new Promise<void>((done) => { resolve = done; });
  return { promise, resolve };
};

const scene = (pending: string | null = null, registration = Promise.resolve(), reply = Promise.resolve()) => {
  let requested = () => {};
  const unlisten = vi.fn();
  const api = {
    takeSearchRequest: vi.fn(async () => { const text = pending; pending = null; await reply; return text; }),
    onSearchRequested: vi.fn(async (handler: () => void) => { await registration; requested = handler; return unlisten; }),
    alignmentTick: vi.fn(async () => null),
  } as unknown as typeof nativeApi;
  return { api, unlisten, request: () => requested(), send: (text: string) => { pending = text; requested(); } };
};

test("launch text arrives intact once, and reopening does not replay it", async () => {
  const { api, request, send } = scene("雪 & café? + %20");
  const input = document.createElement("input");
  const select = vi.spyOn(input, "select");
  const picker = () => {
    const [query, setQuery] = useState("");
    useNativeRequests(api, query, (text) => setQuery(text), { current: input }, 0);
    return query;
  };
  const { result, rerender, unmount } = renderHook(picker);
  await act(async () => {});
  expect(result.current).toBe("雪 & café? + %20");
  expect(select).toHaveBeenCalledTimes(1);
  rerender();
  await act(async () => { send("invented garden"); request(); });
  expect(result.current).toBe("invented garden");
  expect(select).toHaveBeenCalledTimes(2);
  unmount();
  const reopened = renderHook(picker);
  await act(async () => {});
  expect(reopened.result.current).toBe("");
  expect(select).toHaveBeenCalledTimes(2);
});

test.each([false, true])("a search during listener registration respects newer typing (%s)", async (typing) => {
  const registration = deferred();
  const { api, send } = scene(null, registration.promise);
  const { result } = renderHook(() => {
    const [query, setQuery] = useState("");
    useNativeRequests(api, query, setQuery, { current: null }, 0);
    return { query, setQuery };
  });
  await act(async () => { send("invented between"); });
  if (typing) act(() => result.current.setQuery("typed"));
  await act(async () => { registration.resolve(); });
  expect(result.current.query).toBe(typing ? "typed" : "invented between");
});

test("overlapping notifications cannot discard a destructively consumed search", async () => {
  const first = deferred(), second = deferred();
  const { api, request } = scene();
  const { result } = renderHook(() => {
    const [query, setQuery] = useState("");
    useNativeRequests(api, query, setQuery, { current: null }, 0);
    return query;
  });
  await act(async () => {});
  vi.mocked(api.takeSearchRequest)
    .mockImplementationOnce(async () => { await first.promise; return "invented older"; })
    .mockImplementationOnce(async () => { await second.promise; return "invented newest"; });
  await act(async () => { request(); request(); });
  await act(async () => { second.resolve(); });
  expect(result.current).toBe("");
  await act(async () => { first.resolve(); });
  expect(result.current).toBe("invented newest");
});

test("a rerender while waiting keeps the answer", async () => {
  let answer = (_: string) => {};
  const api = { ...scene().api, takeSearchRequest: () => new Promise((resolve) => { answer = resolve; }) } as unknown as typeof nativeApi;
  const { result, rerender } = renderHook(() => {
    const [query, setQuery] = useState("");
    useNativeRequests(api, query, (text) => setQuery(text), { current: null }, 0);
    return query;
  });
  rerender();
  await act(async () => {});
  await act(async () => { answer("invented orchard"); });
  expect(result.current).toBe("invented orchard");
});

test("a failed read keeps the current search and a later notification still works", async () => {
  const { api, send } = scene();
  vi.mocked(api.takeSearchRequest).mockRejectedValueOnce(new Error("invented IPC failure"));
  const { result } = renderHook(() => {
    const [query, setQuery] = useState("invented current");
    useNativeRequests(api, query, setQuery, { current: null }, 0);
    return query;
  });
  await act(async () => {});
  expect(result.current).toBe("invented current");
  await act(async () => { send("invented recovery"); });
  expect(result.current).toBe("invented recovery");
});

test.each(["typed", ""])("typing wins over a delayed native reply even when it ends at %j", async (final) => {
  const reply = deferred();
  const { api } = scene("invented late", Promise.resolve(), reply.promise);
  const { result } = renderHook(() => {
    const [query, setQuery] = useState("");
    useNativeRequests(api, query, (text) => setQuery(text), { current: null }, 0);
    return { query, setQuery };
  });
  await act(async () => {});
  act(() => result.current.setQuery("typed"));
  act(() => result.current.setQuery(final));
  await act(async () => { reply.resolve(); });
  expect(result.current.query).toBe(final);
});

test("typing after a queued notification wins before its read can start", async () => {
  const reply = deferred();
  const { api, send } = scene(null, Promise.resolve(), reply.promise);
  const { result } = renderHook(() => {
    const [query, setQuery] = useState("");
    useNativeRequests(api, query, setQuery, { current: null }, 0);
    return { query, setQuery };
  });
  await act(async () => {});
  await act(async () => { send("invented queued"); });
  act(() => result.current.setQuery("typed"));
  await act(async () => { reply.resolve(); });
  expect(result.current.query).toBe("typed");
});

test.each(["subscribing", "consuming"])("closing while %s ignores late text and removes the listener", async (phase) => {
  const gate = deferred();
  const { api, unlisten } = scene("invented late", phase === "subscribing" ? gate.promise : Promise.resolve(), gate.promise);
  const setQuery = vi.fn();
  const { unmount } = renderHook(() => useNativeRequests(api, "", setQuery, { current: null }, 0));
  await act(async () => {});
  unmount();
  await act(async () => { gate.resolve(); });
  expect(setQuery).not.toHaveBeenCalled();
  expect(unlisten).toHaveBeenCalled();
});

test("only a growing citation ticks", () => {
  const { api } = scene();
  const { rerender } = renderHook(({ shelf }) => useNativeRequests(api, "", vi.fn(), { current: null }, shelf), { initialProps: { shelf: 1 } });
  expect(api.alignmentTick).not.toHaveBeenCalled();
  rerender({ shelf: 2 });
  expect(api.alignmentTick).toHaveBeenCalledTimes(1);
  rerender({ shelf: 1 });
  expect(api.alignmentTick).toHaveBeenCalledTimes(1);
});
