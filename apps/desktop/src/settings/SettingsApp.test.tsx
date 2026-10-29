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
import recordings from "../testing/shortcut-recordings.json";
import pickerConflicts from "../testing/picker-shortcut-conflicts.json";

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
    finished: 0,
    pages: [] as ((page: string) => void)[],
    closes: [] as (() => Promise<void>)[],
    shortcut: { shortcut: DEFAULT_PICKER_SHORTCUT, registered: true, error: null } as ShortcutStatus,
    shortcuts: [] as ((status: ShortcutStatus) => void)[],
    shortcutChoices: [] as (string | null)[],
    automatic: [] as ((on: boolean) => void)[],
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
    dragPasteApp: vi.fn(async () => undefined),
    finishPasteGuide: async () => { state.finished += 1; return true; },
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
    onUpdateChecks: async (handler) => { state.automatic.push(handler); return () => { state.automatic = state.automatic.filter((each) => each !== handler); }; },
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

test.each([
  ["initial", "success"], ["initial", "failure"],
  ["focus", "success"], ["focus", "failure"],
])("an automatic-update event wins over an older %s read's %s", async (from, outcome) => {
  const { api, state } = fakeApi({ granted: true });
  const read = deferred<boolean>();
  if (from === "initial") vi.spyOn(api, "updateChecks").mockReturnValueOnce(read.promise);
  const { unmount } = render(<SettingsApp api={api} initialPage="about" />);
  if (from === "focus") {
    await screen.findByRole("switch", { name: settingsText.updates.automatic });
    vi.spyOn(api, "updateChecks").mockReturnValueOnce(read.promise);
    fireEvent.focus(window);
  }
  act(() => state.automatic[0]!(true));
  const automatic = screen.getByRole("switch", { name: settingsText.updates.automatic }) as HTMLInputElement;
  expect(automatic.checked).toBe(true);
  await act(async () => { if (outcome === "success") read.resolve(false); else read.reject(); });
  expect(screen.getByRole("switch", { name: settingsText.updates.automatic })).toBe(automatic);
  expect(automatic.checked).toBe(true);
  // A read started after the event can still observe a later saved choice.
  vi.spyOn(api, "updateChecks").mockResolvedValueOnce(false);
  fireEvent.focus(window);
  await waitFor(() => expect(automatic.checked).toBe(false));
  unmount();
  await act(async () => { await Promise.resolve(); });
  expect(state.automatic).toEqual([]);
});

test.each(["success", "failure"])("a newer automatic-update read wins over an older read's %s", async (outcome) => {
  const { api } = fakeApi({ granted: true });
  const old = deferred<boolean>(), latest = deferred<boolean>();
  vi.spyOn(api, "updateChecks").mockReturnValueOnce(old.promise).mockReturnValueOnce(latest.promise);
  render(<SettingsApp api={api} initialPage="about" />);
  fireEvent.focus(window);
  await act(async () => { latest.resolve(true); });
  const automatic = screen.getByRole("switch", { name: settingsText.updates.automatic }) as HTMLInputElement;
  await act(async () => { if (outcome === "success") old.resolve(false); else old.reject(); });
  expect(screen.getByRole("switch", { name: settingsText.updates.automatic })).toBe(automatic);
  expect(automatic.checked).toBe(true);
});

test("Settings opens at the top of General with every built page in a navigable, selected sidebar list", async () => {
  const { api, state } = fakeApi();
  render(<SettingsApp api={api} initialPage={null} />);
  await screen.findByRole("heading", { name: "Allow Ibis to paste for you" });
  const sidebar = screen.getByRole("navigation", { name: "Settings pages" });
  const list = within(sidebar).getByRole("listbox", { name: "Settings pages" });
  const items = within(list).getAllByRole("option");
  expect(items.map((item) => item.textContent)).toEqual(["General", "Shortcuts", "About"]);
  const selected = () => within(list).getAllByRole("option", { selected: true });
  expect(selected().map((item) => item.textContent)).toEqual(["General"]);
  expect(selected()[0]!.getAttribute("aria-current")).toBe("page");
  const main = screen.getByRole("main", { name: "General" });
  expect(main.firstElementChild).toBe(within(main).getByRole("heading", { name: "General", level: 1 }));
  expect(main.scrollTop).toBe(0);
  // Reopening the current page must reset it too, even though its name did not change.
  main.scrollTop = 320;
  act(() => state.pages[0]!("general"));
  expect(main.scrollTop).toBe(0);
  main.scrollTop = 180;
  fireEvent.click(within(list).getByRole("option", { name: "About" }));
  expect(screen.getByRole("main", { name: "About" }).scrollTop).toBe(0);
  expect(selected().map((item) => item.textContent)).toEqual(["About"]);
  fireEvent.click(within(list).getByRole("option", { name: "General" }));
  expect(screen.getByRole("main", { name: "General" }).scrollTop).toBe(0);
  expect(selected().map((item) => item.textContent)).toEqual(["General"]);
});

test("sidebar arrows and Home/End move focus, select the page and reset its scroll", async () => {
  const { api } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="general" />);
  await screen.findByRole("switch", { name: "Larger text" });
  const general = screen.getByRole("option", { name: "General" });
  const shortcuts = screen.getByRole("option", { name: "Shortcuts" });
  const about = screen.getByRole("option", { name: "About" });
  act(() => general.focus());
  for (const [key, target, name] of [
    ["ArrowDown", shortcuts, "Shortcuts"], ["ArrowDown", about, "About"],
    ["ArrowUp", shortcuts, "Shortcuts"], ["ArrowUp", general, "General"],
    ["End", about, "About"], ["Home", general, "General"],
  ] as const) {
    screen.getByRole("main").scrollTop = 120;
    fireEvent.keyDown(document.activeElement!, { key });
    expect(document.activeElement).toBe(target);
    expect(target.getAttribute("aria-current")).toBe("page");
    expect(target.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("main", { name }).scrollTop).toBe(0);
  }
});

test("clicking the current sidebar page once resets its scroll to the top", async () => {
  const { api } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="general" />);
  await screen.findByRole("switch", { name: "Larger text" });
  for (const name of ["General", "Shortcuts", "About"]) {
    const item = screen.getByRole("option", { name });
    fireEvent.click(item);
    const main = screen.getByRole("main", { name });
    main.scrollTop = 240;
    fireEvent.pointerDown(item, { button: 0, buttons: 1, pointerType: "mouse", pointerId: 1 });
    fireEvent.pointerUp(item, { button: 0, buttons: 0, pointerType: "mouse", pointerId: 1 });
    fireEvent.click(item, { button: 0, detail: 1 });
    expect(main.scrollTop).toBe(0);
    expect(item.getAttribute("aria-current")).toBe("page");
  }
});

test("the title-bar drag region is outside the page scroller and sidebar list", async () => {
  const { api } = fakeApi();
  const { container } = render(<SettingsApp api={api} initialPage={null} />);
  await screen.findByRole("switch", { name: "Larger text" });
  const titlebar = container.querySelector("[data-tauri-drag-region]")!;
  expect(titlebar.parentElement?.className).toBe("settings");
  expect(screen.getByRole("main").contains(titlebar)).toBe(false);
  expect(screen.getByRole("navigation").contains(titlebar)).toBe(false);
  expect(container.querySelectorAll("[data-tauri-drag-region]")).toHaveLength(1);
});

test("a paste deep link waits for permission data and scrolls only the page", async () => {
  const { api } = fakeApi();
  const permission = deferred<PastePermission>();
  vi.spyOn(api, "pastePermission").mockReturnValue(permission.promise);
  const rect = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    return { top: this.id === "paste" ? 180 : 52 } as DOMRect;
  });
  // This API can scroll every ancestor, including the document; the shell must never use it.
  const previous = Object.getOwnPropertyDescriptor(Element.prototype, "scrollIntoView");
  const scrollAncestors = vi.fn();
  Object.defineProperty(Element.prototype, "scrollIntoView", { configurable: true, value: scrollAncestors });
  try {
    render(<SettingsApp api={api} initialPage="paste" />);
    await screen.findByRole("switch", { name: "Larger text" });
    const main = screen.getByRole("main");
    expect(main.scrollTop).toBe(0);
    await act(async () => { permission.resolve({ pastes: true, needed: true, granted: false, list: "Accessibility" }); });
    expect(main.scrollTop).toBe(128);
    expect(scrollAncestors).not.toHaveBeenCalled();
    expect(document.documentElement.scrollTop).toBe(0);
    expect(document.body.scrollTop).toBe(0);
  } finally {
    rect.mockRestore();
    if (previous) Object.defineProperty(Element.prototype, "scrollIntoView", previous);
    else Reflect.deleteProperty(Element.prototype, "scrollIntoView");
  }
});

test("the paste guide names the list, opens it, notices the grant on its own and says so", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const { api, state } = fakeApi();
  render(<SettingsApp api={api} initialPage="general" />);
  const guide = await screen.findByRole("heading", { name: "Allow Ibis to paste for you" });
  const card = guide.closest(".paste-guide") as HTMLElement;
  expect(card.textContent).toContain("In Privacy & Security › Device Control and Data Access, turn on Ibis Nightly.");
  expect(card.textContent).toContain("To paste, allow Ibis in macOS;");
  expect(card.textContent).not.toContain("Zotero");
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

test("a drag fallback notices a read-back grant on return without repeating automatic listing", async () => {
  const { api, state } = fakeApi();
  render(<SettingsApp api={api} initialPage="general" />);
  const open = await screen.findByRole("button", { name: "Open System Settings" });
  fireEvent.click(open);
  fireEvent.dragStart(screen.getByRole("button", { name: "Drag Ibis Nightly to Device Control and Data Access" }));
  await act(async () => undefined);
  expect(api.dragPasteApp).toHaveBeenCalledExactlyOnceWith();
  expect(state.opened).toBe(1);
  expect(screen.queryByText("Ibis can paste now.")).toBeNull();
  state.permission = { ...state.permission, granted: true };
  fireEvent.focus(window);
  expect(await screen.findByText("Ibis can paste now.")).toBeTruthy();
  expect(screen.queryByRole("button", { name: /Drag Ibis/ })).toBeNull();
  expect(state.finished).toBe(0);
});

test("the permission detour finishes only on return after a read-back grant", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const { api, state } = fakeApi();
  render(<SettingsApp api={api} initialPage="paste" />);
  await screen.findByRole("heading", { name: "Allow Ibis to paste for you" });
  fireEvent.blur(window);
  state.permission = { ...state.permission, granted: true };
  await act(async () => { await vi.advanceTimersByTimeAsync(RECHECK_MS); });
  expect(state.finished).toBe(0);
  fireEvent.focus(window);
  await act(async () => { await Promise.resolve(); });
  expect(state.finished).toBe(1);
});

test("an ungranted return and an ordinary Settings visit never finish a detour", async () => {
  const detour = fakeApi();
  const { unmount } = render(<SettingsApp api={detour.api} initialPage="paste" />);
  await screen.findByRole("heading", { name: "Allow Ibis to paste for you" });
  fireEvent.focus(window);
  expect(detour.state.finished).toBe(0);
  unmount();
  const ordinary = fakeApi({ granted: true });
  render(<SettingsApp api={ordinary.api} initialPage="general" />);
  await screen.findByText(/Allowed in Privacy/);
  fireEvent.focus(window);
  expect(ordinary.state.finished).toBe(0);
});

test.each(["saved", "stuck"])("returning from the guide drains a %s preference save for at most two seconds", async (outcome) => {
  const { api, state, commit } = fakeApi();
  const save = deferred<null>();
  vi.spyOn(api, "setPickerPreferences").mockImplementation(async (next) => { await save.promise; return commit(next); });
  render(<SettingsApp api={api} initialPage="general" />);
  const larger = await screen.findByRole("switch", { name: "Larger text" });
  vi.useFakeTimers();
  fireEvent.click(larger);
  act(() => state.pages[0]!("paste"));
  state.permission = { ...state.permission, granted: true };
  await act(async () => { fireEvent.focus(window); });
  expect(state.finished).toBe(0);
  if (outcome === "saved") await act(async () => save.resolve(null));
  else await act(async () => { await vi.advanceTimersByTimeAsync(2_000); });
  expect(state.finished).toBe(1);
  await act(async () => save.resolve(null));
});

test("a failed guide return keeps recovery instructions in Settings", async () => {
  const { api, state } = fakeApi();
  vi.spyOn(api, "finishPasteGuide").mockRejectedValue(new Error("synthetic window failure"));
  render(<SettingsApp api={api} initialPage="paste" />);
  await screen.findByRole("heading", { name: "Allow Ibis to paste for you" });
  state.permission = { ...state.permission, granted: true };
  fireEvent.focus(window);
  expect((await screen.findByRole("alert")).textContent).toBe("Couldn't return to your citation. Open Ibis with your shortcut to continue.");
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
  fireEvent.click(screen.getByRole("option", { name: "About" }));
  expect(await screen.findByText("Ibis Nightly 0.3.0")).toBeTruthy();
  expect(screen.getByRole("option", { name: "About" }).getAttribute("aria-current")).toBe("page");
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

test.each(pickerConflicts)("a recorded $name refusal names its command and keeps the working shortcut", async ({ event, shortcut, command, label }) => {
  const { api } = fakeApi({ granted: true });
  vi.spyOn(api, "setShortcut").mockRejectedValueOnce({ "picker-shortcut": command });
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  const original = recorder.textContent;
  fireEvent.click(recorder);
  fireEvent.keyDown(recorder, event);
  expect((await screen.findByRole("alert")).textContent).toBe(`That shortcut is used for ${label}. Try another shortcut.`);
  expect(api.setShortcut).toHaveBeenCalledWith(shortcut);
  expect(recorder.textContent).toBe(original);
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
  state.shortcut = { shortcut: "Shift+CommandOrControl+Y", registered: true, error: null };
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  await screen.findByRole("button", { name: /^Open Ibis:/ });
  expect(screen.queryByRole("button", { name: "Use Default" })).toBeNull();
});

// Rust consumes the same cases through validation, saving, reloading and
// native registration, so the recorder's accelerator syntax cannot drift.
test.each(recordings)("recorder-to-registration: $name", async ({ event, shortcut }) => {
  const { api, state } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  fireEvent.click(recorder);
  fireEvent.keyDown(recorder, event);
  await waitFor(() => expect(state.shortcutChoices).toEqual([shortcut]));
});

test("a physical default-position key still offers the logical default", async () => {
  const { api, state } = fakeApi({ granted: true });
  state.shortcut = { shortcut: "CommandOrControl+Shift+KeyY", registered: true, error: null };
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  await screen.findByRole("button", { name: /^Open Ibis:/ });
  fireEvent.click(screen.getByRole("button", { name: "Use Default" }));
  await waitFor(() => expect(state.shortcutChoices).toEqual([null]));
});
