import { act, renderHook } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { useRecentReferences } from "./useRecentReferences";

const failing = () => { throw new Error("synthetic I/O failure"); };

test("a failed clear after Recent citations turns off stays offered and is reported", () => {
  const setItem = vi.spyOn(Storage.prototype, "setItem").mockImplementation(failing);
  const props = { enabled: true as boolean | undefined };
  const { result, rerender } = renderHook(({ enabled }) => useRecentReferences(undefined, null, null, 0, enabled), { initialProps: props });
  rerender({ enabled: false });
  expect(result.current.forgetFailed).toBe(true);
  expect(result.current.forget).toBeTruthy();
  setItem.mockRestore();
  let cleared = false;
  act(() => { cleared = result.current.forget!(); });
  expect(cleared).toBe(true);
  expect(result.current.forgetFailed).toBe(false);
  expect(result.current.forget).toBeNull();
});
