import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { afterEach, expect, test } from "vitest";
import { App } from "../App";
import { commands } from "../bindings";
import { backend, chooseRows, resetApp } from "../testing/app";
import { SettingsApp } from "./SettingsApp";
import { settingsApi } from "./api";
import { demoApi } from "./demo";

afterEach(resetApp);

test("late Settings A and newer picker B both reach both real interfaces and saved state", async () => {
  let release!: () => void;
  const slow = new Promise<void>((resolve) => { release = resolve; });
  const writes: number[] = [];
  backend({ picker_preferences_saved: async ({ preferences, receipt }) => {    writes.push(receipt as number);
    if ((preferences as { larger_text: boolean }).larger_text) await slow;
  } });
  // Keep platform-only panels fake; exercise the actual command wrappers and
  // event subscription for the shared preference state in both interfaces.
  const api = { ...demoApi(), pickerPreferences: settingsApi.pickerPreferences,
    savePickerPreferences: settingsApi.savePickerPreferences, pickerPreferencesSaved: settingsApi.pickerPreferencesSaved,
    onPreferencesChanged: settingsApi.onPreferencesChanged };
  const view = render(<><App /><SettingsApp api={api} initialPage="look" /></>);
  const rows = await screen.findByRole("radiogroup", { name: "Rows" });
  fireEvent.click(screen.getByRole("radio", { name: "Larger" }));
  await waitFor(() => expect(writes).toHaveLength(1));
  await chooseRows("compact");
  await waitFor(() => expect(writes).toHaveLength(2));
  expect(writes[0]!).toBeLessThan(writes[1]!);
  await waitFor(async () => expect((await commands.pickerPreferences()).density).toBe("compact"));
  await act(async () => { await emit("preferences:changed", { ...await commands.pickerPreferences(), revision: writes[1]! }); });
  await act(async () => { release(); });
  await waitFor(() => expect(within(rows).getByRole("radio", { name: "Compact" })).toHaveProperty("checked", true));
  expect(await commands.pickerPreferences()).toMatchObject({ density: "compact", larger_text: true });
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  expect(await screen.findByRole("menuitem", { name: "Rows: Compact" })).toBeTruthy();
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  view.unmount();
  render(<SettingsApp api={api} initialPage="look" />);
  expect(await screen.findByRole("radio", { name: "Compact" })).toHaveProperty("checked", true);
});
