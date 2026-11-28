import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import axe from "axe-core";
import { afterEach, expect, onTestFinished, test, vi } from "vitest";
import type { PickerPreferences, PickerSave, ShortcutStatus, WindowSize } from "../bindings";
import { bindingKeys, DEFAULT_PICKER_SHORTCUT, KEY_TABLE, shortcutLabel } from "../keys";
import { currentPlatform } from "../platform";
import { SettingsApp } from "./SettingsApp";
import { RECHECK_MS, STALE_HINT_MS } from "./PasteGuide";
import type { PastePermission, SettingsApi } from "./api";

/** The owner's save, by patch and receipt, so a test can hold or refuse one. */
type TestApi = SettingsApi & { setPickerPreferences: (patch: Partial<PickerPreferences>, receipt: number) => Promise<PickerSave> };
import english from "../../../../locales/en-US.ftl?raw";
import { createInterfaceMessages, InterfaceMessagesProvider, say } from "../i18n";
import { SHORTCUT_ACTIONS } from "../i18n/settings";
import { deferred } from "../testing/deferred";

afterEach(() => { vi.useRealTimers(); localStorage.clear(); });

function fakeApi(permission: Partial<PastePermission> = {}, preferences: Partial<PickerPreferences> = {}) {
  const state = {
    permission: { pastes: true, needed: true, granted: false, list: "Device Control and Data Access", ...permission },
    preferences: { output: "pandoc", density: "comfortable", larger_text: false, paste: true, menu_bar: true, ...preferences } as PickerPreferences,
    saved: [] as PickerPreferences[],
    patches: [] as Partial<PickerPreferences>[],
    opened: 0,
    reads: 0,
    revision: 0,
    finished: 0,
    windowSaved: undefined as ((saved: boolean, revision: number) => void) | undefined,
    windowRevision: 0,
    pages: [] as ((page: string) => void)[],
    closes: [] as (() => Promise<void>)[],
    shortcut: { shortcut: DEFAULT_PICKER_SHORTCUT, registered: true, error: null } as ShortcutStatus,
    shortcuts: [] as ((status: ShortcutStatus) => void)[],
    shortcutChoices: [] as (string | null)[],
    configured: 0,
    login: false,
    updates: true,
    preferenceEvents: [] as ((revision: number) => void)[],
    automatic: [] as ((on: boolean) => void)[],
  };
  const commit = async (next: Partial<PickerPreferences>) => {
    state.patches.push(next);
    state.preferences = { ...state.preferences, ...next };
    state.saved.push({ ...state.preferences });
    return { preferences: state.preferences };
  };
  /** Each choice reaches the owner when made; how it went is asked for by receipt. */
  const submitted = new Map<number, Partial<PickerPreferences>>();
  const api: TestApi = {
    attachmentTextLibraries: async () => [],
    setAttachmentText: async (_id, on) => on,
    attachmentTextOutsideLibrary: async () => false,
    setAttachmentTextOutsideLibrary: async (on) => on,
    pickerPreferences: async () => state.preferences,
    windowSize: vi.fn(async (): Promise<WindowSize | null> => "medium"),
    setWindowSize: vi.fn(async (): Promise<number> => ++state.windowRevision),
    onWindowSaved: vi.fn(async (handler: (saved: boolean, revision: number) => void) => { state.windowSaved = handler; return () => undefined; }),
    savePickerPreferences: async (patch) => { const receipt = ++state.revision; submitted.set(receipt, patch as Partial<PickerPreferences>); return receipt; },
    pickerPreferencesSaved: async (receipt) => api.setPickerPreferences(submitted.get(receipt)!, receipt),
    setPickerPreferences: commit,
    backUpSettings: vi.fn(async () => null),
    chooseSettingsBackup: vi.fn(async () => null),
    restoreSettingsBackup: vi.fn(async () => null),
    cancelSettingsRestore: vi.fn(async () => null),
    loginItem: async () => ({ name: "Ibis Nightly", enabled: state.login, problem: null }),
    setLoginItem: async (enabled) => { state.login = enabled; return { name: "Ibis Nightly", enabled, problem: null }; },
    updateChecks: async () => state.updates,
    setUpdateChecks: async (on) => { state.updates = on; for (const changed of state.automatic) changed(on); return undefined as never; },
    openAboutLink: vi.fn(async () => undefined),
    copyVersion: vi.fn(async () => null),
  checkForUpdates: async () => undefined as never,
    pastePermission: async () => { state.reads += 1; return state.permission; },
    openPastePermission: async () => { state.opened += 1; return undefined as never; },
    dragPasteApp: vi.fn(async () => undefined),
    finishPasteGuide: async () => { state.finished += 1; return true; },
    shortcutStatus: async () => state.shortcut,
    configureShortcuts: async () => { state.configured += 1; return null; },
    setShortcut: async (shortcut) => {
      state.shortcutChoices.push(shortcut);
      state.shortcut = { shortcut: shortcut ?? DEFAULT_PICKER_SHORTCUT, registered: true, error: null };
      for (const handler of state.shortcuts) handler(state.shortcut);
      return state.shortcut;
    },
    onShortcutStatus: async (handler) => { state.shortcuts.push(handler); return () => { state.shortcuts = state.shortcuts.filter((each) => each !== handler); }; },
    version: async () => "0.3.0",
    onPreferencesChanged: async (changed) => { state.preferenceEvents.push(changed); return () => { state.preferenceEvents = state.preferenceEvents.filter((each) => each !== changed); }; },
    onSettingsChanged: async (changed) => { state.preferenceEvents.push(changed); return () => { state.preferenceEvents = state.preferenceEvents.filter((each) => each !== changed); }; },
    onPage: async (handler) => { state.pages.push(handler); return () => undefined; },
    onUpdateChecks: async (handler) => { state.automatic.push(handler); return () => { state.automatic = state.automatic.filter((each) => each !== handler); }; },
    onCloseRequested: async (handler) => { state.closes.push(handler); return () => undefined; },
  };
  return { api, state, commit };
}

test("Settings opens at the top of General with every built page in a navigable, selected sidebar list", async () => {
  const { api, state } = fakeApi();
  render(<SettingsApp api={api} initialPage={null} />);
  await screen.findByRole("heading", { name: "Allow Ibis to paste for you" });
  const sidebar = screen.getByRole("navigation", { name: "Settings pages" });
  const list = within(sidebar).getByRole("listbox", { name: "Settings pages" });
  const items = within(list).getAllByRole("option");
  expect(items.map((item) => item.textContent)).toEqual(["General", "Appearance", "Citations", "AI Apps", "Shortcuts", "About Ibis"]);
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
  fireEvent.click(within(list).getByRole("option", { name: "About Ibis" }));
  expect(screen.getByRole("main", { name: "About Ibis" }).scrollTop).toBe(0);
  expect(selected().map((item) => item.textContent)).toEqual(["About Ibis"]);
  fireEvent.click(within(list).getByRole("option", { name: "General" }));
  expect(screen.getByRole("main", { name: "General" }).scrollTop).toBe(0);
  expect(selected().map((item) => item.textContent)).toEqual(["General"]);
});

test("the paste guide names the list, opens it, notices the grant on its own and says so", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const { api, state } = fakeApi();
  render(<SettingsApp api={api} initialPage="general" />);
  const guide = await screen.findByRole("heading", { name: "Allow Ibis to paste for you" });
  const card = guide.closest(".ui-paste-guide") as HTMLElement;
  expect(card.textContent).toContain("In Privacy & Security › Device Control and Data Access, turn on Ibis Nightly.");
  expect(card.textContent).toContain("Ibis needs Accessibility access to check which window has the keyboard");
  expect(card.textContent).not.toContain("Zotero");
  fireEvent.click(within(card).getByRole("button", { name: "Open System Settings" }));
  expect(state.opened).toBe(1);
  expect(within(card).getByRole("status").textContent).toBe("Waiting for your OK…");
  // Still waiting after a while: say how to clear a stale switch.
  await act(async () => { await vi.advanceTimersByTimeAsync(STALE_HINT_MS); });
  expect(card.textContent).toContain("remove Ibis Nightly from the list, add this copy again, then turn it on");
  state.permission = { ...state.permission, granted: true };
  await act(async () => { await vi.advanceTimersByTimeAsync(RECHECK_MS); });
  expect(within(card).getByRole("status").textContent).toBe("Pasting is on.");
  expect(within(card).queryByRole("button", { name: "Open System Settings" })).toBeNull();
  expect(card.textContent).not.toContain("remove Ibis Nightly from the list");
  // Once allowed, Settings stops asking every second.
  const reads = state.reads;
  await act(async () => { await vi.advanceTimersByTimeAsync(RECHECK_MS * 5); });
  expect(state.reads).toBeLessThanOrEqual(reads + 1);
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

test("the sidebar switches pages, a page request from the app moves it, and the window has no axe violations", async () => {
  const { api, state } = fakeApi({ granted: true });
  const { container } = render(<SettingsApp api={api} initialPage="general" />);
  await screen.findByRole("button", { name: /^Open Ibis:/ });
  fireEvent.click(screen.getByRole("option", { name: "About Ibis" }));
  expect(await screen.findByText("Version 0.3.0")).toBeTruthy();
  expect(screen.getByRole("option", { name: "About Ibis" }).getAttribute("aria-current")).toBe("page");
  act(() => state.pages[0]!("paste"));
  expect(await screen.findByRole("switch", { name: "Paste into the app you came from" })).toBeTruthy();
  const found = await axe.run(container);
  expect(found.violations.map((v) => v.id)).toEqual([]);
});

test("Shortcuts records a change, restores the default, and lists every picker key without controls", async () => {
  const { api, state } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  const label = (shortcut: string) => shortcutLabel(shortcut, currentPlatform() === "macos");
  expect(recorder.textContent).toBe(label(DEFAULT_PICKER_SHORTCUT));
  expect(screen.queryByRole("button", { name: "Use Default for Open Ibis" })).toBeNull();
  expect(document.querySelector("details, summary")).toBeNull();
  const lists = [...document.querySelectorAll<HTMLElement>('tbody[id^="shortcuts-"]:not(#shortcuts-system)')];
  const bindings = KEY_TABLE.filter((binding) => bindingKeys(binding, currentPlatform() === "macos").length);
  for (const binding of bindings) expect(screen.getByText(say(SHORTCUT_ACTIONS[binding.action]))).toBeTruthy();
  expect(lists.flatMap((list) => within(list).queryAllByRole("button"))).toEqual([]);
  expect(lists.flatMap((list) => [...list.querySelectorAll(".ui-settings-keys")])).toHaveLength(bindings.length);
  fireEvent.click(recorder);
  fireEvent.keyDown(recorder, { key: "k", code: "KeyK", altKey: true, shiftKey: true });
  await waitFor(() => expect(recorder.textContent).toBe(label("Alt+Shift+K")));
  expect(state.shortcutChoices).toEqual(["Alt+Shift+K"]);
  fireEvent.click(screen.getByRole("button", { name: "Use Default for Open Ibis" }));
  await waitFor(() => expect(recorder.textContent).toBe(label(DEFAULT_PICKER_SHORTCUT)));
  expect(state.shortcutChoices).toEqual(["Alt+Shift+K", null]);
  expect(screen.queryByRole("button", { name: "Use Default for Open Ibis" })).toBeNull();
});

test.each(["general", "shortcuts"])("%s has no axe violations while idle, recording, or showing a refusal", async (page) => {
  const { api } = fakeApi({ granted: true });
  vi.spyOn(api, "setShortcut").mockRejectedValueOnce(shortcutFailure("app-shortcut"));
  const { container } = render(<SettingsApp api={api} initialPage={page} />);
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  expect((await axe.run(container)).violations.map((v) => v.id)).toEqual([]);
  fireEvent.click(recorder);
  expect((await axe.run(container)).violations.map((v) => v.id)).toEqual([]);
  fireEvent.keyDown(recorder, { key: "r", code: "KeyR", metaKey: true });
  await screen.findByRole("alert");
  expect((await axe.run(container)).violations.map((v) => v.id)).toEqual([]);
});

test("search finds theme by one Appearance name, then reveals and briefly tints the setting", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const { api } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="general" />);
  expect(screen.getAllByRole("option").map((x) => x.textContent)).toEqual(["General", "Appearance", "Citations", "AI Apps", "Shortcuts", "About Ibis"]);
  const search = screen.getByRole("searchbox", { name: "Search Settings" });
  fireEvent.change(search, { target: { value: "theme" } });
  const results = await screen.findByRole("listbox", { name: "Search results" });
  const appearance = within(results).getByRole("option", { name: "Appearance" });
  expect(within(results).getAllByRole("option")).toEqual([appearance]);
  fireEvent.click(appearance);
  expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Appearance");
  expect(document.getElementById("appearance")!.closest(".ui-row")!.hasAttribute("data-highlighted")).toBe(true);
  await act(() => vi.advanceTimersByTimeAsync(2000));
  expect(document.getElementById("appearance")!.closest(".ui-row")!.hasAttribute("data-highlighted")).toBe(false);
});

test("search results are sidebar rows, and Enter opens the first", async () => {
  const { api } = fakeApi({ granted: true });
  const { container } = render(<SettingsApp api={api} initialPage="general" />);
  const search = screen.getByRole("searchbox", { name: "Search Settings" });
  fireEvent.change(search, { target: { value: "a" } });
  const results = await screen.findByRole("listbox", { name: "Search results" });
  expect(within(results).queryAllByRole("button")).toEqual([]);
  const row = within(results).getByRole("option", { name: "Appearance" });
  expect(row.classList.contains("ui-side-navigation-item")).toBe(true);
  expect(row.querySelector('.ui-side-navigation-tile[data-page="look"] svg')).not.toBeNull();
  fireEvent.keyDown(search, { key: "Enter", isComposing: true });
  expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("General");
  fireEvent.change(search, { target: { value: "appearance" } });
  fireEvent.keyDown(search, { key: "Enter" });
  expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Appearance");
  expect(container.querySelector(".ui-settings-search-results")).toBeNull();
  fireEvent.change(search, { target: { value: "no such setting" } });
  expect((await screen.findByText("No settings match ‘no such setting’")).classList.contains("ui-side-navigation-empty")).toBe(true);
});

// Decision 0062: a search jump keeps the focused control in what the person
// sees of the page, under its sticky title, however late content above it lands
// (the case found in review: 1.7 s later, a block twice the window's height).
test("a search jump keeps the focused setting in view while slow content lands above it", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const { container } = render(<SettingsApp api={fakeApi({ granted: true }).api} initialPage="general" />);
  const page = container.querySelector<HTMLElement>("main.settings-page")!;
  const TITLE = 48, HEIGHT = 560;
  let late: HTMLElement | null = null;
  // jsdom applies no stylesheet: the title is sticky, as SettingsLayout.css makes it.
  page.querySelector<HTMLElement>(".ui-settings-page-title")!.style.position = "sticky";
  // jsdom lays nothing out: each element sits 2 px below the one before it in
  // the page, and everything after the late block 2 windows lower.
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this === page) return new DOMRect(0, 0, 700, HEIGHT);
    if (this.matches(".ui-settings-page-title")) return new DOMRect(0, 0, 700, TITLE);
    if (!page.contains(this)) return new DOMRect();
    const order = [...page.querySelectorAll("*")].indexOf(this);
    const pushed = late && late !== this && late.compareDocumentPosition(this) & Node.DOCUMENT_POSITION_FOLLOWING ? 2 * HEIGHT : 0;
    return new DOMRect(0, TITLE + order * 2 + pushed - page.scrollTop, 700, 24);
  });
  const search = screen.getByRole("searchbox", { name: "Search Settings" });
  fireEvent.change(search, { target: { value: "window size" } });
  fireEvent.keyDown(search, { key: "Enter" });
  const medium = await screen.findByRole("radio", { name: "Medium" });
  await waitFor(() => expect(document.activeElement).toBe(medium));
  const row = document.getElementById("window-size")!.closest<HTMLElement>(".ui-row")!;
  expect(row.getBoundingClientRect().top).toBe(TITLE);
  await act(() => vi.advanceTimersByTimeAsync(1700));
  late = document.createElement("div");
  await act(async () => { page.querySelector(".settings-page-content")!.prepend(late!); });
  expect(row.getBoundingClientRect().top).toBe(TITLE);
  expect(document.activeElement).toBe(medium);
});

test("Appearance, both accents and both text sizes persist separately", async () => {
  const { api, state } = fakeApi({ granted: true });
  const mounted = render(<SettingsApp api={api} initialPage="look" />);
  await screen.findByRole("radiogroup", { name: "Accent" });
  fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
  fireEvent.click(screen.getByRole("radio", { name: "Ibis scarlet" }));
  fireEvent.click(screen.getByRole("radio", { name: "Larger" }));
  await act(async () => state.closes[0]!());
  expect(state.preferences).toMatchObject({ appearance: "dark", accent: "ibis", larger_text: true });
  expect(state.patches.at(0)).toEqual({ appearance: "dark" });
  expect(state.patches.at(1)).toEqual({ accent: "ibis" });
  mounted.unmount();
  render(<SettingsApp api={api} initialPage={null} />);
  expect(screen.getByRole("heading", { level: 1 })).toHaveProperty("textContent", "General");
  fireEvent.click(screen.getByRole("option", { name: "Appearance" }));
  expect(await screen.findByRole("radio", { name: "Ibis scarlet" })).toHaveProperty("checked", true);
  expect(document.documentElement.dataset.appearance).toBe("dark");
  fireEvent.click(screen.getByRole("radio", { name: "System" }));
  fireEvent.click(screen.getByRole("radio", { name: "Follow the system" }));
  fireEvent.click(screen.getByRole("radio", { name: "Default" }));
  await act(async () => state.closes.at(-1)!());
  expect(state.preferences).toMatchObject({ appearance: "system", accent: "system", larger_text: false });
});

test("the Ibis default-format menu saves one output by its name, restores refusals, and reopens on General", async () => {
  const { api, state } = fakeApi({ granted: true });
  const mounted = render(<SettingsApp api={api} initialPage="citations" />);
  const choose = async (name: string) => {
    fireEvent.click(await screen.findByRole("button", { name: /^In Word, LibreOffice and other documents / }));
    expect(await screen.findByRole("menu")).toBeTruthy();
    fireEvent.click(screen.getByRole("menuitemradio", { name }));
    await act(async () => state.closes.at(-1)!());
  };
  await choose("MLA");
  expect(state.preferences).toMatchObject({ output: "modern-language-association" });
  await choose("APA");
  expect(state.preferences.output).toBe("apa");
  vi.spyOn(api, "setPickerPreferences").mockRejectedValueOnce(new Error("synthetic refusal"));
  await choose("MLA");
  await screen.findByRole("alert");
  expect(screen.getByRole("button", { name: "In Word, LibreOffice and other documents APA" })).toBeTruthy();
  expect(document.querySelector("select")).toBeNull();
  mounted.unmount();
  render(<SettingsApp api={api} initialPage={null} />);
  expect(screen.getByRole("heading", { level: 1 })).toHaveProperty("textContent", "General");
  fireEvent.click(screen.getByRole("option", { name: "Citations" }));
  expect(await screen.findByRole("button", { name: "In Word, LibreOffice and other documents APA" })).toBeTruthy();
});

test("Citations shows both formats with nothing saved, key formats first", async () => {
  const { api } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="citations" />);
  const keys = await screen.findByRole("button", { name: /^In LaTeX, Markdown and Obsidian / });
  const documents = screen.getByRole("button", { name: /^In Word, LibreOffice and other documents / });
  expect(keys.compareDocumentPosition(documents) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

test("a read-only refusal names the file constraint beside the restored control", async () => {
  const { api } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="look" />);
  const choice = await screen.findByRole("radio", { name: "Compact" });
  vi.spyOn(api, "setPickerPreferences").mockRejectedValueOnce({ code: "settings-read-only", message: { id: "app-settings-error-read-only", args: {} }, recovery: { action: "review-settings" }, focus: "settings" });
  fireEvent.click(choice);
  const problem = await screen.findByRole("alert");
  expect(problem).toHaveProperty("textContent", "Settings are read-only. Make the settings file writable before trying again.");
  expect(problem.closest(".ui-row")?.querySelector("#rows")).toBeTruthy();
  expect(screen.getByRole("radio", { name: "Comfortable" })).toHaveProperty("checked", true);
});

test("General cancels recording on Escape, Tab and page changes without saving", async () => {
  const { api, state } = fakeApi();
  render(<SettingsApp api={api} initialPage="general" />);
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  const original = recorder.textContent;
  for (const key of ["Escape", "Tab"]) {
    fireEvent.click(recorder);
    fireEvent.keyDown(recorder, { key, code: key });
    expect(recorder.textContent).toBe(original);
  }
  fireEvent.click(recorder);
  fireEvent.click(screen.getByRole("option", { name: "Shortcuts" }));
  expect(screen.getByRole("button", { name: /^Open Ibis:/ }).getAttribute("aria-pressed")).toBe("false");
  fireEvent.click(screen.getByRole("option", { name: "General" }));
  expect(screen.getByRole("button", { name: /^Open Ibis:/ }).textContent).toBe(original);
  expect(state.shortcutChoices).toEqual([]);
});

test("About copies the version natively, never through the web view's clipboard, and says so", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  const copyVersion = vi.fn().mockResolvedValue(null);
  const pseudo = createInterfaceMessages(english, true);
  render(<InterfaceMessagesProvider messages={pseudo}><SettingsApp api={{ ...fakeApi().api, copyVersion }} initialPage="about" /></InterfaceMessagesProvider>);
  fireEvent.click(await screen.findByRole("button", { name: pseudo.text("about-version", { version: "0.3.0" }) }));
  await screen.findByRole("button", { name: pseudo.text("about-copied") });
  expect(copyVersion).toHaveBeenCalledExactlyOnceWith(false);
  expect(writeText).not.toHaveBeenCalled();
});

test.each(["macos", "windows"])("settings search on %s uses only keys to find, choose, clear and leave", async (platform) => {
  const previous = document.documentElement.dataset.system;
  document.documentElement.dataset.system = platform;
  onTestFinished(() => { if (previous === undefined) delete document.documentElement.dataset.system; else document.documentElement.dataset.system = previous; });
  render(<SettingsApp api={fakeApi().api} initialPage="general" />);
  const opener = await screen.findByRole("button", { name: /^Open Ibis:/ });
  act(() => opener.focus());
  fireEvent.keyDown(opener, { key: "f", metaKey: platform === "macos", ctrlKey: platform !== "macos" });
  const search = screen.getByRole("searchbox", { name: "Search Settings" });
  expect(document.activeElement).toBe(search);
  fireEvent.change(search, { target: { value: "appearance" } });
  fireEvent.keyDown(search, { key: "Escape" });
  expect(search).toHaveProperty("value", "");
  expect(document.activeElement).toBe(search);
  fireEvent.keyDown(search, { key: "Escape" });
  expect(document.activeElement).toBe(opener);
  fireEvent.keyDown(opener, { key: "f", metaKey: platform === "macos", ctrlKey: platform !== "macos" });
  fireEvent.change(search, { target: { value: "size" } });
  fireEvent.keyDown(search, { key: "ArrowDown" });
  const first = await screen.findByRole("option", { name: "Window size, Appearance" });
  expect(document.activeElement).toBe(first);
  fireEvent.keyDown(first, { key: "ArrowDown" });
  const second = screen.getByRole("option", { name: "Text size, Appearance" });
  expect(document.activeElement).toBe(second);
  fireEvent.keyDown(second, { key: "ArrowUp" });
  expect(document.activeElement).toBe(first);
  fireEvent.keyDown(first, { key: "Enter" });
  fireEvent.keyUp(first, { key: "Enter" });
  await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("radio", { name: "Medium" })));
});

test("setting defaults appear only for changed values, restore with Undo and survive reopening", async () => {
  // Compact rows are the default, so Comfortable is the changed value.
  const { api, state } = fakeApi({}, { appearance: "dark", density: "comfortable" });
  const mounted = render(<SettingsApp api={api} initialPage="look" />);
  const reset = await screen.findByRole("button", { name: "Use Default for Appearance" });
  expect(screen.queryByRole("button", { name: "Use Default for Accent" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Use Default for Text size" })).toBeNull();
  fireEvent.click(reset);
  const undo = await screen.findByRole("button", { name: "Undo Restore Default for Appearance" });
  expect(screen.getByRole("radio", { name: "System" })).toHaveProperty("checked", true);
  expect(screen.queryByRole("button", { name: "Use Default for Appearance" })).toBeNull();
  fireEvent.click(undo);
  await waitFor(() => expect(screen.getByRole("radio", { name: "Dark" })).toHaveProperty("checked", true));
  fireEvent.click(screen.getByRole("button", { name: "Use Default for Rows" }));
  await screen.findByRole("button", { name: "Undo Restore Default for Rows" });
  mounted.unmount();
  render(<SettingsApp api={api} initialPage="look" />);
  expect(await screen.findByRole("radio", { name: "Compact" })).toHaveProperty("checked", true);
  expect(state.preferences.appearance).toBe("dark");
});

test.each(["general", "citations"] as const)("a settings file that does not parse explains the fallback on %s with its one recovery", async (page) => {
  const { api } = fakeApi();
  api.formatSettings = vi.fn(() => Promise.reject({ code: "settings-invalid", message: { id: "app-settings-error-invalid", args: {} }, recovery: { action: "review-settings" }, focus: "settings" }));
  api.openSettingsFile = vi.fn(async () => null);
  render(<SettingsApp api={api} initialPage={page} />);
  const problem = await screen.findByRole("alert");
  expect(problem.textContent).toContain("settings.json isn’t valid, so Ibis is using your last good settings.");
  expect(problem.textContent).not.toContain("Open Settings to review it.");
  expect(screen.queryByText(/could not read your settings/)).toBeNull();
  fireEvent.click(within(problem).getByRole("button", { name: "Open" }));
  expect(api.openSettingsFile).toHaveBeenCalledOnce();
});

function shortcutFailure(code: string) {
  return { code, message: { id: `shortcuts-refusal-${code}`, args: {} },
    recovery: { action: code === "unavailable" ? "open-ibis" : code === "not-saved" ? "retry" : "choose-shortcut" }, focus: "recovery-action" };
}

test.each(["general", "look", "citations", "ai-apps", "about"])("%s: every card holds rows only, so the card alone draws its dividers", async (page) => {
  const { demoApi } = await import("./demo");
  const { container } = render(<SettingsApp api={demoApi()} initialPage={page} />);
  await act(() => new Promise((resolve) => setTimeout(resolve, 50)));
  const groups = [...container.querySelectorAll(".ui-group")];
  expect(groups.length).toBeGreaterThan(0);
  // The live picture heads Appearance's first row; a Details' wrapper holds its rows.
  const loose = groups.flatMap((group) => [...group.children]).filter((child) => !child.matches(".ui-row, .ui-picture-preview")
    && !(child.matches("div[id]") && [...child.children].every((row) => row.matches(".ui-row"))));
  expect(loose.map((child) => child.outerHTML.slice(0, 80))).toEqual([]);
});
