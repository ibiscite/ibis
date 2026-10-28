import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { useDestination } from "./useDestination";
import type { PickerDestination } from "../../api";

afterEach(cleanup);
const browser: PickerDestination = { app: "Example Browser", app_id: "example.browser", automatic: null, remembered: null, unreadable: false };
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
