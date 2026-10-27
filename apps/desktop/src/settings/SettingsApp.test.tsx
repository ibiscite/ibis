import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import axe from "axe-core";
import { afterEach, expect, test, vi } from "vitest";
import type { PickerPreferences } from "../bindings";
import { pageOf, SettingsApp } from "./SettingsApp";
import { RECHECK_MS, STALE_HINT_MS } from "./PasteGuide";
import type { PastePermission, SettingsApi } from "./api";

afterEach(() => vi.useRealTimers());

function fakeApi(permission: Partial<PastePermission> = {}, preferences: Partial<PickerPreferences> = {}) {
  const state = {
    permission: { needed: true, granted: false, list: "Device Control and Data Access", ...permission },
    preferences: { citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true, search_mark: false, paste: true, ...preferences } as PickerPreferences,
    saved: [] as PickerPreferences[],
    opened: 0,
    reads: 0,
    pages: [] as ((page: string) => void)[],
  };
  const api: SettingsApi = {
    pickerPreferences: async () => state.preferences,
    setPickerPreferences: async (next) => { state.saved.push(next); state.preferences = next; return null as never; },
    loginItem: async () => ({ name: "Ibis Nightly", enabled: false, problem: null }),
    setLoginItem: async (enabled) => ({ name: "Ibis Nightly", enabled, problem: null }),
    updateChecks: async () => true,
    setUpdateChecks: async () => undefined as never,
    checkForUpdates: async () => undefined as never,
    pastePermission: async () => { state.reads += 1; return state.permission; },
    openPastePermission: async () => { state.opened += 1; return undefined as never; },
    version: async () => "0.3.0",
    onPage: async (handler) => { state.pages.push(handler); return () => undefined; },
    onUpdateChecks: async () => () => undefined,
  };
  return { api, state };
}

test("pages are opened by name, and the paste guide is General's pasting group", () => {
  expect(pageOf("paste")).toEqual({ page: "general", focus: "paste" });
  expect(pageOf("about")).toEqual({ page: "about", focus: null });
  expect(pageOf("unknown")).toEqual({ page: "general", focus: null });
  expect(pageOf(null)).toEqual({ page: "general", focus: null });
});

test("the paste guide names the list, opens it, notices the grant on its own and says so", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const { api, state } = fakeApi();
  render(<SettingsApp api={api} initialPage="paste" />);
  const guide = await screen.findByRole("heading", { name: "Allow Ibis to paste for you" });
  const card = guide.closest(".paste-guide") as HTMLElement;
  expect(card.textContent).toContain("In Privacy & Security › Device Control and Data Access, turn on Ibis Nightly.");
  fireEvent.click(within(card).getByRole("button", { name: "Open System Settings" }));
  expect(state.opened).toBe(1);
  expect(within(card).getByRole("status").textContent).toBe("Waiting for your OK…");
  // Still waiting after a while: say how to clear a stale switch.
  await act(async () => { await vi.advanceTimersByTimeAsync(STALE_HINT_MS); });
  expect(card.textContent).toContain("Ibis clears the old switch");
  state.permission = { ...state.permission, granted: true };
  await act(async () => { await vi.advanceTimersByTimeAsync(RECHECK_MS); });
  expect(within(card).getByRole("status").textContent).toBe("Ibis can paste now.");
  expect(within(card).queryByRole("button", { name: "Open System Settings" })).toBeNull();
  expect(card.textContent).not.toContain("Ibis clears the old switch");
  // Once allowed, Settings stops asking every second.
  const reads = state.reads;
  await act(async () => { await vi.advanceTimersByTimeAsync(RECHECK_MS * 5); });
  expect(state.reads).toBeLessThanOrEqual(reads + 1);
});

test("a permission already given shows one quiet line, and turning pasting off hides the guide", async () => {
  const granted = fakeApi({ granted: true });
  const { unmount } = render(<SettingsApp api={granted.api} initialPage="general" />);
  expect(await screen.findByText("Allowed in Privacy & Security › Device Control and Data Access")).toBeTruthy();
  expect(screen.queryByRole("heading", { name: "Allow Ibis to paste for you" })).toBeNull();
  unmount();
  const { api, state } = fakeApi();
  render(<SettingsApp api={api} initialPage="general" />);
  await screen.findByRole("heading", { name: "Allow Ibis to paste for you" });
  fireEvent.click(screen.getByRole("switch", { name: "Paste into the app you came from" }));
  await waitFor(() => expect(state.saved.at(-1)?.paste).toBe(false));
  expect(screen.queryByRole("heading", { name: "Allow Ibis to paste for you" })).toBeNull();
});

test("systems where Ibis only copies show no pasting choices", async () => {
  const { api } = fakeApi({ needed: false, granted: true, list: "" });
  render(<SettingsApp api={api} initialPage="general" />);
  await screen.findByRole("switch", { name: "Larger text" });
  expect(screen.queryByRole("switch", { name: "Paste into the app you came from" })).toBeNull();
  expect(screen.queryByText(/Privacy & Security/)).toBeNull();
});

test("picker choices save at once, in order, keeping the others", async () => {
  const { api, state } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="general" />);
  fireEvent.click(await screen.findByRole("radio", { name: "Spacious" }));
  fireEvent.click(screen.getByRole("switch", { name: "Larger text" }));
  await waitFor(() => expect(state.saved).toHaveLength(2));
  expect(state.saved[0]).toMatchObject({ density: "spacious", larger_text: false, paste: true });
  expect(state.saved[1]).toMatchObject({ density: "spacious", larger_text: true, paste: true });
});

test("the sidebar switches pages, a page request from the app moves it, and the window has no axe violations", async () => {
  const { api, state } = fakeApi({ granted: true });
  const { container } = render(<SettingsApp api={api} initialPage="general" />);
  await screen.findByRole("switch", { name: "Larger text" });
  fireEvent.click(screen.getByRole("button", { name: "About" }));
  expect(await screen.findByText("Ibis Nightly 0.3.0")).toBeTruthy();
  expect(screen.getByRole("button", { name: "About" }).getAttribute("aria-current")).toBe("page");
  act(() => state.pages[0]!("paste"));
  expect(await screen.findByRole("switch", { name: "Paste into the app you came from" })).toBeTruthy();
  const found = await axe.run(container);
  expect(found.violations.map((v) => v.id)).toEqual([]);
});
