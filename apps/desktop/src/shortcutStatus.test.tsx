import { expect, test, vi, afterEach } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import * as api from "./api";
import { backend, resetApp } from "./testing/app";

afterEach(resetApp);

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
