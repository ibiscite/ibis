import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import axe from "axe-core";
import { afterEach, expect, test, vi } from "vitest";
import type { PickerPreferences, ShortcutStatus } from "../bindings";
import { DEFAULT_PICKER_SHORTCUT, KEY_TABLE, shortcutLabel } from "../keys";
import { keyText, settingsText } from "../messages";
import { currentPlatform } from "../platform";
import { pageOf, SettingsApp } from "./SettingsApp";
import { RECHECK_MS, STALE_HINT_MS } from "./PasteGuide";
import type { PastePermission, SettingsApi } from "./api";

afterEach(() => vi.useRealTimers());

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: () => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function fakeApi(permission: Partial<PastePermission> = {}, preferences: Partial<PickerPreferences> = {}) {
  const state = {
    permission: { pastes: true, needed: true, granted: false, list: "Device Control and Data Access", ...permission },
    preferences: { citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true, search_mark: false, paste: true, ...preferences } as PickerPreferences,
    saved: [] as PickerPreferences[],
    patches: [] as Partial<PickerPreferences>[],
    opened: 0,
    reads: 0,
    pages: [] as ((page: string) => void)[],
    closes: [] as (() => Promise<void>)[],
    shortcut: { shortcut: DEFAULT_PICKER_SHORTCUT, registered: true, error: null } as ShortcutStatus,
    shortcuts: [] as ((status: ShortcutStatus) => void)[],
    shortcutChoices: [] as (string | null)[],
  };
  const commit = async (next: Partial<PickerPreferences>) => {
    state.patches.push(next);
    state.preferences = { ...state.preferences, ...next };
    state.saved.push({ ...state.preferences });
    return { preferences: state.preferences, copy_format: "pandoc" as const };
  };
  const api: SettingsApi = {
    pickerPreferences: async () => state.preferences,
    setPickerPreferences: commit,
    loginItem: async () => ({ name: "Ibis Nightly", enabled: false, problem: null }),
    setLoginItem: async (enabled) => ({ name: "Ibis Nightly", enabled, problem: null }),
    updateChecks: async () => true,
    setUpdateChecks: async () => undefined as never,
    checkForUpdates: async () => undefined as never,
    pastePermission: async () => { state.reads += 1; return state.permission; },
    openPastePermission: async () => { state.opened += 1; return undefined as never; },
    shortcutStatus: async () => state.shortcut,
    setShortcut: async (shortcut) => {
      state.shortcutChoices.push(shortcut);
      state.shortcut = { shortcut: shortcut ?? DEFAULT_PICKER_SHORTCUT, registered: true, error: null };
      for (const handler of state.shortcuts) handler(state.shortcut);
      return state.shortcut;
    },
    onShortcutStatus: async (handler) => { state.shortcuts.push(handler); return () => { state.shortcuts = state.shortcuts.filter((each) => each !== handler); }; },
    version: async () => "0.3.0",
    onPage: async (handler) => { state.pages.push(handler); return () => undefined; },
    onUpdateChecks: async () => () => undefined,
    onCloseRequested: async (handler) => { state.closes.push(handler); return () => undefined; },
  };
  return { api, state, commit };
}

test("pages are opened by name, and the paste guide is General's pasting group", () => {
  expect(pageOf("paste")).toEqual({ page: "general", focus: "paste" });
  expect(pageOf("about")).toEqual({ page: "about", focus: null });
  expect(pageOf("shortcuts")).toEqual({ page: "shortcuts", focus: null });
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
  const { api } = fakeApi({ pastes: false, needed: false, granted: true, list: "" });
  render(<SettingsApp api={api} initialPage="general" />);
  await screen.findByRole("switch", { name: "Larger text" });
  expect(screen.queryByRole("switch", { name: "Paste into the app you came from" })).toBeNull();
  expect(screen.queryByText(/Privacy & Security/)).toBeNull();
});

test("systems that paste without asking show the switch and no guide", async () => {
  const { api } = fakeApi({ needed: false, granted: true, list: "" });
  render(<SettingsApp api={api} initialPage="general" />);
  await screen.findByRole("switch", { name: "Paste into the app you came from" });
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

test.each(["success", "failure", "later edit"])("closing drains both queued choices (%s)", async (outcome) => {
  const { api, state, commit } = fakeApi({ granted: true });
  const first = deferred<null>(), second = deferred<null>();
  const save = vi.spyOn(api, "setPickerPreferences").mockImplementationOnce(async (next) => { await first.promise; return commit(next); }).mockImplementationOnce(async (next) => { await second.promise; return commit(next); });
  const read = vi.spyOn(api, "pickerPreferences");
  render(<SettingsApp api={api} initialPage="general" />);
  fireEvent.click(await screen.findByRole("radio", { name: "Spacious" }));
  if (outcome !== "later edit") fireEvent.click(screen.getByRole("switch", { name: "Larger text" }));
  await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
  const closed = vi.fn();
  const closing = state.closes[0]!().then(closed);
  if (outcome === "later edit") fireEvent.click(screen.getByRole("switch", { name: "Larger text" }));
  await act(async () => { await Promise.resolve(); });
  expect(closed).not.toHaveBeenCalled();
  await act(async () => { if (outcome === "failure") first.reject(); else first.resolve(null); });
  expect(save).toHaveBeenNthCalledWith(2, outcome === "failure" ? { density: "spacious", larger_text: true } : { larger_text: true }, null);
  expect(read).toHaveBeenCalledOnce();
  expect(closed).not.toHaveBeenCalled();
  await act(async () => { second.resolve(null); await closing; });
  expect(closed).toHaveBeenCalledOnce();
});

test("a stuck save holds closing for at most two seconds", async () => {
  const { api, state, commit } = fakeApi({ granted: true });
  const save = deferred<null>();
  vi.spyOn(api, "setPickerPreferences").mockImplementation(async (next) => { await save.promise; return commit(next); });
  render(<SettingsApp api={api} initialPage="general" />);
  const larger = await screen.findByRole("switch", { name: "Larger text" });
  vi.useFakeTimers();
  fireEvent.click(larger);
  const closed = vi.fn();
  const closing = state.closes[0]!().then(closed);
  await act(async () => { await vi.advanceTimersByTimeAsync(1_999); });
  expect(closed).not.toHaveBeenCalled();
  await act(async () => { await vi.advanceTimersByTimeAsync(1); await closing; });
  expect(closed).toHaveBeenCalledOnce();
  await act(async () => { save.resolve(null); });
});

test("a failed save reads back the saved switch and keeps the alert", async () => {
  const { api, state, commit } = fakeApi({ granted: true });
  const save = deferred<null>(), read = deferred<PickerPreferences>();
  vi.spyOn(api, "setPickerPreferences").mockImplementation(async (next) => { await save.promise; return commit(next); });
  render(<SettingsApp api={api} initialPage="general" />);
  const paste = await screen.findByRole("switch", { name: "Paste into the app you came from" });
  const reread = vi.spyOn(api, "pickerPreferences").mockReturnValue(read.promise);
  fireEvent.click(paste);
  expect((paste as HTMLInputElement).checked).toBe(false);
  await act(async () => { save.reject(); });
  expect(reread).toHaveBeenCalledOnce();
  await act(async () => { read.resolve(state.preferences); });
  expect((paste as HTMLInputElement).checked).toBe(true);
  expect(screen.getByRole("alert")).toBeTruthy();
});

test.each(["before", "during", "during, resolved after saving"])("a focus read started %s an edit cannot overwrite a local choice", async (when) => {
  const { api, state, commit } = fakeApi({ granted: true });
  const stale = { ...state.preferences };
  const read = deferred<PickerPreferences>(), save = deferred<null>();
  vi.spyOn(api, "setPickerPreferences").mockImplementationOnce(async (next) => { await save.promise; return commit(next); });
  render(<SettingsApp api={api} initialPage="general" />);
  const larger = await screen.findByRole("switch", { name: "Larger text" });
  vi.spyOn(api, "pickerPreferences").mockReturnValueOnce(read.promise);
  if (when === "before") fireEvent.focus(window);
  fireEvent.click(larger);
  if (when.startsWith("during")) fireEvent.focus(window);
  if (when !== "during") await act(async () => { save.resolve(null); });
  await act(async () => { read.resolve(stale); });
  expect((larger as HTMLInputElement).checked).toBe(true);
  fireEvent.click(screen.getByRole("radio", { name: "Spacious" }));
  await act(async () => { save.resolve(null); });
  await waitFor(() => expect(state.saved.at(-1)).toMatchObject({ density: "spacious", larger_text: true }));
});

test("a failure read cannot overwrite a newer edit", async () => {
  const { api, state, commit } = fakeApi({ granted: true });
  const saved = state.preferences;
  const save = deferred<null>(), read = deferred<PickerPreferences>();
  vi.spyOn(api, "setPickerPreferences").mockImplementationOnce(async (next) => { await save.promise; return commit(next); });
  render(<SettingsApp api={api} initialPage="general" />);
  const larger = await screen.findByRole("switch", { name: "Larger text" });
  vi.spyOn(api, "pickerPreferences").mockReturnValueOnce(read.promise);
  fireEvent.click(larger);
  await act(async () => { save.reject(); });
  fireEvent.click(screen.getByRole("radio", { name: "Spacious" }));
  await waitFor(() => expect(state.saved.at(-1)).toMatchObject({ density: "spacious", larger_text: true }));
  await act(async () => { read.resolve(saved); });
  expect((larger as HTMLInputElement).checked).toBe(true);
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

test("a Settings field patch keeps an output chosen in the other window", async () => {
  const { api, state } = fakeApi({ granted: true }, { citation_style: "apa" });
  render(<SettingsApp api={api} initialPage="general" />);
  const larger = await screen.findByRole("switch", { name: "Larger text" });
  state.preferences = { ...state.preferences, citation_style: "ieee" };
  fireEvent.click(larger);
  await waitFor(() => expect(state.patches).toEqual([{ larger_text: true }]));
  expect(state.preferences).toMatchObject({ citation_style: "ieee", larger_text: true });
});

test("the latest focus read wins even when no local edit occurred", async () => {
  const { api, state } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="general" />);
  const larger = await screen.findByRole("switch", { name: "Larger text" });
  const old = deferred<PickerPreferences>(), latest = deferred<PickerPreferences>();
  vi.spyOn(api, "pickerPreferences").mockReturnValueOnce(old.promise).mockReturnValueOnce(latest.promise);
  fireEvent.focus(window);
  fireEvent.focus(window);
  await act(async () => { latest.resolve({ ...state.preferences, larger_text: true }); });
  await act(async () => { old.resolve(state.preferences); });
  expect((larger as HTMLInputElement).checked).toBe(true);
});

test("a later equal-valued switch intent is retried when its own save fails", async () => {
  const { api, state, commit } = fakeApi({ granted: true });
  const first = deferred<void>();
  let writes = 0;
  vi.spyOn(api, "setPickerPreferences").mockImplementation(async (patch) => {
    const request = ++writes;
    if (request === 1) await first.promise;
    if (request === 3) throw new Error("synthetic third-save failure");
    return commit(patch);
  });
  render(<SettingsApp api={api} initialPage="general" />);
  const larger = await screen.findByRole("switch", { name: "Larger text" });
  fireEvent.click(larger);
  fireEvent.click(larger);
  fireEvent.click(larger);
  // Queue another field before the third write fails; it must carry the still-unsaved intent.
  fireEvent.click(screen.getByRole("radio", { name: "Spacious" }));
  await act(async () => { first.resolve(); });
  await waitFor(() => expect(writes).toBe(4));
  expect(state.preferences).toMatchObject({ larger_text: true, density: "spacious" });
  expect((larger as HTMLInputElement).checked).toBe(true);
});

test("Shortcuts records a change, restores the default, and lists every picker key without controls", async () => {
  const { api, state } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  const label = (shortcut: string) => shortcutLabel(shortcut, currentPlatform() === "macos");
  expect(recorder.textContent).toBe(label(DEFAULT_PICKER_SHORTCUT));
  expect(screen.queryByRole("button", { name: "Use Default" })).toBeNull();
  const list = screen.getByRole("region", { name: "In the picker" });
  for (const binding of KEY_TABLE) expect(within(list).getByText(keyText.actions[binding.action])).toBeTruthy();
  expect(within(list).queryAllByRole("button")).toEqual([]);
  expect(list.querySelectorAll("kbd")).toHaveLength(KEY_TABLE.length);
  fireEvent.click(recorder);
  fireEvent.keyDown(recorder, { key: "k", code: "KeyK", altKey: true, shiftKey: true });
  await waitFor(() => expect(recorder.textContent).toBe(label("Alt+Shift+K")));
  expect(state.shortcutChoices).toEqual(["Alt+Shift+K"]);
  fireEvent.click(screen.getByRole("button", { name: "Use Default" }));
  await waitFor(() => expect(recorder.textContent).toBe(label(DEFAULT_PICKER_SHORTCUT)));
  expect(state.shortcutChoices).toEqual(["Alt+Shift+K", null]);
  expect(screen.queryByRole("button", { name: "Use Default" })).toBeNull();
});

test.each(Object.entries(settingsText.shortcut.refusals))("the %s refusal explains the result and keeps the working shortcut", async (code, message) => {
  const { api, state } = fakeApi({ granted: true });
  vi.spyOn(api, "setShortcut").mockImplementationOnce(async () => {
    if (code === "not-saved") {
      state.shortcut = { shortcut: "Alt+K", registered: true, error: null };
      for (const handler of state.shortcuts) handler(state.shortcut);
    }
    throw code;
  });
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  const original = recorder.textContent;
  fireEvent.click(recorder);
  fireEvent.keyDown(recorder, { key: "k", code: "KeyK", altKey: true });
  expect((await screen.findByRole("alert")).textContent).toBe(message);
  expect(recorder.textContent).toBe(code === "not-saved" ? shortcutLabel("Alt+K", currentPlatform() === "macos") : original);
  fireEvent.click(recorder);
  fireEvent.keyDown(recorder, { key: "j", code: "KeyJ", altKey: true });
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(api.setShortcut).toHaveBeenLastCalledWith("Alt+J");
});

test("shortcut saves disable the recorder and default, and finish before Settings closes", async () => {
  const { api, state } = fakeApi({ granted: true });
  state.shortcut = { shortcut: "Alt+K", registered: true, error: null };
  const save = deferred<ShortcutStatus>();
  vi.spyOn(api, "setShortcut").mockReturnValueOnce(save.promise);
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  fireEvent.click(recorder);
  fireEvent.keyDown(recorder, { key: "j", code: "KeyJ", altKey: true });
  await waitFor(() => expect((recorder as HTMLButtonElement).disabled).toBe(true));
  expect((screen.getByRole("button", { name: "Use Default" }) as HTMLButtonElement).disabled).toBe(true);
  let closed = false;
  const close = state.closes[0]!().then(() => { closed = true; });
  await act(async () => { await Promise.resolve(); });
  expect(closed).toBe(false);
  await act(async () => { save.resolve({ shortcut: "Alt+J", registered: true, error: null }); await close; });
  expect(closed).toBe(true);
  expect((recorder as HTMLButtonElement).disabled).toBe(false);
});

test("a shortcut event wins over an older read, updates its layout note, and is cleaned up", async () => {
  const { api, state } = fakeApi({ granted: true });
  const read = deferred<ShortcutStatus>();
  vi.spyOn(api, "shortcutStatus").mockReturnValueOnce(read.promise);
  const { unmount } = render(<SettingsApp api={api} initialPage="shortcuts" />);
  await waitFor(() => expect(api.shortcutStatus).toHaveBeenCalled());
  act(() => state.shortcuts[0]!({ shortcut: "Alt+K", registered: true, error: "The shortcut uses its US keyboard position." }));
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  expect(screen.getByRole("status").textContent).toBe("The shortcut uses its US keyboard position.");
  const changed = recorder.textContent;
  await act(async () => { read.resolve(state.shortcut); });
  expect(recorder.textContent).toBe(changed);
  unmount();
  await act(async () => { await Promise.resolve(); });
  expect(state.shortcuts).toEqual([]);
});

test("a failed shortcut read is explained", async () => {
  const { api } = fakeApi({ granted: true });
  vi.spyOn(api, "shortcutStatus").mockRejectedValueOnce(new Error("unavailable"));
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  expect((await screen.findByRole("alert")).textContent).toBe(settingsText.shortcut.readFailed);
});

test("Shortcuts has no axe violations while idle, recording, or showing a refusal", async () => {
  const { api } = fakeApi({ granted: true });
  vi.spyOn(api, "setShortcut").mockRejectedValueOnce("app-shortcut");
  const { container } = render(<SettingsApp api={api} initialPage="shortcuts" />);
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  expect((await axe.run(container)).violations.map((v) => v.id)).toEqual([]);
  fireEvent.click(recorder);
  expect((await axe.run(container)).violations.map((v) => v.id)).toEqual([]);
  fireEvent.keyDown(recorder, { key: "r", code: "KeyR", metaKey: true });
  await screen.findByRole("alert");
  expect((await axe.run(container)).violations.map((v) => v.id)).toEqual([]);
});

test("a focus read started before recording cannot restore the old shortcut", async () => {
  const { api, state } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  const read = deferred<ShortcutStatus>();
  const old = state.shortcut;
  vi.spyOn(api, "shortcutStatus").mockReturnValueOnce(read.promise);
  fireEvent.focus(window);
  fireEvent.click(recorder);
  fireEvent.keyDown(recorder, { key: "k", code: "KeyK", altKey: true });
  await waitFor(() => expect(state.shortcutChoices).toEqual(["Alt+K"]));
  await act(async () => { read.resolve(old); });
  expect(recorder.textContent).toBe(shortcutLabel("Alt+K", currentPlatform() === "macos"));
});

test("default aliases and modifier order do not show Use Default", async () => {
  const { api, state } = fakeApi({ granted: true });
  state.shortcut = { shortcut: "Shift+CommandOrControl+KeyY", registered: true, error: null };
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  await screen.findByRole("button", { name: /^Open Ibis:/ });
  expect(screen.queryByRole("button", { name: "Use Default" })).toBeNull();
});
