import { act, renderHook } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { PickerDestination } from "../../api";
import { useDeliveryTarget } from "./useDeliveryTarget";

const target = (can_paste: boolean): PickerDestination => ({ app: "Example Word", app_id: "example.word", app_identity: null, automatic: null, remembered: null, unreadable: false, can_paste, output_question: null, paste: null, paste_question: false, can_choose_paste: false });

test("observes existing reads only, and a late destination cannot replace the current action", async () => {
  const finish: ((value: PickerDestination) => void)[] = [];
  const pickerDestination = vi.fn(() => new Promise<PickerDestination>((resolve) => { finish.push(resolve); }));
  const api = { pickerDestination };
  const { result } = renderHook(() => useDeliveryTarget(api));
  expect(pickerDestination).not.toHaveBeenCalled();
  let first!: Promise<PickerDestination>, second!: Promise<PickerDestination>;
  act(() => { first = result.current.api.pickerDestination(); second = result.current.api.pickerDestination(); });
  await act(async () => { finish[1]!(target(true)); await second; });
  expect(result.current.canPaste).toBe(true);
  await act(async () => { finish[0]!(target(false)); await first; });
  expect(result.current.canPaste).toBe(true);
  expect(pickerDestination).toHaveBeenCalledTimes(2);
});

test("a new read withdraws Paste immediately, and a failed read keeps Copy", async () => {
  const error = new Error("invented unavailable destination");
  const pickerDestination = vi.fn().mockResolvedValueOnce(target(true)).mockRejectedValueOnce(error);
  const api = { pickerDestination };
  const { result } = renderHook(() => useDeliveryTarget(api));
  await act(async () => { await result.current.api.pickerDestination(); });
  expect(result.current.canPaste).toBe(true);
  await act(async () => { await expect(result.current.api.pickerDestination()).rejects.toBe(error); });
  expect(result.current.canPaste).toBe(false);
});
