import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { useDestination } from "./useDestination";
import type { PickerDestination } from "../../api";
import type { OutputId } from "../../format/outputs";

afterEach(cleanup);
const browser: PickerDestination = { app: "Example Browser", app_id: "example.browser", app_identity: null, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: "browser", paste: null, paste_question: false, can_choose_paste: false };
const pending = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
};
function options() {
  return { fallback: "pandoc" as const, chooseDefault: vi.fn(), report: vi.fn() };
}

const refused = () => vi.fn(async (): Promise<null> => { throw new Error("synthetic save failure"); });
const noApp: PickerDestination = { ...browser, app: null, app_id: null, output_question: null };

test("a pick is remembered for the app, says so, and never changes the default", async () => {
  const settings = options();
  const rememberAppOutput = vi.fn(async () => null);
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => browser, rememberAppOutput }, settings));
  await act(async () => result.current.read());
  expect(result.current).toMatchObject({ format: "pandoc", source: "default", pick: "app" });
  await act(async () => result.current.choose("cite"));
  expect(rememberAppOutput).toHaveBeenCalledExactlyOnceWith("example.browser", "cite", null, null);
  expect(result.current).toMatchObject({ format: "cite", source: "remembered", pick: "app" });
  expect(settings.chooseDefault).not.toHaveBeenCalled();
});

test("a format comes from the app's memory, then its suggestion, then the default", async () => {
  // Rust works out the suggestion, a formatted app's style from the default included.
  let destination: PickerDestination = { ...browser, output_question: null, automatic: "ieee" };
  const { result, rerender } = renderHook(({ fallback }) => useDestination({ pickerDestination: async () => destination,
    rememberAppOutput: vi.fn(async () => null) }, { ...options(), fallback }), { initialProps: { fallback: "ieee" as OutputId } });
  await act(async () => result.current.read());
  expect(result.current).toMatchObject({ format: "ieee", source: "suggested" });
  rerender({ fallback: "pandoc" });
  expect(result.current).toMatchObject({ format: "ieee", source: "suggested" });
  destination = { ...destination, remembered: "typst" };
  await act(async () => result.current.read());
  expect(result.current).toMatchObject({ format: "typst", source: "remembered" });
  // An id this Ibis does not list is as good as none.
  destination = { ...browser, output_question: null, remembered: "a-later-output" };
  await act(async () => result.current.read());
  expect(result.current).toMatchObject({ format: "pandoc", source: "default" });
});

test("with no app a pick becomes the default, so it is not lost and nothing is remembered", async () => {
  const settings = options();
  const rememberAppOutput = vi.fn();
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => noApp, rememberAppOutput }, settings));
  await act(async () => result.current.read());
  expect(result.current).toMatchObject({ app: null, source: "default", pick: "default" });
  act(() => result.current.choose("typst"));
  expect(settings.chooseDefault).toHaveBeenCalledExactlyOnceWith("typst");
  expect(rememberAppOutput).not.toHaveBeenCalled();
  expect(result.current.source).toBe("default");
});

test("an unread destination may be a real app, so a pick holds for this paste and leaves the default alone", async () => {
  const settings = options();
  const rememberAppOutput = vi.fn();
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => { throw new Error("synthetic read failure"); }, rememberAppOutput }, settings));
  await act(async () => result.current.read());
  expect(settings.report).toHaveBeenLastCalledWith("appRead");
  expect(result.current).toMatchObject({ source: "default", pick: "paste" });
  act(() => result.current.choose("typst"));
  expect(result.current).toMatchObject({ format: "typst", source: "held" });
  expect(settings.chooseDefault).not.toHaveBeenCalled();
  expect(rememberAppOutput).not.toHaveBeenCalled();
  act(() => result.current.consumeHeld(result.current.copyToken()));
  expect(result.current).toMatchObject({ format: "pandoc", source: "default" });
});

test("the format a changed citation was written in applies once and is never remembered", async () => {
  const remember = vi.fn(async () => null);
  const writer = { ...browser, output_question: null, can_choose_paste: true };
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => writer, rememberAppOutput: remember }, options()));
  act(() => result.current.read());
  await waitFor(() => expect(result.current.ready).toBe(true));
  act(() => result.current.chooseOnce("cite"));
  expect(result.current.format).toBe("cite");
  act(() => result.current.choosePaste("copy-only"));
  await waitFor(() => expect(remember).toHaveBeenCalledOnce());
  expect(remember).toHaveBeenCalledWith(writer.app_id, null, "copy-only", null);
});

test("a choice made during a slow destination read wins over that read", async () => {
  const read = pending<PickerDestination>();
  const settings = options();
  const { result } = renderHook(() => useDestination({ pickerDestination: () => read.promise, rememberAppOutput: vi.fn() }, settings));
  act(() => result.current.read());
  act(() => result.current.choose("typst"));
  await act(async () => read.resolve({ ...browser, remembered: "cite" }));
  expect(result.current.format).toBe("typst");
  expect(settings.chooseDefault).not.toHaveBeenCalled();
});

test("a pick being saved already reads as remembered", async () => {
  const settings = options();
  const save = pending<null>();
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => browser, rememberAppOutput: () => save.promise }, settings));
  await act(async () => result.current.read());
  act(() => result.current.choose("cite"));
  expect(result.current).toMatchObject({ format: "cite", source: "remembered", saving: true });
  await act(async () => { save.resolve(null); });
  expect(result.current).toMatchObject({ format: "cite", source: "remembered", saving: false });
});

test.each(["failed I/O", "refused by the native side"])("a pick whose save is %s stays for this paste and does not claim it was remembered", async () => {
  const settings = options();
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => browser, rememberAppOutput: refused() }, settings));
  await act(async () => result.current.read());
  await act(async () => result.current.choose("cite"));
  expect(result.current).toMatchObject({ format: "cite", source: "held", saving: false });
  expect(settings.report).toHaveBeenCalledWith("appChoice");
  act(() => result.current.consumeHeld(result.current.copyToken()));
  expect(result.current).toMatchObject({ format: "pandoc", source: "default" });
});

test("a held pick and its save failure are dropped on reopening, which shows what is durably saved", async () => {
  let destination = browser;
  const settings = options();
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => destination, rememberAppOutput: refused() }, settings));
  await act(async () => result.current.read());
  await act(async () => result.current.choose("cite"));
  expect(result.current.source).toBe("held");
  destination = { ...browser, remembered: "typst" };
  await act(async () => result.current.read());
  expect(result.current).toMatchObject({ format: "typst", source: "remembered" });
  expect(settings.report).toHaveBeenCalledWith("appChoice", false);
});

test("a delivery answer to an open first-paste question accepts its format, so it is not asked again", async () => {
  let destination: PickerDestination = { ...browser, can_choose_paste: true };
  const rememberAppOutput = vi.fn(async (_id: string, output: PickerDestination["remembered"]) => {
    destination = { ...destination, remembered: output, paste: "copy-only" };
    return null;
  });
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => destination, rememberAppOutput }, options()));
  await act(async () => result.current.read());
  expect(result.current.shouldAsk).toBe(true);
  await act(async () => result.current.choosePaste("copy-only", true));
  await waitFor(() => expect(result.current.ready && !result.current.saving).toBe(true));
  expect(rememberAppOutput).toHaveBeenCalledExactlyOnceWith("example.browser", "pandoc", "copy-only", null);
  expect(result.current).toMatchObject({ shouldAsk: false, source: "remembered" });
});

test("a delivery answer with no open question leaves the format to follow the default", async () => {
  const rememberAppOutput = vi.fn(async () => null);
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => browser, rememberAppOutput }, options()));
  await act(async () => result.current.read());
  await act(async () => result.current.choosePaste("copy-only"));
  await waitFor(() => expect(result.current.ready && !result.current.saving).toBe(true));
  expect(rememberAppOutput).toHaveBeenCalledExactlyOnceWith("example.browser", null, "copy-only", null);
  expect(result.current.shouldAsk).toBe(true);
});

test("a held format is never promoted by a delivery answer, even with a question open", async () => {
  const rememberAppOutput = vi.fn(async (_id: string, output: unknown) => {
    if (output) throw new Error("synthetic save failure");
    return null;
  });
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => browser, rememberAppOutput }, options()));
  await act(async () => result.current.read());
  await act(async () => result.current.choose("typst"));
  await act(async () => result.current.choosePaste("copy-only", true));
  await waitFor(() => expect(result.current.ready && !result.current.saving).toBe(true));
  expect(rememberAppOutput).toHaveBeenLastCalledWith("example.browser", null, "copy-only", null);
  expect(result.current).toMatchObject({ format: "typst", source: "held" });
});

test("a delivery answer keeps a held format and sends no format of its own", async () => {
  let destination: PickerDestination = { ...browser, output_question: null, paste_question: true, can_choose_paste: false };
  const rememberAppOutput = vi.fn(async (_id: string, output: unknown) => {
    if (output) throw new Error("synthetic save failure");
    destination = { ...destination, paste: "paste", paste_question: false, can_choose_paste: false, can_paste: true };
    return null;
  });
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => destination, rememberAppOutput }, options()));
  await act(async () => result.current.read());
  await act(async () => result.current.choose("typst"));
  await act(async () => result.current.choosePaste("paste"));
  await waitFor(() => expect(result.current.ready && !result.current.saving).toBe(true));
  expect(rememberAppOutput).toHaveBeenLastCalledWith("example.browser", null, "paste", null);
  expect(result.current).toMatchObject({ format: "typst", source: "held" });
  act(() => result.current.consumeHeld(result.current.copyToken()));
  expect(result.current.format).toBe("pandoc");
});

test("recovery keeps a proven app's delivery choices available and a format save cannot clear its warning", async () => {
  const destination: PickerDestination = { ...browser, output_question: null, unreadable: true, can_choose_paste: true };
  const settings = options();
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => destination,
    rememberAppOutput: vi.fn(async () => null) }, settings));
  await act(async () => result.current.read());
  expect(result.current.pasteChoices).toBe(true);
  await act(async () => result.current.choose("cite"));
  await waitFor(() => expect(result.current.ready && !result.current.saving).toBe(true));
  expect(settings.report).toHaveBeenLastCalledWith("appChoices", true);
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
  expect(result.current).toMatchObject({ format: "pandoc", source: "default" });
  expect(rememberAppOutput).toHaveBeenCalledWith("example.browser", "cite", null, null);
});

test("a save that finishes after a newer pick does not clear the newer pick", async () => {
  const first = pending<null>();
  const rememberAppOutput = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValue(null);
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => browser, rememberAppOutput }, options()));
  await act(async () => result.current.read());
  act(() => result.current.choose("cite"));
  act(() => result.current.choose("typst"));
  await act(async () => { first.resolve(null); });
  expect(result.current.format).toBe("typst");
  await waitFor(() => expect(result.current).toMatchObject({ format: "typst", source: "remembered", saving: false }));
  expect(rememberAppOutput.mock.calls.map((call) => call[1])).toEqual(["cite", "typst"]);
});

test("an old successful copy cannot consume a newer held pick", async () => {
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => browser, rememberAppOutput: refused() }, options()));
  await act(async () => result.current.read());
  await act(async () => result.current.choose("typst"));
  const old = result.current.copyToken();
  await act(async () => result.current.choose("cite"));
  act(() => result.current.consumeHeld(old));
  expect(result.current).toMatchObject({ format: "cite", source: "held" });
});

test("a remembered format answers the question through a Settings refresh", async () => {
  let destination = browser;
  const settings = options();
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => destination,
    rememberAppOutput: vi.fn(async () => { destination = { ...browser, remembered: "pandoc" }; return null; }) }, settings));
  await act(async () => result.current.read());
  expect(result.current.shouldAsk).toBe(true);
  await act(async () => result.current.choose("pandoc"));
  expect(result.current.shouldAsk).toBe(false);
  await act(async () => result.current.read(false));
  expect(result.current.shouldAsk).toBe(false);
  expect(settings.chooseDefault).not.toHaveBeenCalled();
});

test("a held pick answers the question for one citation, then it comes back", async () => {
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => browser, rememberAppOutput: refused() }, options()));
  await act(async () => result.current.read());
  await act(async () => result.current.choose("apa"));
  expect(result.current.shouldAsk).toBe(false);
  act(() => result.current.consumeHeld(result.current.copyToken()));
  expect(result.current.shouldAsk).toBe(true);
});

test("returning from permission keeps the prepared output once while reading the changed destination", async () => {
  let destination = browser;
  const rememberAppOutput = vi.fn(async () => null);
  const { result } = renderHook(() => useDestination({ pickerDestination: async () => destination, rememberAppOutput }, options()));
  await act(async () => result.current.read());
  await act(async () => result.current.choose("typst"));
  const oldCopy = result.current.copyToken();
  destination = { ...browser, app: "Example Writer", app_id: "example.writer", app_identity: null, remembered: "cite" };
  await act(async () => result.current.read(true, true));
  expect(result.current.app).toBe("Example Writer");
  expect(result.current).toMatchObject({ format: "typst", source: "held" });
  expect(result.current.isCurrentCopy(oldCopy)).toBe(false);
  expect(rememberAppOutput).toHaveBeenCalledTimes(1);
  act(() => result.current.consumeHeld(result.current.copyToken()));
  expect(result.current).toMatchObject({ format: "cite", source: "remembered" });
});

test("superseding destination reads preserve the permission return's output and copy token until consumed", async () => {
  const returning = pending<PickerDestination>();
  let destination = browser;
  let reads = 0;
  const { result } = renderHook(() => useDestination({
    pickerDestination: () => ++reads === 2 ? returning.promise : Promise.resolve(destination),
    rememberAppOutput: refused(),
  }, options()));
  await act(async () => result.current.read());
  await act(async () => result.current.choose("typst"));
  act(() => result.current.read(true, true));
  const token = result.current.copyToken();
  destination = { ...browser, app: "Example Writer", app_id: "example.writer", app_identity: null, remembered: "cite" };
  await act(async () => result.current.read(false));
  await act(async () => returning.resolve(browser));
  expect(result.current.app).toBe("Example Writer");
  expect(result.current).toMatchObject({ format: "typst", source: "held" });
  expect(result.current.isCurrentCopy(token)).toBe(true);
  act(() => result.current.consumeHeld(token));
  expect(result.current).toMatchObject({ format: "cite", source: "remembered" });
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
  const rememberAppOutput = refused();
  const settings = options();
  const { result } = renderHook(() => useDestination({
    pickerDestination: () => ++reads === 2 ? returning.promise : Promise.resolve(destination),
    rememberAppOutput,
  }, settings));
  await act(async () => result.current.read());
  expect(result.current.shouldAsk).toBe(true);
  await act(async () => result.current.choose("typst"));
  const oldCopy = result.current.copyToken();
  act(() => result.current.read(true, true));
  const token = result.current.copyToken();
  destination = { ...browser, app: "Another Browser", app_id: "example.another-browser" };
  await act(async () => result.current.read(false));
  await act(async () => returning.resolve(browser));
  expect(result.current.app).toBe("Another Browser");
  expect(result.current).toMatchObject({ format: "typst", source: "held" });
  expect(result.current.shouldAsk).toBe(false);
  expect(result.current.isCurrentCopy(token)).toBe(true);
  act(() => result.current.consumeHeld(oldCopy));
  expect(result.current.shouldAsk).toBe(false);
  act(() => result.current.consumeHeld(token));
  expect(result.current).toMatchObject({ format: "pandoc", source: "default" });
  expect(result.current.shouldAsk).toBe(true);
  expect(rememberAppOutput).toHaveBeenCalledTimes(1);
  expect(settings.chooseDefault).not.toHaveBeenCalled();
});
