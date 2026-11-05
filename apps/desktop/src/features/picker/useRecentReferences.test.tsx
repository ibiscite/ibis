import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { useRecentReferences } from "./useRecentReferences";
import type { recentReferences } from "../../api";

const empty = { revision: "1", results: [] };
const reply: Awaited<ReturnType<typeof recentReferences>> = [empty, false, false];
const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((a, b) => { resolve = a; reject = b; });
  return { promise, resolve, reject };
};

test("a failed native clear stays offered when disabled and a successful retry removes it", async () => {
  let fail = true;
  const native = vi.fn<typeof recentReferences>(async (forget) => {
    if (forget && fail) throw new Error("synthetic I/O failure");
    return [empty, false, fail];
  });
  const { result } = renderHook(() => useRecentReferences(native, null, 0));
  await act(async () => { await result.current.load!(); });
  expect(result.current.forgetFailed).toBe(true);
  act(() => { result.current.forget!(); });
  await waitFor(() => expect(native).toHaveBeenCalledWith(true));
  expect(result.current.forget).toBeTruthy();
  fail = false;
  await act(async () => { result.current.forget!(); });
  expect(result.current.forgetFailed).toBe(false);
  expect(result.current.forget).toBeNull();
});

test("a delayed clear failure cannot replace a newer success", async () => {
  const older = deferred<typeof reply>(), newer = deferred<typeof reply>();
  const native = vi.fn<typeof recentReferences>().mockResolvedValueOnce([empty, true, false]).mockReturnValueOnce(older.promise).mockReturnValueOnce(newer.promise);
  const { result } = renderHook(() => useRecentReferences(native, "invented.writer", 0));
  await act(async () => { await result.current.load!(); });
  const forget = result.current.forget!;
  act(() => { forget(); forget(); });
  await act(async () => { newer.resolve(reply); });
  await act(async () => { older.reject(new Error("older clear")); });
  expect(result.current.forgetFailed).toBe(false);
});

test("a delayed history read after Forget returns no retired rows", async () => {
  const load = deferred<typeof reply>();
  const native = vi.fn<typeof recentReferences>().mockResolvedValueOnce([empty, true, false]).mockReturnValueOnce(load.promise).mockResolvedValue(reply);
  const { result } = renderHook(() => useRecentReferences(native, "invented.writer", 0));
  await act(async () => { await result.current.load!(); });
  const pending = result.current.load!();
  act(() => { result.current.forget!(); });
  await act(async () => { load.resolve([{ ...empty, results: [{ title: "retired" } as never] }, true, false]); });
  expect((await pending).results).toEqual([]);
  expect(result.current.forget).toBeNull();
});

test("a refresh waits for native Forget before reading rows", async () => {
  const clear = deferred<typeof reply>();
  const native = vi.fn<typeof recentReferences>().mockResolvedValueOnce([empty, true, false]).mockReturnValueOnce(clear.promise).mockResolvedValue(reply);
  const { result } = renderHook(() => useRecentReferences(native, "invented.writer", 0));
  await act(async () => { await result.current.load!(); });
  act(() => { result.current.forget!(); });
  const pending = result.current.load!();
  await act(async () => { await Promise.resolve(); });
  expect(native).toHaveBeenCalledTimes(2);
  await act(async () => { clear.resolve(reply); await pending; });
  expect(native).toHaveBeenCalledTimes(3);
  expect(result.current.forget).toBeNull();
});
