import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { usePickerPreferences } from "./usePickerPreferences";
import type { PickerPreferences } from "../../api";

afterEach(cleanup);
const preferences: PickerPreferences = { citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true, search_mark: false, paste: true };
const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
};
const context = { styles: true, report: vi.fn(), perform: async (_operation: unknown, action: () => Promise<unknown>) => { await action(); } };

test("an older Settings notification read cannot restore the wrong global output", async () => {
  const first = deferred<PickerPreferences>();
  const second = deferred<PickerPreferences>();
  const reads = [first.promise, second.promise];
  const { result } = renderHook(() => usePickerPreferences({
    copyFormat: async () => "cite", pickerPreferences: () => reads.shift()!,
    setPickerPreferences: vi.fn(async () => ({ preferences, copy_format: "cite" as const })), quit: vi.fn(async () => undefined),
  }, context));
  act(() => result.current.loadPreferences(() => true));
  act(() => result.current.loadPreferences(() => true));
  await act(async () => { second.resolve(preferences); });
  expect(result.current.format).toBe("cite");
  await act(async () => { first.resolve({ ...preferences, citation_style: "apa" }); });
  expect(result.current.format).toBe("cite");
});

test("a global key choice during a stale Settings read wins and sends only its field patch", async () => {
  const old = deferred<PickerPreferences>();
  const save = deferred<{ preferences: PickerPreferences; copy_format: "cite" }>();
  const setPickerPreferences = vi.fn(() => save.promise);
  let reading = false;
  const { result } = renderHook(() => usePickerPreferences({
    copyFormat: async () => "pandoc", pickerPreferences: async () => reading ? old.promise : { ...preferences, citation_style: "apa" },
    setPickerPreferences, quit: async () => undefined,
  }, context));
  await act(async () => { result.current.loadPreferences(() => true); });
  reading = true;
  act(() => result.current.loadPreferences(() => true));
  act(() => result.current.chooseFormat("cite"));
  await act(async () => { save.resolve({ preferences, copy_format: "cite" }); });
  await act(async () => { old.resolve({ ...preferences, citation_style: "apa" }); });
  expect(result.current.format).toBe("cite");
  expect(setPickerPreferences).toHaveBeenCalledWith({ citation_style: null }, "cite");
});

test("a queued view save never replays an output acknowledged before it reaches the native lock", async () => {
  const output = deferred<{ preferences: PickerPreferences; copy_format: "cite" }>();
  const setPickerPreferences = vi.fn()
    .mockImplementationOnce(() => output.promise)
    .mockImplementationOnce(async (patch: Partial<PickerPreferences>) => ({
      // The other window changed the global style after the first write.
      preferences: { ...preferences, citation_style: "ieee", ...patch }, copy_format: "cite",
    }));
  const { result } = renderHook(() => usePickerPreferences({ copyFormat: async () => "pandoc",
    pickerPreferences: async () => ({ ...preferences, citation_style: "apa" }), setPickerPreferences, quit: async () => undefined }, context));
  await act(async () => { result.current.loadPreferences(() => true); });
  act(() => result.current.chooseFormat("cite"));
  act(() => result.current.toggleLargerText());
  await act(async () => { output.resolve({ preferences, copy_format: "cite" }); });
  expect(setPickerPreferences.mock.calls[1]).toEqual([{ larger_text: true }, null]);
  expect(result.current.format).toBe("style:ieee");
});
