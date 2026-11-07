import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { usePickerPreferences } from "./usePickerPreferences";
import type { PickerPreferences } from "../../api";
import type { PickerPreferencesPatch } from "../../bindings";

/** The settings owner as the picker sees it: each choice gets a receipt when
 * made, and `save` answers for it by patch and receipt. */
function owner(save: (patch: Partial<PickerPreferences>, receipt: number) => Promise<{ preferences: PickerPreferences }>) {
  const sent = new Map<number, Partial<PickerPreferences>>();
  let receipts = 0;
  return {
    savePickerPreferences: async (patch: PickerPreferencesPatch) => { const receipt = ++receipts; sent.set(receipt, patch as Partial<PickerPreferences>); return receipt; },
    pickerPreferencesSaved: (receipt: number) => save(sent.get(receipt)!, receipt),
  };
}
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const preferences: PickerPreferences = { output: "pandoc", density: "comfortable", larger_text: false, paste: true, menu_bar: true };
const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
};
const context = { presentation: 0, styles: true, report: vi.fn(), perform: async (_operation: unknown, action: () => Promise<unknown>) => { try { await action(); } catch {} } };

test("an older Settings notification read cannot restore the wrong global output", async () => {
  const first = deferred<PickerPreferences>();
  const second = deferred<PickerPreferences>();
  const reads = [first.promise, second.promise];
  const { result } = renderHook(() => usePickerPreferences({
    pickerPreferences: () => reads.shift()!,
    ...owner(vi.fn(async () => ({ preferences }))), quit: vi.fn(async () => undefined),
  }, context));
  act(() => result.current.loadPreferences(() => true));
  act(() => result.current.loadPreferences(() => true));
  await act(async () => { second.resolve({ ...preferences, output: "cite" }); });
  expect(result.current.format).toBe("cite");
  await act(async () => { first.resolve({ ...preferences, output: "apa" }); });
  expect(result.current.format).toBe("cite");
});

test("a global key choice during a stale Settings read wins and sends only its field patch", async () => {
  const old = deferred<PickerPreferences>();
  const save = deferred<{ preferences: PickerPreferences }>();
  const setPickerPreferences = vi.fn(() => save.promise);
  let reading = false;
  const { result } = renderHook(() => usePickerPreferences({
    pickerPreferences: async () => reading ? old.promise : { ...preferences, output: "apa" },
    ...owner(setPickerPreferences), quit: async () => undefined,
  }, context));
  await act(async () => { result.current.loadPreferences(() => true); });
  reading = true;
  act(() => result.current.loadPreferences(() => true));
  act(() => result.current.chooseFormat("cite"));
  await act(async () => { save.resolve({ preferences: { ...preferences, output: "cite" } }); });
  await act(async () => { old.resolve({ ...preferences, output: "apa" }); });
  expect(result.current.format).toBe("cite");
  expect(setPickerPreferences).toHaveBeenCalledWith({ output: "cite" }, 1);
});

test("a queued view save never replays an output acknowledged before it reaches the native lock", async () => {
  const output = deferred<{ preferences: PickerPreferences }>();
  const setPickerPreferences = vi.fn()
    .mockImplementationOnce(() => output.promise)
    .mockImplementationOnce(async (patch: Partial<PickerPreferences>) => ({
      // The other window changed the global style after the first write.
      preferences: { ...preferences, output: "ieee", ...patch },
    }));
  const { result } = renderHook(() => usePickerPreferences({
    pickerPreferences: async () => ({ ...preferences, output: "apa" }), ...owner(setPickerPreferences), quit: async () => undefined }, context));
  await act(async () => { result.current.loadPreferences(() => true); });
  act(() => result.current.chooseFormat("cite"));
  act(() => result.current.toggleLargerText());
  await act(async () => { output.resolve({ preferences: { ...preferences, output: "cite" } }); });
  expect(setPickerPreferences.mock.calls[1]).toEqual([{ larger_text: true }, 2]);
  expect(result.current.format).toBe("ieee");
});

test("a failed later output stays applied in the picker and is never sent again", async () => {
  const first = deferred<void>();
  let state = { ...preferences, output: "apa" } as PickerPreferences;
  let writes = 0;
  const setPickerPreferences = vi.fn(async (patch: Partial<PickerPreferences>) => {
    const request = ++writes;
    if (request === 1) await first.promise;
    if (request === 3) throw new Error("synthetic third-save failure");
    state = { ...state, ...patch };
    return { preferences: state };
  });
  const { result } = renderHook(() => usePickerPreferences({ pickerPreferences: async () => state,
    ...owner(setPickerPreferences), quit: async () => undefined }, context));
  await act(async () => { result.current.loadPreferences(() => true); });
  act(() => result.current.chooseFormat("cite"));
  act(() => result.current.chooseFormat("ieee"));
  act(() => result.current.chooseFormat("typst"));
  await act(async () => { first.resolve(); });
  expect(writes).toBe(3);
  expect(state.output).toBe("ieee");
  await act(async () => { result.current.toggleLargerText(); });
  // The next choice carries only its own field: the saved IEEE is never written over later.
  expect(setPickerPreferences.mock.calls.at(-1)?.[0]).toEqual({ larger_text: true });
  expect(state.output).toBe("ieee");
  expect(result.current.format).toBe("typst");
});
