import { expect, test, vi, afterEach } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { StrictMode } from "react";
import { App } from "./App";
import * as api from "./api";
import { backend, resetApp } from "./testing/app";

afterEach(resetApp);

test("the keys list preserves the desktop's trigger text without inventing an accelerator", async () => {
  const label = "Strg + Umschalt + Y";
  backend({ shortcut_status: () => ({ shortcut: label, registered: true, error: null, desktop: { configurable: true, guidance: "Desktop settings" } }) });
  render(<App />);
  const input = await screen.findByLabelText("Search references");
  await act(async () => { await emit("shortcut:status", { shortcut: label, registered: true, error: null, desktop: { configurable: true, guidance: "Desktop settings" } }); });
  fireEvent.keyDown(input, { key: "/", code: "Slash", ctrlKey: true });
  const trigger = await screen.findByText(label);
  expect(trigger.tagName).toBe("KBD");
  expect(trigger.hasAttribute("aria-keyshortcuts")).toBe(false);
});

test("the picker shows the exact compositor binding when the portal is unavailable", async () => {
  const guidance = "Sway: add bindsym --no-repeat Ctrl+Shift+y exec ibis toggle to ~/.config/sway/config.";
  backend({ shortcut_status: () => ({ shortcut: "", registered: false, error: "The desktop shortcut is unavailable.", desktop: { configurable: false, guidance } }) });
  render(<App />);
  expect((await screen.findByText(/Sway: add bindsym/)).textContent).toContain(guidance);
});

test("a shortcut that could not be registered is explained", async () => {
  backend({
    shortcut_status: () => ({ shortcut: "Control+Command+R", registered: false, error: "another app may be using it" }),
  });
  render(<App />);
  expect((await screen.findByText(/another app may be using it/)).getAttribute("role")).toBe("status");
});

test("a shortcut note that appears while the picker is hidden shows when it opens again", async () => {
  let error: string | null = null;
  backend({ shortcut_status: () => ({ shortcut: "Command+Shift+Y", registered: true, error }) });
  render(<App />);
  await act(async () => { await emit("picker:hidden"); });
  error = "No key on this keyboard layout types the shortcut's letter";
  await act(async () => { await emit("picker:shown"); });
  expect(await screen.findByText(/No key on this keyboard layout/)).toBeTruthy();
  error = null;
  await act(async () => { await emit("picker:hidden"); await emit("picker:shown"); });
  await waitFor(() => expect(screen.queryByText(/No key on this keyboard layout/)).toBeNull());
});

test("a shortcut note that changes while the picker is open shows at once", async () => {
  backend({ shortcut_status: () => ({ shortcut: "Command+Shift+Y", registered: true, error: null }) });
  render(<App />);
  await screen.findByLabelText("Search references");
  const note = "No key on this keyboard layout types the shortcut's letter";
  await act(async () => { await emit("shortcut:status", { shortcut: "Command+Shift+Y", registered: true, error: note }); });
  expect(await screen.findByText(/No key on this keyboard layout/)).toBeTruthy();
  await act(async () => { await emit("shortcut:status", { shortcut: "Command+Shift+Y", registered: true, error: null }); });
  await waitFor(() => expect(screen.queryByText(/No key on this keyboard layout/)).toBeNull());
});

test("the shortcut status is read only once its change listener is in place", async () => {
  backend();
  let ready: () => void = () => undefined;
  const reads = vi.fn(async () => ({ shortcut: "Command+Shift+Y", registered: true, error: null }));
  const onShortcutStatus = () => new Promise<() => void>((resolve) => { ready = () => resolve(() => undefined); });
  render(<App api={{ ...api, shortcutStatus: reads, onShortcutStatus }} />);
  await screen.findByLabelText("Search references");
  expect(reads).not.toHaveBeenCalled();
  await act(async () => { ready(); });
  await waitFor(() => expect(reads).toHaveBeenCalledTimes(1));
});

test("a remount's abandoned shortcut subscription failing later reports nothing", async () => {
  backend();
  let fail: () => void = () => undefined;
  let subscriptions = 0;
  const onShortcutStatus = () => {
    subscriptions += 1;
    // The first, abandoned subscription fails only after the live one is in place.
    return subscriptions === 1
      ? new Promise<() => void>((_, reject) => { fail = () => reject(new Error("gone")); })
      : Promise.resolve(() => undefined);
  };
  render(<StrictMode><App api={{ ...api, onShortcutStatus }} /></StrictMode>);
  await screen.findByLabelText("Search references");
  await waitFor(() => expect(subscriptions).toBe(2));
  await act(async () => { fail(); await new Promise((resolve) => setTimeout(resolve, 0)); });
  expect(screen.queryByText(/Restart Ibis/)).toBeNull();
});
