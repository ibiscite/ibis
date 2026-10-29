import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { RECHECK_MS, usePastePermission } from "./PasteGuide";

afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); });
const denied = { pastes: true, needed: true, granted: false, list: "Accessibility" };

test("permission reads pause on other pages and while hidden", async () => {
  vi.useFakeTimers();
  const api = { pastePermission: vi.fn(async () => denied) };
  const { rerender } = renderHook(({ active }) => usePastePermission(api, active), { initialProps: { active: false } });
  await act(async () => { window.dispatchEvent(new Event("focus")); await vi.advanceTimersByTimeAsync(RECHECK_MS * 2); });
  expect(api.pastePermission).not.toHaveBeenCalled();
  await act(async () => rerender({ active: true }));
  expect(api.pastePermission).toHaveBeenCalled();
  const reads = api.pastePermission.mock.calls.length;
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  await act(async () => { document.dispatchEvent(new Event("visibilitychange")); });
  expect(vi.getTimerCount()).toBe(0);
  await act(async () => { window.dispatchEvent(new Event("focus")); await vi.advanceTimersByTimeAsync(RECHECK_MS * 2); });
  expect(api.pastePermission).toHaveBeenCalledTimes(reads);
  hidden.mockReturnValue(false);
  await act(async () => { document.dispatchEvent(new Event("visibilitychange")); });
  expect(api.pastePermission).toHaveBeenCalledTimes(reads + 1);
  rerender({ active: false });
  await act(async () => { await vi.advanceTimersByTimeAsync(RECHECK_MS * 2); });
  expect(api.pastePermission).toHaveBeenCalledTimes(reads + 1);
});

test("pasting switched off reads capabilities once and on return without polling", async () => {
  vi.useFakeTimers();
  const api = { pastePermission: vi.fn(async () => denied) };
  renderHook(() => usePastePermission(api, true, false));
  await act(async () => { await Promise.resolve(); });
  expect(api.pastePermission).toHaveBeenCalledOnce();
  await act(async () => { await vi.advanceTimersByTimeAsync(RECHECK_MS * 3); });
  expect(api.pastePermission).toHaveBeenCalledOnce();
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  expect(api.pastePermission).toHaveBeenCalledTimes(2);
});

test("a stale permission read cannot replace the grant read on return", async () => {
  let finish!: (permission: typeof denied) => void;
  const api = { pastePermission: vi.fn().mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }))
    .mockResolvedValue({ ...denied, granted: true }) };
  const { result } = renderHook(() => usePastePermission(api, true));
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  expect(result.current?.granted).toBe(true);
  await act(async () => finish(denied));
  expect(result.current?.granted).toBe(true);
});

test("a failed read does not claim permission was granted", async () => {
  const api = { pastePermission: vi.fn().mockResolvedValueOnce(denied).mockRejectedValue(new Error("synthetic failure")) };
  const { result } = renderHook(() => usePastePermission(api, true));
  await act(async () => { await Promise.resolve(); });
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  expect(result.current?.granted).toBe(false);
});
