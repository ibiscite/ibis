import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { FormatSettingsView } from "../bindings";
import { FormatSettings as FormatRows, useFormatSettings } from "./FormatSettings";
import type { SettingsApi } from "./api";

afterEach(cleanup);
const empty: FormatSettingsView = { apps: [], writings: [], key_output_visible: false };
const remembered: FormatSettingsView = { ...empty, apps: [{ id: "macos:example.word", app: "Word", format: "apa" }] };
const pending = <T,>() => {
  let resolve!: (next: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
};
function api() {
  return { formatSettings: vi.fn(async () => remembered), forgetAppFormat: vi.fn(async () => null),
    openSettingsFile: vi.fn(async () => null), onPreferencesChanged: async () => () => {} } as unknown as SettingsApi;
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
