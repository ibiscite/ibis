import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { useDestination } from "./useDestination";
import type { PickerDestination } from "../../api";

afterEach(cleanup);
const browser: PickerDestination = { app: "Example Browser", app_id: "example.browser", automatic: null, remembered: null, unreadable: false, output_question: "browser" };
const pending = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
};
function options() {
  return { fallback: "pandoc" as const, chooseDefault: vi.fn(), report: vi.fn() };
}

test("a browser choice is once-only and never changes another app's default", async () => {
  const settings = options();
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => browser, rememberAppOutput: vi.fn() }, settings));
  act(() => result.current.read());
  await waitFor(() => expect(result.current.ready).toBe(true));
  act(() => result.current.toggleOnce());
  act(() => result.current.choose("cite"));
  expect(result.current.format).toBe("cite");
  expect(settings.chooseDefault).not.toHaveBeenCalled();
  act(() => result.current.read());
  await waitFor(() => expect(result.current.format).toBe("pandoc"));
});

test("a choice made during a slow destination read wins over that read", async () => {
  const read = pending<PickerDestination>();
  const settings = options();
  const { result } = renderHook(() => useDestination({ pickerDestination: () => read.promise, rememberAppOutput: vi.fn() }, settings));
  act(() => result.current.read());
  act(() => result.current.choose("typst"));
  await act(async () => read.resolve({ ...browser, remembered: { key: "cite" } }));
  expect(result.current.format).toBe("typst");
  expect(settings.chooseDefault).not.toHaveBeenCalled();
});

test("a failed save leaves the choice once-only and does not claim it was remembered", async () => {
  const settings = options();
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => browser,
    rememberAppOutput: vi.fn(async () => { throw new Error("synthetic save failure"); }) }, settings));
  act(() => result.current.read());
  await waitFor(() => expect(result.current.ready).toBe(true));
  await act(async () => result.current.choose("cite"));
  expect(result.current.remembered).toBe(false);
  expect(result.current.format).toBe("cite");
  expect(settings.report).toHaveBeenCalledWith("appChoice");
});

test("a stale save completion cannot mark a different app as remembered", async () => {
  let destination = browser;
  const save = pending<null>();
  const settings = options();
  const rememberAppOutput = vi.fn(() => save.promise);
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => destination, rememberAppOutput }, settings));
  act(() => result.current.read());
  await waitFor(() => expect(result.current.ready).toBe(true));
  act(() => result.current.choose("cite"));
  destination = { ...browser, app: "Example Editor", app_id: "example.editor" };
  act(() => result.current.read());
  await act(async () => save.resolve(null));
  await waitFor(() => expect(result.current.app).toBe("Example Editor"));
  expect(result.current.remembered).toBe(false);
  expect(rememberAppOutput).toHaveBeenCalledWith("example.browser", { key: "cite" });
});

test("an old successful copy cannot consume a newer one-time output choice", async () => {
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => browser, rememberAppOutput: vi.fn() }, options()));
  act(() => result.current.read());
  await waitFor(() => expect(result.current.ready).toBe(true));
  act(() => result.current.toggleOnce());
  act(() => result.current.choose("typst"));
  const old = result.current.copyToken();
  act(() => result.current.choose("cite"));
  act(() => result.current.consumeOnce(old));
  expect(result.current.format).toBe("cite");
  expect(result.current.onceOnly).toBe(true);
});

test("remembering, Automatic and Forget control the question independently of the global fallback", async () => {
  const settings = options();
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => browser,
    rememberAppOutput: vi.fn(async () => null), forgetAppOutputs: vi.fn(async () => null) }, settings));
  act(() => result.current.read());
  await waitFor(() => expect(result.current.ready).toBe(true));
  expect(result.current.shouldAsk).toBe(true);
  await act(async () => result.current.choose("pandoc"));
  expect(result.current.shouldAsk).toBe(false);
  expect(result.current.scope).toBe("Pandoc remembered for this browser");
  await act(async () => result.current.restoreAutomatic());
  expect(result.current.shouldAsk).toBe(true);
  expect(result.current.scope).toBeNull();
  await act(async () => result.current.choose("cite"));
  await act(async () => result.current.forgetAll?.());
  expect(result.current.shouldAsk).toBe(true);
  expect(settings.chooseDefault).not.toHaveBeenCalled();
});

test("a once-only pick announces its scope and consuming it brings the question back", async () => {
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => browser, rememberAppOutput: vi.fn() }, options()));
  act(() => result.current.read());
  await waitFor(() => expect(result.current.ready).toBe(true));
  act(() => result.current.toggleOnce());
  act(() => result.current.choose("style:apa"));
  expect(result.current.scope).toBe("APA for this citation");
  expect(result.current.shouldAsk).toBe(false);
  act(() => result.current.consumeOnce(result.current.copyToken()));
  expect(result.current.shouldAsk).toBe(true);
  expect(result.current.scope).toBeNull();
});

test("returning from permission keeps the prepared output once while reading the changed destination", async () => {
  let destination = browser;
  const rememberAppOutput = vi.fn();
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => destination, rememberAppOutput }, options()));
  await act(async () => result.current.read());
  act(() => result.current.toggleOnce());
  act(() => result.current.choose("typst"));
  const oldCopy = result.current.copyToken();
  destination = { ...browser, app: "Example Writer", app_id: "example.writer", remembered: { key: "cite" } };
  await act(async () => result.current.read(true, true));
  expect(result.current.app).toBe("Example Writer");
  expect(result.current.format).toBe("typst");
  expect(result.current.onceOnly).toBe(true);
  expect(result.current.isCurrentCopy(oldCopy)).toBe(false);
  expect(rememberAppOutput).not.toHaveBeenCalled();
  act(() => result.current.consumeOnce(result.current.copyToken()));
  expect(result.current.format).toBe("cite");
});

test("superseding destination reads preserve the permission return's output and copy token until consumed", async () => {
  const returning = pending<PickerDestination>();
  let destination = browser;
  let reads = 0;
  const { result } = renderHook(() => useDestination({
    pickerDestination: () => ++reads === 2 ? returning.promise : Promise.resolve(destination),
    rememberAppOutput: vi.fn(),
  }, options()));
  await act(async () => result.current.read());
  act(() => result.current.toggleOnce());
  act(() => result.current.choose("typst"));
  act(() => result.current.read(true, true));
  const token = result.current.copyToken();
  destination = { ...browser, app: "Example Writer", app_id: "example.writer", remembered: { key: "cite" } };
  await act(async () => result.current.read(false));
  await act(async () => returning.resolve(browser));
  expect(result.current.app).toBe("Example Writer");
  expect(result.current.format).toBe("typst");
  expect(result.current.onceOnly).toBe(true);
  expect(result.current.isCurrentCopy(token)).toBe(true);
  act(() => result.current.consumeOnce(token));
  expect(result.current.format).toBe("cite");
  expect(result.current.onceOnly).toBe(false);
  // Consuming the preserved output restores ordinary destination changes.
  destination = browser;
  await act(async () => result.current.read(false));
  expect(result.current.format).toBe("pandoc");
  expect(result.current.isCurrentCopy(token)).toBe(false);
});

test("a preserved browser output answers the question through refreshes until that citation is consumed", async () => {
  const returning = pending<PickerDestination>();
  let destination = browser;
  let reads = 0;
  const rememberAppOutput = vi.fn();
  const settings = options();
  const { result } = renderHook(() => useDestination({
    pickerDestination: () => ++reads === 2 ? returning.promise : Promise.resolve(destination),
    rememberAppOutput,
  }, settings));
  await act(async () => result.current.read());
  expect(result.current.shouldAsk).toBe(true);
  act(() => result.current.toggleOnce());
  act(() => result.current.choose("typst"));
  const oldCopy = result.current.copyToken();
  act(() => result.current.read(true, true));
  const token = result.current.copyToken();
  destination = { ...browser, app: "Another Browser", app_id: "example.another-browser" };
  await act(async () => result.current.read(false));
  await act(async () => returning.resolve(browser));
  expect(result.current.app).toBe("Another Browser");
  expect(result.current.format).toBe("typst");
  expect(result.current.onceOnly).toBe(true);
  expect(result.current.shouldAsk).toBe(false);
  expect(result.current.isCurrentCopy(token)).toBe(true);
  act(() => result.current.consumeOnce(oldCopy));
  expect(result.current.shouldAsk).toBe(false);
  act(() => result.current.consumeOnce(token));
  expect(result.current.format).toBe("pandoc");
  expect(result.current.onceOnly).toBe(false);
  expect(result.current.shouldAsk).toBe(true);
  expect(result.current.scope).toBeNull();
  expect(rememberAppOutput).not.toHaveBeenCalled();
  expect(settings.chooseDefault).not.toHaveBeenCalled();
});
