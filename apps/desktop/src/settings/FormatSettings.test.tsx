import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { emit } from "@tauri-apps/api/event";
import type { FormatSettingsView } from "../bindings";
import { FormatSettings as FormatRows, useFormatSettings } from "./FormatSettings";
import { settingsApi, type SettingsApi } from "./api";

vi.mock("@tauri-apps/api/event", () => {
  const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();
  return {
    listen: async (name: string, handler: (event: { payload: unknown }) => void) => {
      if (!listeners.has(name)) listeners.set(name, new Set());
      listeners.get(name)!.add(handler);
      return () => { listeners.get(name)!.delete(handler); };
    },
    emit: async (name: string, payload: unknown) => {
      for (const handler of listeners.get(name) ?? []) handler({ payload });
    },
  };
});

afterEach(cleanup);
const empty: FormatSettingsView = { apps: [], writings: [], key_output_visible: false };
const remembered: FormatSettingsView = { ...empty, apps: [{ id: "macos:example.word", app: "Word", format: "apa" }] };
const pending = <T,>() => {
  let resolve!: (next: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
};
function api() {
  return { ...settingsApi, formatSettings: vi.fn(async () => remembered), forgetAppFormat: vi.fn(async () => null),
    openSettingsFile: vi.fn(async () => null) } as SettingsApi;
}

function FormatSettings({ api }: { api: SettingsApi }) {
  return <FormatRows settings={useFormatSettings(api)} highlighted={null} />;
}

test("forgetting an app restores its usual format and hides the empty group", async () => {
  const native = api();
  const read = vi.mocked(native.formatSettings!);
  read.mockResolvedValueOnce(remembered).mockResolvedValueOnce(empty);
  render(<FormatSettings api={native} />);
  fireEvent.click(await screen.findByRole("button", { name: "Forget" }));
  await waitFor(() => expect(screen.queryByText("Word")).toBeNull());
  expect(native.forgetAppFormat).toHaveBeenCalledExactlyOnceWith("macos:example.word");
  expect(screen.queryByText("Writings")).toBeNull();
});

test.each(["before", "after"])("a focus read finishing %s Forget cannot leave a forgotten app shown", async (order) => {
  const native = api(), save = pending<void>(), focused = pending<FormatSettingsView>();
  let saved = remembered;
  vi.mocked(native.formatSettings!).mockResolvedValueOnce(remembered).mockReturnValueOnce(focused.promise).mockImplementation(async () => saved);
  vi.mocked(native.forgetAppFormat!).mockImplementation(async () => { await save.promise; saved = empty; return null; });
  render(<FormatSettings api={native} />);
  fireEvent.click(await screen.findByRole("button", { name: "Forget" }));
  await act(async () => { fireEvent.focus(window); });
  if (order === "before") await act(async () => focused.resolve(remembered));
  expect(screen.getByText("Word")).toBeTruthy();
  await act(async () => save.resolve());
  if (order === "after") await act(async () => focused.resolve(remembered));
  expect(saved.apps).toHaveLength(0);
  expect(screen.queryByText("Word")).toBeNull();
  expect(screen.queryAllByRole("button", { name: "Forget" })).toHaveLength(0);
});

test.each(["preferences:changed", "writings:changed"])("a live edit reported by %s refreshes Settings without refocusing", async (event) => {
  const native = api();
  let saved: FormatSettingsView = { ...empty, writings: [{ id: "draft", name: "Draft", output: "apa", bibliography: null }] };
  vi.mocked(native.formatSettings!).mockImplementation(async () => saved);
  render(<FormatSettings api={native} />);
  await screen.findByText("Draft");
  saved = { ...empty, writings: [{ id: "draft", name: "Revised draft", output: "apa", bibliography: null }] };
  await act(async () => { await emit(event, { revision: 1, fields: [] }); });
  expect(screen.queryByText("Draft")).toBeNull();
  expect(screen.getByText("Revised draft")).toBeTruthy();
});

test.each(["refused", "failed I/O"])("a forget that is %s keeps the saved app and reports the failure", async () => {
  const native = api();
  vi.mocked(native.forgetAppFormat!).mockRejectedValueOnce(new Error("synthetic refusal"));
  render(<FormatSettings api={native} />);
  fireEvent.click(await screen.findByRole("button", { name: "Forget" }));
  await screen.findByRole("alert");
  expect(screen.getByText("Word")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Forget" }).hasAttribute("disabled")).toBe(false);
});

test("an older file snapshot cannot restore an app after a newer read or close and reopen", async () => {
  const native = api(), older = pending<FormatSettingsView>(), newer = pending<FormatSettingsView>();
  vi.mocked(native.formatSettings!).mockReturnValueOnce(older.promise).mockReturnValueOnce(newer.promise).mockResolvedValue(empty);
  const mounted = render(<FormatSettings api={native} />);
  fireEvent.focus(window);
  await act(async () => newer.resolve(remembered));
  expect(screen.getByText("Word")).toBeTruthy();
  mounted.unmount();
  render(<FormatSettings api={native} />);
  await act(async () => {});
  expect(screen.queryByText("Word")).toBeNull();
  await act(async () => older.resolve(remembered));
  expect(screen.queryByText("Word")).toBeNull();
});
