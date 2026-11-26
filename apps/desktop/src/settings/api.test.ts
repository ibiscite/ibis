import { afterEach, expect, test, vi } from "vitest";
import { settingsApi } from "./api";

const native = vi.hoisted(() => ({
  listeners: new Map<string, Set<(event: { payload: unknown }) => void>>(),
  closeRequested: [] as ((event: unknown) => Promise<void>)[],
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, handler: (event: { payload: unknown }) => void) => {
    if (!native.listeners.has(name)) native.listeners.set(name, new Set());
    native.listeners.get(name)!.add(handler);
    return () => { native.listeners.get(name)!.delete(handler); };
  },
}));
// Tauri's own wrapper destroys the window once the handler resolves.
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onCloseRequested: async (handler: (event: unknown) => Promise<void>) => { native.closeRequested.push(handler); return () => undefined; },
  }),
}));

afterEach(() => {
  delete document.documentElement.dataset.system;
  native.listeners.clear();
  native.closeRequested.length = 0;
});

test("macOS keeps Settings: no close listener exists, and the page hears that it was hidden", async () => {
  document.documentElement.dataset.system = "macos";
  const heard: boolean[] = [];
  const stop = await settingsApi.onCloseRequested(async (kept) => { heard.push(kept); });
  expect(native.closeRequested).toHaveLength(0);
  for (const handler of native.listeners.get("settings:hidden") ?? []) handler({ payload: null });
  expect(heard).toEqual([true]);
  stop();
  expect(native.listeners.get("settings:hidden")?.size).toBe(0);
});

test.each(["windows", "linux"])("%s closes Settings once the page has saved", async (system) => {
  document.documentElement.dataset.system = system;
  const heard: boolean[] = [];
  await settingsApi.onCloseRequested(async (kept) => { heard.push(kept); });
  expect(native.listeners.has("settings:hidden")).toBe(false);
  expect(native.closeRequested).toHaveLength(1);
  await native.closeRequested[0]!({});
  expect(heard).toEqual([false]);
});
