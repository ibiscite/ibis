import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { FormatSettingsView } from "../bindings";
import { FormatSettings as FormatRows, SettingsFileRow, useFormatSettings } from "./FormatSettings";
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
const empty: FormatSettingsView = { apps: [], writings: [] };
const remembered: FormatSettingsView = { ...empty, apps: [{ id: "macos:example.word", app: "Word", format: "apa" }] };
function api() {
  return { ...settingsApi, formatSettings: vi.fn(async () => remembered), forgetAppFormat: vi.fn(async () => null),
    openSettingsFile: vi.fn(async () => null) } as SettingsApi;
}

function FormatSettings({ api }: { api: SettingsApi }) {
  const settings = useFormatSettings(api);
  return <><FormatRows settings={settings} /><SettingsFileRow settings={settings} highlighted={null} /></>;
}

test("forgetting an app restores its usual format and hides the empty group", async () => {
  const native = api();
  const read = vi.mocked(native.formatSettings!);
  read.mockResolvedValueOnce(remembered).mockResolvedValueOnce(empty);
  render(<FormatSettings api={native} />);
  fireEvent.click(await screen.findByRole("button", { name: "Forget" }));
  await waitFor(() => expect(screen.queryByText("Word")).toBeNull());
  expect(native.forgetAppFormat).toHaveBeenCalledExactlyOnceWith("macos:example.word");
  expect(screen.queryByText("Projects")).toBeNull();
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
