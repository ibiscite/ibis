import { afterEach, expect, test } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { SettingsApp } from "./settings/SettingsApp";
import { settingsApi } from "./settings/api";
import { backend, resetApp, type Handler } from "./testing/app";

afterEach(resetApp);

function settings(page = "general", overrides: Record<string, Handler> = {}) {
  const calls = backend({ update_checks: () => true, ...overrides });
  render(<SettingsApp initialPage={page} api={{ ...settingsApi,
    version: async () => "0.2.2", pastePermission: async () => ({ pastes: false, needed: false, granted: false, list: "" }),
    onCloseRequested: async () => () => undefined,
    settingsShown: undefined,
  }} />);
  return calls;
}

test("a refused login change is explained in Settings and never shown as on", async () => {
  settings("general", { set_login_item: () => ({ name: "Ibis", enabled: false, problem: "refused" }) });
  const item = await screen.findByRole<HTMLInputElement>("switch", { name: "Start on login" });
  fireEvent.click(item);
  await screen.findByText(/The system did not let .*Ibis.* change this/);
  expect(item.checked).toBe(false);
  expect(document.getElementById(item.getAttribute("aria-describedby")!)?.textContent).toContain("startup applications settings");
});
