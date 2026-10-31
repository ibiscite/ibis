import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { SettingsApp } from "./settings/SettingsApp";
import { settingsApi } from "./settings/api";
import { backend, chooseFromMore, resetApp, type Handler } from "./testing/app";

afterEach(resetApp);

function settings(page = "general", overrides: Record<string, Handler> = {}) {
  const calls = backend({ update_checks: () => true, ...overrides });
  render(<SettingsApp initialPage={page} api={{ ...settingsApi,
    version: async () => "0.2.2", pastePermission: async () => ({ pastes: false, needed: false, granted: false, list: "" }),
    onCloseRequested: async () => () => undefined,
  }} />);
  return calls;
}

test("Settings shows login state from the system and follows changes made elsewhere", async () => {
  let system = true;
  const calls = settings("general", {
    login_item: () => ({ name: "Ibis", enabled: system, problem: null }),
    set_login_item: ({ enabled }) => { system = Boolean(enabled); return { name: "Ibis", enabled: system, problem: null }; },
  });
  const item = await screen.findByRole<HTMLInputElement>("switch", { name: "Start on Login" });
  expect(item.checked).toBe(true);
  fireEvent.click(item);
  await waitFor(() => expect(calls).toContain("set_login_item"));
  await waitFor(() => expect(item.checked).toBe(false));
  system = true;
  fireEvent.focus(window);
  await waitFor(() => expect(item.checked).toBe(true));
  expect(screen.queryByRole("alert")).toBeNull();
});

test("a refused login change is explained in Settings and never shown as on", async () => {
  settings("general", { set_login_item: () => ({ name: "Ibis", enabled: false, problem: "refused" }) });
  const item = await screen.findByRole<HTMLInputElement>("switch", { name: "Start on Login" });
  fireEvent.click(item);
  await screen.findByText(/The system did not let .*Ibis.* change this/);
  expect(item.checked).toBe(false);
  expect(document.getElementById(item.getAttribute("aria-describedby")!)?.textContent).toContain("startup applications settings");
});

test("a failed login command keeps the saved state and explains failure in Settings", async () => {
  settings("general", { set_login_item: () => { throw new Error("SYNTHETIC_PRIVATE_ERROR"); } });
  const item = await screen.findByRole<HTMLInputElement>("switch", { name: "Start on Login" });
  fireEvent.click(item);
  expect((await screen.findByRole("alert")).textContent).toBe("Ibis could not save this choice; the saved value is shown.");
  expect(item.checked).toBe(false);
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_ERROR");
});

test("Settings explains the unavailable login choice on unsupported systems", async () => {
  const calls = settings("general", { login_item: () => ({ name: "Ibis", enabled: false, problem: "unsupported" }) });
  const item = await screen.findByRole<HTMLInputElement>("switch", { name: "Start on Login" });
  await waitFor(() => expect(document.getElementById(item.getAttribute("aria-describedby")!)?.textContent).toContain("cannot start"));
  expect(item.disabled).toBe(true);
  expect(item.checked).toBe(false);
  expect(calls).not.toContain("set_login_item");
  expect(screen.queryByRole("alert")).toBeNull();
});

test("Settings owns automatic checks, follows saved events, and Check now explicitly checks once", async () => {
  const asked: unknown[] = [];
  const calls = settings("about", { update_checks: () => false,
    set_update_checks: ({ enabled }) => { asked.push(enabled); return null; }, check_for_updates: () => null });
  const item = await screen.findByRole<HTMLInputElement>("switch", { name: "Check for updates automatically" });
  expect(item.checked).toBe(false);
  fireEvent.click(item);
  await waitFor(() => expect(asked).toEqual([true]));
  expect(item.checked).toBe(false);
  await act(async () => { await emit("update:automatic", true); });
  expect(item.checked).toBe(true);
  await act(async () => { await emit("update:automatic", false); });
  expect(item.checked).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Check now" }));
  await waitFor(() => expect(calls.filter((c) => c === "check_for_updates")).toHaveLength(1));
});

test("a hidden picker ignores update availability without showing, checking or surfacing Settings preferences", async () => {
  const calls = backend({ "plugin:window|is_visible": () => false, update_available: () => true });
  await act(async () => { render(<App />); });
  await act(async () => { await emit("update:available", true); await emit("update:automatic", true); });
  expect(document.documentElement.dataset.pickerHidden).toBe("");
  await chooseFromMore("Rows: Comfortable");
  expect(screen.queryByRole("menuitem", { name: /[Uu]pdates/ })).toBeNull();
  expect(calls).not.toContain("check_for_updates");
  expect(calls).not.toContain("update_checks");
  expect(calls).not.toContain("login_item");
  expect(calls).not.toContain("plugin:window|show");
  expect(calls).not.toContain("plugin:window|set_focus");
});
