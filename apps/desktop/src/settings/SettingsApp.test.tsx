import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import axe from "axe-core";
import { afterEach, expect, test, vi } from "vitest";
import type { KeyFormat, PickerPreferences, ShortcutStatus, WindowSize } from "../bindings";
import { DEFAULT_PICKER_SHORTCUT, KEY_TABLE, keyLabel, shortcutLabel } from "../keys";
import { keyText } from "../messages";
import { currentPlatform } from "../platform";
import { pageOf, SettingsApp } from "./SettingsApp";
import { RECHECK_MS, STALE_HINT_MS } from "./PasteGuide";
import type { PastePermission, SettingsApi } from "./api";
import recordings from "../testing/shortcut-recordings.json";
import pickerConflicts from "../testing/picker-shortcut-conflicts.json";
import english from "../i18n/en-US.ftl?raw";
import { createInterfaceMessages, englishMessages, InterfaceMessagesProvider } from "../i18n";

afterEach(() => { vi.useRealTimers(); localStorage.clear(); });

test.each(["macos", "windows", "linux"])("Shortcuts preserves every English name and key label on %s", async (platform) => {
  const previous = document.documentElement.dataset.system;
  document.documentElement.dataset.system = platform;
  try {
    const { api } = fakeApi({ granted: true });
    render(<SettingsApp api={api} initialPage="shortcuts" />);
    const label = shortcutLabel(DEFAULT_PICKER_SHORTCUT, platform === "macos");
    expect(await screen.findByRole("button", { name: `Open Ibis: ${label}` })).toHaveProperty("textContent", label);
    for (const binding of KEY_TABLE) {
      const row = document.getElementById(`key-${binding.action}`)!.closest(".ui-settings-row")!;
      expect(row.querySelector(".ui-settings-row-text")?.textContent).toBe(keyText.actions[binding.action]);
      expect(row.closest("section")?.querySelector("h2")?.textContent).toBe(keyText.regions[binding.region]);
      const compact = (text: string | null | undefined) => text?.replace(/[\s+]/g, "");
      expect(row.querySelectorAll(".key-caps")).toHaveLength(binding.keys.length);
      expect(compact(row.querySelector(".ui-settings-keys")?.textContent))
        .toBe(compact(binding.keys.map((keys) => keyLabel(keys, platform === "macos")).join(" or ")));
    }
  } finally {
    if (previous === undefined) delete document.documentElement.dataset.system;
    else document.documentElement.dataset.system = previous;
  }
});

test.each(["general", "shortcuts"])("localized %s records directly, refuses registration, and reopens the saved shortcut", async (page) => {
  const { api, state } = fakeApi();
  const pseudo = createInterfaceMessages(english, true);
  const view = () => <InterfaceMessagesProvider messages={pseudo}><SettingsApp api={api} initialPage={page} /></InterfaceMessagesProvider>;
  const mounted = render(view());
  const recorder = await screen.findByRole("button", { name: `${pseudo.text("shortcuts-open")}: ${shortcutLabel(DEFAULT_PICKER_SHORTCUT, currentPlatform() === "macos")}` });
  const keys = [...document.querySelectorAll(".key-caps")].map((key) => key.textContent?.replaceAll("+", ""));
  if (page === "shortcuts") expect(keys).toContain(currentPlatform() === "macos" ? "⌘K" : "CtrlK");
  for (const row of document.querySelectorAll("#picker-shortcuts .ui-settings-row-text")) expect(row.textContent).toContain("⟦");
  vi.spyOn(api, "setShortcut").mockRejectedValueOnce("in-use");
  fireEvent.click(recorder);
  expect(recorder.textContent).toBe(pseudo.text("shortcuts-record"));
  expect(recorder.getAttribute("aria-label")).toBe(`${pseudo.text("shortcuts-open")}: ${recorder.textContent}`);
  fireEvent.keyDown(recorder, { key: "k", code: "KeyK", altKey: true });
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", pseudo.text("shortcuts-refusal-in-use"));
  expect(recorder.textContent).toBe(shortcutLabel(DEFAULT_PICKER_SHORTCUT, currentPlatform() === "macos"));
  fireEvent.click(recorder);
  fireEvent.keyDown(recorder, { key: "j", code: "KeyJ", altKey: true });
  await waitFor(() => expect(recorder.textContent).toBe(shortcutLabel("Alt+J", currentPlatform() === "macos")));
  await act(async () => state.closes[0]!());
  mounted.unmount();
  render(view());
  expect(await screen.findByRole("button", { name: `${pseudo.text("shortcuts-open")}: ${shortcutLabel("Alt+J", currentPlatform() === "macos")}` })).toBeTruthy();
  expect(screen.getByRole("button", { name: pseudo.text("shortcuts-use-default") })).toBeTruthy();
  expect(state.shortcutChoices).toEqual(["Alt+J"]);
});

test.each(["success", "failure"])("localized About keeps newer updates after an older read's %s and reopening", async (outcome) => {
  const { api, state } = fakeApi();
  const old = deferred<boolean>(), latest = deferred<boolean>();
  vi.spyOn(api, "updateChecks").mockReturnValueOnce(old.promise).mockReturnValueOnce(latest.promise);
  vi.spyOn(api, "setUpdateChecks").mockImplementation(async (on) => { for (const handler of state.automatic) handler(on); return undefined as never; });
  const pseudo = createInterfaceMessages(english, true);
  const view = () => <InterfaceMessagesProvider messages={pseudo}><SettingsApp api={api} initialPage="about" /></InterfaceMessagesProvider>;
  const mounted = render(view());
  fireEvent.focus(window);
  await act(async () => latest.resolve(true));
  const automatic = screen.getByRole("switch", { name: pseudo.text("about-automatic") });
  await act(async () => { if (outcome === "success") old.resolve(false); else old.reject(); });
  expect(automatic).toHaveProperty("checked", true);
  expect(screen.getByText(pseudo.text("about-version", { name: "Ibis Nightly", version: "0.3.0" }))).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: pseudo.text("about-check") }));
  fireEvent.click(automatic);
  expect(api.setUpdateChecks).toHaveBeenCalledWith(false);
  await act(async () => state.closes[0]!());
  mounted.unmount();
  vi.spyOn(api, "updateChecks").mockResolvedValueOnce(false);
  render(view());
  expect(await screen.findByRole("switch", { name: pseudo.text("about-automatic") })).toHaveProperty("checked", false);
});

test("localized panel errors and native values keep their separate owners and stay literal", async () => {
  const { api, state } = fakeApi();
  const hostile = '<img src=x onerror="throw 1"> { $secret } & <b>native</b>';
  state.shortcut = { shortcut: "", registered: false, error: hostile, desktop: { configurable: true, guidance: hostile } };
  vi.spyOn(api, "configureShortcuts").mockRejectedValueOnce(new Error(hostile));
  vi.spyOn(api, "loginItem").mockResolvedValue({ name: hostile, enabled: false, problem: null });
  vi.spyOn(api, "version").mockResolvedValue(hostile);
  vi.spyOn(api, "updateChecks").mockRejectedValueOnce(new Error(hostile));
  const pseudo = createInterfaceMessages(english, true);
  render(<InterfaceMessagesProvider messages={pseudo}><SettingsApp api={api} initialPage="shortcuts" /></InterfaceMessagesProvider>);
  expect(await screen.findByText(pseudo.text("shortcuts-not-assigned"))).toBeTruthy();
  expect(screen.getAllByText(hostile)).toHaveLength(2);
  fireEvent.click(screen.getByRole("button", { name: pseudo.text("shortcuts-change") }));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", pseudo.text("shortcuts-configure-failed"));
  fireEvent.click(screen.getByRole("option", { name: pseudo.text("settings-about") }));
  expect(screen.getByText(pseudo.text("about-version", { name: hostile, version: hostile }))).toBeTruthy();
  expect(screen.getByText(pseudo.text("settings-read-failed"))).toBeTruthy();
  expect(screen.queryByText(pseudo.text("about-unavailable"))).toBeNull();
  expect(document.querySelector("img, b")).toBeNull();
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: () => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function fakeApi(permission: Partial<PastePermission> = {}, preferences: Partial<PickerPreferences> = {}) {
  const state = {
    permission: { pastes: true, needed: true, granted: false, list: "Device Control and Data Access", ...permission },
    preferences: { citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true, search_icon: "magnifier", paste: true, menu_bar: true, ...preferences } as PickerPreferences,
    saved: [] as PickerPreferences[],
    patches: [] as Partial<PickerPreferences>[],
    opened: 0,
    reads: 0,
    revision: 0,
    finished: 0,
    windowSaved: undefined as ((saved: boolean) => void) | undefined,
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
  const commit = async (next: Partial<PickerPreferences>, key: KeyFormat | null = null) => {
    state.patches.push(next);
    state.preferences = { ...state.preferences, ...next, ...(key ? { copy_format: key } : {}) };
    state.saved.push({ ...state.preferences });
    return { preferences: state.preferences, copy_format: state.preferences.copy_format ?? "pandoc" };
  };
  const api: SettingsApi = {
    pickerPreferences: async () => state.preferences,
    windowSize: vi.fn(async (): Promise<WindowSize | null> => "medium"),
    setWindowSize: vi.fn(async (): Promise<null> => null),
    onWindowSaved: vi.fn(async (handler: (saved: boolean) => void) => { state.windowSaved = handler; return () => undefined; }),
    preferenceRevision: async () => ++state.revision,
    setPickerPreferences: commit,
    forgetAppOutputs: async () => null,
    loginItem: async () => ({ name: "Ibis Nightly", enabled: state.login, problem: null }),
    setLoginItem: async (enabled) => { state.login = enabled; return { name: "Ibis Nightly", enabled, problem: null }; },
    updateChecks: async () => state.updates,
    setUpdateChecks: async (on) => { state.updates = on; for (const changed of state.automatic) changed(on); return undefined as never; },
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
    onPage: async (handler) => { state.pages.push(handler); return () => undefined; },
    onUpdateChecks: async (handler) => { state.automatic.push(handler); return () => { state.automatic = state.automatic.filter((each) => each !== handler); }; },
    onCloseRequested: async (handler) => { state.closes.push(handler); return () => undefined; },
  };
  return { api, state, commit };
}

test("localized General keeps pending choices, refuses failed saves, and reopens saved state", async () => {
  const { api, state, commit } = fakeApi({ granted: true });
  const pseudo = createInterfaceMessages(english, true);
  const label = pseudo.text("look-larger-text");
  const save = deferred<{ preferences: PickerPreferences; copy_format: "pandoc" }>();
  vi.spyOn(api, "setPickerPreferences").mockReturnValueOnce(save.promise).mockImplementation(commit);
  const view = () => <InterfaceMessagesProvider messages={pseudo}><SettingsApp api={api} initialPage="look" /></InterfaceMessagesProvider>;
  const mounted = render(view());
  const choice = await screen.findByRole("radio", { name: label });
  fireEvent.click(choice);
  fireEvent.focus(window);
  await act(async () => undefined);
  expect((choice as HTMLInputElement).checked).toBe(true);
  await act(async () => save.reject());
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", pseudo.text("settings-choice-failed"));
  await waitFor(() => expect((choice as HTMLInputElement).checked).toBe(false));
  fireEvent.click(choice);
  await act(async () => state.closes[0]!());
  mounted.unmount();
  render(view());
  expect(await screen.findByRole("radio", { name: label })).toHaveProperty("checked", true);
  expect(state.patches).toEqual([{ larger_text: true }]);
});

test("localized General rejects delayed A after B without changing settings or locale", async () => {
  const { api, state } = fakeApi({ granted: true });
  const old = deferred<PickerPreferences>(), latest = deferred<PickerPreferences>();
  vi.spyOn(api, "pickerPreferences").mockReturnValueOnce(old.promise).mockReturnValueOnce(latest.promise);
  const pseudo = createInterfaceMessages(english, true);
  render(<InterfaceMessagesProvider messages={pseudo}><SettingsApp api={api} initialPage="look" /></InterfaceMessagesProvider>);
  fireEvent.focus(window);
  await act(async () => latest.resolve({ ...state.preferences, larger_text: true }));
  const choice = screen.getByRole("radio", { name: pseudo.text("look-larger-text") });
  expect(choice).toHaveProperty("checked", true);
  await act(async () => old.resolve(state.preferences));
  expect(choice).toHaveProperty("checked", true);
  expect(screen.getByRole("heading", { level: 1 })).toHaveProperty("textContent", pseudo.text("settings-look"));
  expect(state.patches).toEqual([]);
});

test("Show in Menu Bar is a Mac choice that saves as a patch and goes back on failure", async () => {
  document.documentElement.dataset.system = "macos";
  const { api, state, commit } = fakeApi({ granted: true });
  const save = vi.spyOn(api, "setPickerPreferences").mockRejectedValueOnce(new Error("refused")).mockImplementation(commit);
  const mounted = render(<SettingsApp api={api} initialPage="general" />);
  const choice = await screen.findByRole("switch", { name: "Show in Menu Bar" });
  await waitFor(() => expect((choice as HTMLInputElement).disabled).toBe(false));
  expect((choice as HTMLInputElement).checked).toBe(true);
  fireEvent.click(choice);
  await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
  await waitFor(() => expect((choice as HTMLInputElement).checked).toBe(true));
  expect((await screen.findByRole("alert")).closest(".ui-settings-row")?.textContent).toContain("Show in Menu Bar");
  fireEvent.click(choice);
  await waitFor(() => expect(state.patches).toEqual([{ menu_bar: false }]));
  expect((choice as HTMLInputElement).checked).toBe(false);
  mounted.unmount();
  document.documentElement.dataset.system = "linux";
  try {
    render(<SettingsApp api={api} initialPage="general" />);
    await screen.findByText("Start on Login");
    expect(screen.queryByRole("switch", { name: "Show in Menu Bar" })).toBeNull();
  } finally { delete document.documentElement.dataset.system; }
});

test.each([
  ["pastes", { pastes: true, granted: true }, { paste: true }, "Paste the citation"],
  ["still needs the paste permission", { pastes: true, needed: true, granted: false }, { paste: true }, "Copy the citation"],
  ["paste switched off", { pastes: true, granted: true }, { paste: false }, "Copy the citation"],
  ["a system that copies", { pastes: false, needed: false }, { paste: true }, "Copy the citation"],
])("Shortcuts names what Enter does where Ibis %s", async (_case, permission, preferences, words) => {
  const { api } = fakeApi(permission, preferences);
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  expect(await screen.findByText(words)).toBeTruthy();
  expect(screen.queryByText(words === "Copy the citation" ? "Paste the citation" : "Copy the citation")).toBeNull();
});

test("Recent citations is on by default, saves off as a patch and goes back on failure", async () => {
  const { api, state, commit } = fakeApi({ granted: true });
  const save = vi.spyOn(api, "setPickerPreferences").mockRejectedValueOnce(new Error("refused")).mockImplementation(commit);
  render(<SettingsApp api={api} initialPage="citations" />);
  const choice = await screen.findByRole<HTMLInputElement>("switch", { name: "Recent citations" });
  expect(choice.checked).toBe(true);
  fireEvent.click(choice);
  await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
  await waitFor(() => expect(choice.checked).toBe(true));
  expect((await screen.findByRole("alert")).closest(".ui-settings-row")?.textContent).toContain("Recent citations");
  fireEvent.click(choice);
  await waitFor(() => expect(state.patches).toEqual([{ recent_citations: false }]));
  expect(choice.checked).toBe(false);
});

test("Window size shows a dragged size as Custom, applies once, and goes back on failure", async () => {
  const { api } = fakeApi({ granted: true });
  vi.mocked(api.windowSize).mockResolvedValue("custom");
  const choose = vi.mocked(api.setWindowSize).mockRejectedValueOnce(new Error("refused"));
  render(<SettingsApp api={api} initialPage="look" />);
  await waitFor(() => expect(screen.getByRole("radio", { name: "Custom" })).toHaveProperty("checked", true));
  fireEvent.click(screen.getByRole("radio", { name: "Large" }));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", englishMessages.text("settings-choice-failed"));
  await waitFor(() => expect(screen.getByRole("radio", { name: "Custom" })).toHaveProperty("checked", true));
  fireEvent.click(screen.getByRole("radio", { name: "Small" }));
  await waitFor(() => expect(screen.getByRole("radio", { name: "Small" })).toHaveProperty("checked", true));
  expect(screen.queryByRole("radio", { name: "Custom" })).toBeNull();
  expect(choose.mock.calls).toEqual([["large"], ["small"]]);
  expect(screen.queryByRole("alert")).toBeNull();
});

test("a window size read that returns late cannot undo a newer choice", async () => {
  const { api } = fakeApi({ granted: true });
  const late = deferred<WindowSize | null>();
  render(<SettingsApp api={api} initialPage="look" />);
  const large = await screen.findByRole("radio", { name: "Large" });
  await waitFor(() => expect(large).toHaveProperty("disabled", false));
  vi.mocked(api.windowSize).mockReturnValueOnce(late.promise);
  fireEvent.blur(window); fireEvent.focus(window);
  await waitFor(() => expect(api.windowSize).toHaveBeenCalledTimes(2));
  fireEvent.click(large);
  await act(async () => late.resolve("small"));
  expect(large).toHaveProperty("checked", true);
});

test("a size the disk refused says so and keeps showing what the window has", async () => {
  const { api, state } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="look" />);
  const large = await screen.findByRole("radio", { name: "Large" });
  await waitFor(() => expect(large).toHaveProperty("disabled", false));
  fireEvent.click(large);
  await waitFor(() => expect(state.windowSaved).toBeDefined());
  act(() => state.windowSaved!(false));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", englishMessages.text("settings-choice-failed"));
  expect(large).toHaveProperty("checked", true);
});

test("when two quick choices fail, the size the app still has comes back", async () => {
  const { api } = fakeApi({ granted: true });
  vi.mocked(api.setWindowSize).mockRejectedValue(new Error("refused"));
  render(<SettingsApp api={api} initialPage="look" />);
  const large = await screen.findByRole("radio", { name: "Large" });
  await waitFor(() => expect(large).toHaveProperty("disabled", false));
  fireEvent.click(large);
  fireEvent.click(screen.getByRole("radio", { name: "Small" }));
  await waitFor(() => expect(screen.getByRole("radio", { name: "Medium" })).toHaveProperty("checked", true));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", englishMessages.text("settings-choice-failed"));
});

test("a window size that cannot be read says so and offers no choice", async () => {
  const { api } = fakeApi({ granted: true });
  vi.mocked(api.windowSize).mockResolvedValue(null);
  render(<SettingsApp api={api} initialPage="look" />);
  expect(await screen.findByRole("radio", { name: "Large" })).toHaveProperty("disabled", true);
  await waitFor(() => expect(screen.getByText(englishMessages.text("settings-read-failed"))).toBeTruthy());
});

test("pages are opened by name, and the paste guide is General's pasting group", () => {
  expect(pageOf("paste")).toEqual({ page: "general", focus: "paste" });
  expect(pageOf("about")).toEqual({ page: "about", focus: null });
  expect(pageOf("shortcuts")).toEqual({ page: "shortcuts", focus: null });
  expect(pageOf("unknown")).toEqual({ page: "general", focus: null });
  expect(pageOf(null)).toEqual({ page: "general", focus: null });
});

test.each([1, 2])("Wayland portal v%s shows the desktop trigger, never a recorder or reset", async (version) => {
  const { api, state } = fakeApi();
  state.shortcut = { shortcut: "Strg + Umschalt + Y", registered: true, error: null,
    desktop: { configurable: version >= 2, guidance: version >= 2 ? "Your desktop manages this shortcut." : "Change this shortcut in System Settings > Keyboard > Shortcuts > Ibis." } };
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  expect(await screen.findByText("Strg + Umschalt + Y")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Open Ibis" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Use Default" })).toBeNull();
  if (version >= 2) {
    fireEvent.click(screen.getByRole("button", { name: "Change…" }));
    await waitFor(() => expect(state.configured).toBe(1));
  } else {
    expect(screen.queryByRole("button", { name: "Change…" })).toBeNull();
    expect(screen.getByText(/System Settings > Keyboard > Shortcuts > Ibis/)).toBeTruthy();
  }
  await act(async () => {
    state.shortcut = { ...state.shortcut, shortcut: "Alt + F9" };
    for (const handler of state.shortcuts) handler(state.shortcut);
  });
  expect(screen.getByText("Alt + F9")).toBeTruthy();
  expect(state.shortcutChoices).toEqual([]);
  expect(state.saved).toEqual([]);
  const result = await axe.run(document.body);
  expect(result.violations).toEqual([]);
});

test.each(["Strg + Umschalt + Y", "Alt + F9", ""])("General preserves the desktop's trigger and guidance (%j)", async (trigger) => {
  const { api, state } = fakeApi();
  const guidance = "Your desktop manages this shortcut. Bind ibis toggle in Keyboard Settings.";
  state.shortcut = { shortcut: trigger, registered: !!trigger, error: null, desktop: { configurable: true, guidance } };
  render(<SettingsApp api={api} initialPage="general" />);
  const row = document.getElementById("general-open-shortcut")!.closest(".ui-settings-row")!;
  expect(await within(row as HTMLElement).findByText(trigger || "Not assigned")).toBeTruthy();
  expect(within(row as HTMLElement).getByText(guidance)).toBeTruthy();
  expect(within(row as HTMLElement).queryByText("Click the shortcut, then type a new one. Esc cancels.")).toBeNull();
  fireEvent.click(within(row as HTMLElement).getByRole("button", { name: "Change…" }));
  expect(await screen.findByRole("heading", { level: 1, name: "General" })).toBeTruthy();
  await waitFor(() => expect(state.configured).toBe(1));
  expect(screen.queryByRole("button", { name: /^Open Ibis:/ })).toBeNull();
  expect(state.shortcutChoices).toEqual([]);
});

test("portal configuration prevents repeated requests, reports failure and allows retry", async () => {
  const { api, state } = fakeApi();
  state.shortcut = { shortcut: "Ctrl+Shift+Y", registered: true, error: null, desktop: { configurable: true, guidance: "Desktop settings" } };
  const pending = deferred<null>();
  const configure = vi.spyOn(api, "configureShortcuts").mockReturnValueOnce(pending.promise);
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  const change = await screen.findByRole("button", { name: "Change…" });
  fireEvent.click(change);
  fireEvent.click(change);
  expect(configure).toHaveBeenCalledTimes(1);
  expect(change.hasAttribute("disabled")).toBe(true);
  await act(async () => pending.reject());
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "The desktop could not open its shortcut settings. Try again.");
  fireEvent.click(change);
  await waitFor(() => expect(state.configured).toBe(1));
  expect(state.shortcutChoices).toEqual([]);
});

test.each([false, true])("a successful session without a trigger shows Hyprland bindings and keeps configuration capability (%s)", async (configurable) => {
  const { api, state } = fakeApi();
  const guidance = 'Hyprland: add `hl.bind("CTRL + SHIFT + Y", hl.dsp.exec_cmd("ibis toggle"))`. For hyprland.conf: `bind = CTRL SHIFT, Y, exec, ibis toggle`.';
  state.shortcut = { shortcut: "", registered: false, error: "Your desktop did not report a shortcut.", desktop: { configurable, guidance } };
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  expect(await screen.findByText(guidance)).toBeTruthy();
  expect(screen.getByText("Not assigned")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Open Ibis" })).toBeNull();
  if (configurable) {
    fireEvent.click(screen.getByRole("button", { name: "Change…" }));
    await waitFor(() => expect(state.configured).toBe(1));
  } else expect(screen.queryByRole("button", { name: "Change…" })).toBeNull();
  expect(state.shortcutChoices).toEqual([]);
});

test("portal loss removes the stale trigger and Change, showing the exact fallback binding", async () => {
  const { api, state } = fakeApi();
  state.shortcut = { shortcut: "Ctrl+Shift+Y", registered: true, error: null, desktop: { configurable: true, guidance: "Desktop settings" } };
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  await screen.findByRole("button", { name: "Change…" });
  const guidance = "Sway: add `bindsym --no-repeat Ctrl+Shift+y exec ibis toggle` to ~/.config/sway/config, then run `swaymsg reload`.";
  await act(async () => {
    state.shortcut = { shortcut: "", registered: false, error: "The desktop shortcut is unavailable.", desktop: { configurable: false, guidance } };
    for (const handler of state.shortcuts) handler(state.shortcut);
  });
  expect(screen.getByText("Not assigned")).toBeTruthy();
  expect(screen.getByText(guidance)).toBeTruthy();
  expect(screen.queryByText("Ctrl+Shift+Y")).toBeNull();
  expect(screen.queryByRole("button", { name: "Change…" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Open Ibis" })).toBeNull();
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
    await screen.findByRole("switch", { name: "Check for updates automatically" });
    vi.spyOn(api, "updateChecks").mockReturnValueOnce(read.promise);
    fireEvent.focus(window);
  }
  act(() => state.automatic[0]!(true));
  const automatic = screen.getByRole("switch", { name: "Check for updates automatically" }) as HTMLInputElement;
  expect(automatic.checked).toBe(true);
  await act(async () => { if (outcome === "success") read.resolve(false); else read.reject(); });
  expect(screen.getByRole("switch", { name: "Check for updates automatically" })).toBe(automatic);
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
  const automatic = screen.getByRole("switch", { name: "Check for updates automatically" }) as HTMLInputElement;
  await act(async () => { if (outcome === "success") old.resolve(false); else old.reject(); });
  expect(screen.getByRole("switch", { name: "Check for updates automatically" })).toBe(automatic);
  expect(automatic.checked).toBe(true);
});

test("Settings opens at the top of General with every built page in a navigable, selected sidebar list", async () => {
  const { api, state } = fakeApi();
  render(<SettingsApp api={api} initialPage={null} />);
  await screen.findByRole("heading", { name: "Allow Ibis to paste for you" });
  const sidebar = screen.getByRole("navigation", { name: "Settings pages" });
  const list = within(sidebar).getByRole("listbox", { name: "Settings pages" });
  const items = within(list).getAllByRole("option");
  expect(items.map((item) => item.textContent)).toEqual(["General", "Look", "Citations", "Shortcuts", "About Ibis"]);
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

test("sidebar arrows and Home/End move focus, select the page and reset its scroll", async () => {
  const { api } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="general" />);
  await screen.findByRole("button", { name: /^Open Ibis:/ });
  const general = screen.getByRole("option", { name: "General" });
  const shortcuts = screen.getByRole("option", { name: "Look" });
  const about = screen.getByRole("option", { name: "About Ibis" });
  act(() => general.focus());
  for (const [key, target, name] of [
    ["ArrowDown", shortcuts, "Look"], ["End", about, "About Ibis"],
    ["Home", general, "General"], ["ArrowDown", shortcuts, "Look"],
    ["End", about, "About Ibis"], ["Home", general, "General"],
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
  await screen.findByRole("button", { name: /^Open Ibis:/ });
  for (const name of ["General", "Look", "Citations", "Shortcuts", "About Ibis"]) {
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
  await screen.findByRole("button", { name: /^Open Ibis:/ });
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
    await screen.findByRole("button", { name: /^Open Ibis:/ });
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
  expect(within(card).getByRole("status").textContent).toBe("Ibis can paste now.");
  expect(within(card).queryByRole("button", { name: "Open System Settings" })).toBeNull();
  expect(card.textContent).not.toContain("remove Ibis Nightly from the list");
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
  render(<SettingsApp api={api} initialPage="look" />);
  const larger = await screen.findByRole("radio", { name: "Larger" });
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
  render(<SettingsApp api={api} initialPage="look" />);
  await screen.findByRole("radio", { name: "Larger" });
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
  render(<SettingsApp api={api} initialPage="look" />);
  fireEvent.click(await screen.findByRole("radio", { name: "Spacious" }));
  fireEvent.click(screen.getByRole("radio", { name: "Larger" }));
  await waitFor(() => expect(state.saved).toHaveLength(2));
  expect(state.saved[0]).toMatchObject({ density: "spacious", larger_text: false, paste: true });
  expect(state.saved[1]).toMatchObject({ density: "spacious", larger_text: true, paste: true });
});

test.each(["success", "failure", "later edit"])("closing drains both queued choices (%s)", async (outcome) => {
  const { api, state, commit } = fakeApi({ granted: true });
  const first = deferred<null>(), second = deferred<null>();
  const save = vi.spyOn(api, "setPickerPreferences").mockImplementationOnce(async (next) => { await first.promise; return commit(next); }).mockImplementationOnce(async (next) => { await second.promise; return commit(next); });
  const read = vi.spyOn(api, "pickerPreferences");
  render(<SettingsApp api={api} initialPage="look" />);
  fireEvent.click(await screen.findByRole("radio", { name: "Spacious" }));
  if (outcome !== "later edit") fireEvent.click(screen.getByRole("radio", { name: "Larger" }));
  await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
  const closed = vi.fn();
  const closing = state.closes[0]!().then(closed);
  if (outcome === "later edit") fireEvent.click(screen.getByRole("radio", { name: "Larger" }));
  await act(async () => { await Promise.resolve(); });
  expect(closed).not.toHaveBeenCalled();
  await act(async () => { if (outcome === "failure") first.reject(); else first.resolve(null); });
  expect(save).toHaveBeenNthCalledWith(2, outcome === "failure" ? { density: "spacious" } : { larger_text: true }, null, outcome === "failure" ? 1 : 2);
  expect(read).toHaveBeenCalledOnce();
  expect(closed).not.toHaveBeenCalled();
  await act(async () => { second.resolve(null); await closing; });
  expect(save).toHaveBeenLastCalledWith({ larger_text: true }, null, 2);
  expect(closed).toHaveBeenCalledOnce();
});

test("a stuck save holds closing for at most two seconds", async () => {
  const { api, state, commit } = fakeApi({ granted: true });
  const save = deferred<null>();
  vi.spyOn(api, "setPickerPreferences").mockImplementation(async (next) => { await save.promise; return commit(next); });
  render(<SettingsApp api={api} initialPage="look" />);
  const larger = await screen.findByRole("radio", { name: "Larger" });
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
  render(<SettingsApp api={api} initialPage="look" />);
  const larger = await screen.findByRole("radio", { name: "Larger" });
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
  render(<SettingsApp api={api} initialPage="look" />);
  const larger = await screen.findByRole("radio", { name: "Larger" });
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
  await screen.findByRole("button", { name: /^Open Ibis:/ });
  fireEvent.click(screen.getByRole("option", { name: "About Ibis" }));
  expect(await screen.findByText("Ibis Nightly 0.3.0")).toBeTruthy();
  expect(screen.getByRole("option", { name: "About Ibis" }).getAttribute("aria-current")).toBe("page");
  act(() => state.pages[0]!("paste"));
  expect(await screen.findByRole("switch", { name: "Paste into the app you came from" })).toBeTruthy();
  const found = await axe.run(container);
  expect(found.violations.map((v) => v.id)).toEqual([]);
});

test("a Settings field patch keeps an output chosen in the other window", async () => {
  const { api, state } = fakeApi({ granted: true }, { citation_style: "apa" });
  render(<SettingsApp api={api} initialPage="look" />);
  const larger = await screen.findByRole("radio", { name: "Larger" });
  state.preferences = { ...state.preferences, citation_style: "ieee" };
  fireEvent.click(larger);
  await waitFor(() => expect(state.patches).toEqual([{ larger_text: true }]));
  expect(state.preferences).toMatchObject({ citation_style: "ieee", larger_text: true });
});

test("the latest focus read wins even when no local edit occurred", async () => {
  const { api, state } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="look" />);
  const larger = await screen.findByRole("radio", { name: "Larger" });
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
  render(<SettingsApp api={api} initialPage="look" />);
  const larger = await screen.findByRole("radio", { name: "Larger" });
  fireEvent.click(larger);
  fireEvent.click(screen.getByRole("radio", { name: "Default" }));
  fireEvent.click(larger);
  // Retrying the third intent keeps its ticket separate from the newer row choice.
  fireEvent.click(screen.getByRole("radio", { name: "Spacious" }));
  await act(async () => { first.resolve(); });
  await waitFor(() => expect(writes).toBe(5));
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
  expect(document.querySelector("details, summary")).toBeNull();
  const lists = [...document.querySelectorAll<HTMLElement>('section[id^="shortcuts-"]')];
  for (const binding of KEY_TABLE) expect(screen.getByText(keyText.actions[binding.action])).toBeTruthy();
  expect(lists.flatMap((list) => within(list).queryAllByRole("button"))).toEqual([]);
  expect(lists.flatMap((list) => [...list.querySelectorAll(".ui-settings-keys")])).toHaveLength(KEY_TABLE.length);
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

test.each([
  ["invalid", "Ibis cannot use that key combination. Try another shortcut."],
  ["needs-modifier", "Include Control, Alt or Command. Shift alone is not enough."],
  ["app-shortcut", "That shortcut is used for app commands. Add another modifier."],
  ["alt-gr", "Ctrl+Alt with a typing key is used for AltGr on Windows. Try another shortcut."],
  ["in-use", "Another app or the system may be using that shortcut. Try another."],
  ["unavailable", "Global shortcuts are unavailable on this desktop. Open Ibis from your application launcher."],
  ["not-saved", "The shortcut works now, but Ibis could not save it for next time. Try again."],
].flatMap(([code, message]) => ["general", "shortcuts"].map((page) => ({ page, code, message }))))("$page: the $code refusal explains the result and keeps the working shortcut", async ({ page, code, message }) => {
  const { api, state } = fakeApi({ granted: true });
  vi.spyOn(api, "setShortcut").mockImplementationOnce(async () => {
    if (code === "not-saved") {
      state.shortcut = { shortcut: "Alt+K", registered: true, error: null };
      for (const handler of state.shortcuts) handler(state.shortcut);
    }
    throw code;
  });
  render(<SettingsApp api={api} initialPage={page} />);
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

test.each(["general", "shortcuts"])("%s shortcut saves disable the recorder and default, and finish before Settings closes", async (page) => {
  const { api, state } = fakeApi({ granted: true });
  state.shortcut = { shortcut: "Alt+K", registered: true, error: null };
  const save = deferred<ShortcutStatus>();
  vi.spyOn(api, "setShortcut").mockReturnValueOnce(save.promise);
  render(<SettingsApp api={api} initialPage={page} />);
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

test.each(["general", "shortcuts"])("%s explains a failed shortcut read", async (page) => {
  const { api } = fakeApi({ granted: true });
  vi.spyOn(api, "shortcutStatus").mockRejectedValueOnce(new Error("unavailable"));
  render(<SettingsApp api={api} initialPage={page} />);
  expect((await screen.findByRole("alert")).textContent).toBe("Ibis could not check the shortcut. Try reopening Settings.");
});

test.each(["general", "shortcuts"])("%s has no axe violations while idle, recording, or showing a refusal", async (page) => {
  const { api } = fakeApi({ granted: true });
  vi.spyOn(api, "setShortcut").mockRejectedValueOnce("app-shortcut");
  const { container } = render(<SettingsApp api={api} initialPage={page} />);
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  expect((await axe.run(container)).violations.map((v) => v.id)).toEqual([]);
  fireEvent.click(recorder);
  expect((await axe.run(container)).violations.map((v) => v.id)).toEqual([]);
  fireEvent.keyDown(recorder, { key: "r", code: "KeyR", metaKey: true });
  await screen.findByRole("alert");
  expect((await axe.run(container)).violations.map((v) => v.id)).toEqual([]);
});

test.each([false, true])("a focus read started before recording cannot restore the old shortcut (pseudo %s)", async (pseudolocale) => {
  const { api, state } = fakeApi({ granted: true });
  const messages = createInterfaceMessages(english, pseudolocale);
  render(<InterfaceMessagesProvider messages={messages}><SettingsApp api={api} initialPage="shortcuts" /></InterfaceMessagesProvider>);
  const recorder = await screen.findByRole("button", { name: `${messages.text("shortcuts-open")}: ${shortcutLabel(DEFAULT_PICKER_SHORTCUT, currentPlatform() === "macos")}` });
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

test.each(["toString", '<b>{ $secret }</b>', { "picker-shortcut": "constructor" }, null])("unknown shortcut refusals keep the exact generic message (%j)", async (refusal) => {
  const { api } = fakeApi();
  vi.spyOn(api, "setShortcut").mockRejectedValueOnce(refusal);
  render(<SettingsApp api={api} initialPage="shortcuts" />);
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  const original = recorder.textContent;
  fireEvent.click(recorder);
  fireEvent.keyDown(recorder, { key: "k", code: "KeyK", altKey: true });
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Ibis could not change the shortcut. Try again.");
  expect(recorder.textContent).toBe(original);
  expect(document.querySelector("b")).toBeNull();
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


test("seven pages and search find common words, reveal and briefly tint their cause", async () => {
  const { api } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="general" />);
  expect(screen.getAllByRole("option").map((x) => x.textContent)).toEqual(["General", "Look", "Citations", "Shortcuts", "About Ibis"]);
  const search = screen.getByRole("searchbox", { name: "Search Settings" });
  fireEvent.change(search, { target: { value: "dark" } });
  fireEvent.click(await screen.findByRole("button", { name: "Appearance" }));
  expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Look");
  expect(document.getElementById("appearance")!.closest(".ui-settings-row")!.hasAttribute("data-highlighted")).toBe(true);
  await new Promise((resolve) => setTimeout(resolve, 2100));
  expect(document.getElementById("appearance")!.closest(".ui-settings-row")!.hasAttribute("data-highlighted")).toBe(false);
});

test("Settings shows only built pages, rows and choices, including in search", async () => {
  const { api, state } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="look" />);
  await screen.findByRole("radiogroup", { name: "Appearance" });
  expect(screen.queryByRole("radio", { name: /Coming soon|Largest/ })).toBeNull();
  for (const name of ["General", "Look", "Citations", "Shortcuts", "About Ibis"]) {
    fireEvent.click(screen.getByRole("option", { name }));
    expect(screen.queryByText(/Coming soon/)).toBeNull();
    expect(screen.queryByText("Show the welcome again")).toBeNull();
    expect(document.querySelector("details, summary")).toBeNull();
  }
  expect(screen.queryByRole("option", { name: /Libraries|Advanced/ })).toBeNull();
  for (const query of ["welcome", "Install the ibis command", "Coming soon"]) {
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: query } });
    expect(await screen.findByText("No matching settings")).toBeTruthy();
  }
  expect(state.patches).toEqual([]);
});

test("Look serializes older A before B, so B stays selected and survives reopening", async () => {
  const { api, state, commit } = fakeApi({ granted: true });
  const slow = deferred<void>();
  const write = vi.spyOn(api, "setPickerPreferences").mockImplementationOnce(async (next) => { await slow.promise; return commit(next); });
  const mounted = render(<SettingsApp api={api} initialPage="look" />);
  fireEvent.click(within(await screen.findByRole("radiogroup", { name: "Rows" })).getByRole("radio", { name: "Compact" }));
  fireEvent.click(screen.getByRole("radio", { name: "Spacious" }));
  await act(async () => undefined);
  expect(write).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("radio", { name: "Spacious" })).toHaveProperty("checked", true);
  await act(async () => slow.resolve());
  await act(async () => state.closes[0]!());
  expect(state.preferences.density).toBe("spacious");
  mounted.unmount();
  render(<SettingsApp api={api} initialPage={null} />);
  expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("General");
  fireEvent.click(screen.getByRole("option", { name: "Look" }));
  expect((await screen.findByRole("radio", { name: "Spacious" }))).toHaveProperty("checked", true);
});

test.each(["read-only", "failed I/O"])("%s restores the saved choice even if recovery also fails, beside its cause", async () => {
  const { api } = fakeApi({ granted: true });
  const mounted = render(<SettingsApp api={api} initialPage="look" />);
  const choice = await screen.findByRole("radio", { name: "Spacious" });
  vi.spyOn(api, "setPickerPreferences").mockRejectedValueOnce(new Error("synthetic private OS detail"));
  vi.spyOn(api, "pickerPreferences").mockRejectedValueOnce(new Error("synthetic read failure"));
  fireEvent.click(choice);
  const problem = await screen.findByRole("alert");
  await waitFor(() => expect(screen.getByRole("radio", { name: "Comfortable" })).toHaveProperty("checked", true));
  expect(problem.closest(".ui-settings-row")?.querySelector("#rows")).toBeTruthy();
  expect(problem.textContent).not.toContain("synthetic");
  fireEvent.click(screen.getByRole("option", { name: "General" }));
  expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("General");
  mounted.unmount();
});


test("a picker preference event updates open Settings without focus, and Settings sends only its chosen field", async () => {
  const { api, state } = fakeApi({ granted: true });
  const { unmount } = render(<SettingsApp api={api} initialPage="look" />);
  await screen.findByRole("radio", { name: "Spacious" });
  state.preferences = { ...state.preferences, density: "compact", show_paste_preview: false };
  await act(async () => { for (const changed of state.preferenceEvents) changed(++state.revision); });
  expect(screen.getByRole("radio", { name: "Compact" })).toHaveProperty("checked", true);
  fireEvent.click(screen.getByRole("radio", { name: "Spacious" }));
  await act(async () => state.closes[0]!());
  expect(state.preferences).toMatchObject({ density: "spacious", show_paste_preview: false });
  expect(state.patches).toEqual([{ density: "spacious" }]);
  unmount();
  await act(async () => undefined);
  expect(state.preferenceEvents).toEqual([]);
});

test("a failed queued edit restores the preceding acknowledged save when rereading also fails", async () => {
  const { api, state, commit } = fakeApi({ granted: true });
  const pending = deferred<void>();
  vi.spyOn(api, "setPickerPreferences")
    .mockImplementationOnce(async (patch, key) => { await pending.promise; return commit(patch, key); })
    .mockRejectedValueOnce("settings-read-only");
  render(<SettingsApp api={api} initialPage="look" />);
  await screen.findByRole("radio", { name: "Spacious" });
  vi.spyOn(api, "pickerPreferences").mockRejectedValueOnce(new Error("synthetic failed read"));
  fireEvent.click(screen.getByRole("radio", { name: "Spacious" }));
  fireEvent.click(screen.getByRole("radio", { name: "Larger" }));
  await act(async () => pending.resolve());
  expect(screen.getByRole("alert").textContent).toBe("The settings file is read-only; the saved value is shown.");
  expect(screen.getByRole("radio", { name: "Spacious" })).toHaveProperty("checked", true);
  expect(screen.getByRole("radio", { name: "Default" })).toHaveProperty("checked", true);
  expect(state.preferences).toMatchObject({ density: "spacious", larger_text: false });
});

test("a self-notification before reply preserves the saved row choice when the next save and every read fail", async () => {
  const { api, state, commit } = fakeApi({ granted: true });
  const first = deferred<void>();
  vi.spyOn(api, "setPickerPreferences")
    .mockImplementationOnce(async (patch, key, revision) => {
      await first.promise;
      const saved = await commit(patch, key);
      // Native emits its normal revision-tagged event before returning the save.
      for (const changed of state.preferenceEvents) changed(revision);
      return saved;
    })
    .mockRejectedValueOnce("settings-read-only");
  render(<SettingsApp api={api} initialPage="look" />);
  await screen.findByRole("radio", { name: "Spacious" });
  vi.spyOn(api, "pickerPreferences").mockRejectedValue(new Error("synthetic read failure"));
  fireEvent.click(screen.getByRole("radio", { name: "Spacious" }));
  fireEvent.click(screen.getByRole("radio", { name: "Larger" }));
  await act(async () => first.resolve());
  expect(screen.getByRole("alert").textContent).toBe("The settings file is read-only; the saved value is shown.");
  expect(screen.getByRole("radio", { name: "Spacious" })).toHaveProperty("checked", true);
  expect(screen.getByRole("radio", { name: "Default" })).toHaveProperty("checked", true);
  expect(state.preferences).toMatchObject({ density: "spacious", larger_text: false });
});

test("a picker notification after a recovered refusal retires the saved-value claim until a real read succeeds", async () => {
  const { api, state, commit } = fakeApi({ granted: true });
  vi.spyOn(api, "setPickerPreferences").mockRejectedValueOnce("settings-read-only");
  render(<SettingsApp api={api} initialPage="look" />);
  await screen.findByRole("radio", { name: "Spacious" });
  const read = vi.spyOn(api, "pickerPreferences");
  fireEvent.click(screen.getByRole("radio", { name: "Spacious" }));
  await act(async () => state.closes[0]!());
  expect(read).toHaveBeenCalledOnce();
  expect(screen.getByRole("radio", { name: "Comfortable" })).toHaveProperty("checked", true);
  expect(screen.getByRole("alert")).toHaveProperty("textContent", "The settings file is read-only; the saved value is shown.");

  const refreshing = deferred<PickerPreferences>();
  read.mockRejectedValue(new Error("synthetic failed read")).mockReturnValueOnce(refreshing.promise);
  await act(async () => {
    await commit({ density: "spacious" });
    for (const changed of state.preferenceEvents) changed(++state.revision);
  });
  expect(screen.queryByText(/the saved value is shown/)).toBeNull();
  await act(async () => refreshing.reject());
  const uncertainty = "Ibis could not read your settings; reopen Settings to try again.";
  expect(screen.getByRole("alert")).toHaveProperty("textContent", uncertainty);
  expect(state.preferences.density).toBe("spacious");
  await act(async () => { fireEvent.focus(window); });
  expect(screen.getByRole("alert")).toHaveProperty("textContent", uncertainty);

  read.mockImplementation(async () => state.preferences);
  await act(async () => { fireEvent.focus(window); });
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.getByRole("radio", { name: "Spacious" })).toHaveProperty("checked", true);
});

test("a picker notification before a queued save fails stays uncertain until the saved settings can be read", async () => {
  const { api, state, commit } = fakeApi({ granted: true }, { accent: "system" });
  const reply = deferred<void>();
  const write = vi.spyOn(api, "setPickerPreferences")
    .mockImplementationOnce(async (patch, key, revision) => {
      const saved = await commit(patch, key);
      for (const changed of state.preferenceEvents) changed(revision);
      await reply.promise;
      return saved;
    })
    .mockRejectedValueOnce("settings-read-only");
  render(<SettingsApp api={api} initialPage="look" />);
  await screen.findByRole("radio", { name: "Spacious" });
  const read = vi.spyOn(api, "pickerPreferences").mockRejectedValue(new Error("synthetic read failure"));
  fireEvent.click(screen.getByRole("radio", { name: "Spacious" }));
  fireEvent.click(screen.getByRole("radio", { name: "Larger" }));
  await waitFor(() => expect(write).toHaveBeenCalledTimes(1));
  await act(async () => {
    await commit({ accent: "purple" });
    for (const changed of state.preferenceEvents) changed(++state.revision);
    reply.resolve();
  });
  expect(write).toHaveBeenCalledTimes(2);
  expect(state.preferences).toMatchObject({ density: "spacious", accent: "purple", larger_text: false });
  const uncertainty = "Ibis could not read your settings; reopen Settings to try again.";
  expect(screen.getByRole("alert")).toHaveProperty("textContent", uncertainty);
  expect(screen.queryByText(/the saved value is shown/)).toBeNull();

  fireEvent.click(screen.getByRole("option", { name: "General" }));
  expect(screen.getByRole("heading", { level: 1 })).toHaveProperty("textContent", "General");
  await act(async () => { fireEvent.focus(window); });
  expect(screen.getByRole("alert")).toHaveProperty("textContent", uncertainty);
  fireEvent.click(screen.getByRole("option", { name: "Look" }));
  fireEvent.click(screen.getByRole("radio", { name: "Default" }));
  await act(async () => state.closes[0]!());
  expect(screen.getByRole("alert")).toHaveProperty("textContent", uncertainty);

  read.mockImplementation(async () => state.preferences);
  await act(async () => { fireEvent.focus(window); });
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.getByRole("radio", { name: "Spacious" })).toHaveProperty("checked", true);
  expect(screen.getByRole("radio", { name: "Purple" })).toHaveProperty("checked", true);
  expect(screen.getByRole("radio", { name: "Default" })).toHaveProperty("checked", true);
});

test("a picker change survives a Settings save reply delayed beyond the close timeout", async () => {
  const { api, state, commit } = fakeApi({ granted: true });
  const pending = deferred<Awaited<ReturnType<SettingsApi["setPickerPreferences"]>>>();
  let older!: Awaited<ReturnType<SettingsApi["setPickerPreferences"]>>;
  vi.spyOn(api, "setPickerPreferences").mockImplementationOnce(async (patch, key) => {
    older = await commit(patch, key);
    return pending.promise;
  });
  const mounted = render(<SettingsApp api={api} initialPage="look" />);
  await screen.findByRole("radio", { name: "Compact" });
  vi.useFakeTimers();
  fireEvent.click(screen.getByRole("radio", { name: "Compact" }));
  await act(async () => undefined);
  await commit({ density: "spacious" });
  await act(async () => { for (const changed of state.preferenceEvents) changed(++state.revision); });
  await act(async () => vi.advanceTimersByTimeAsync(2_500));
  await act(async () => pending.resolve(older));
  expect(screen.getByRole("radio", { name: "Spacious" })).toHaveProperty("checked", true);
  expect(state.preferences.density).toBe("spacious");
  mounted.unmount();
  vi.useRealTimers();
  render(<SettingsApp api={api} initialPage="look" />);
  expect(await screen.findByRole("radio", { name: "Spacious" })).toHaveProperty("checked", true);
});

test("Appearance, ten accents, both search icons, both text sizes and preview persist separately", async () => {
  const { api, state } = fakeApi({ granted: true });
  const mounted = render(<SettingsApp api={api} initialPage="look" />);
  const accents = within(await screen.findByRole("radiogroup", { name: "Accent" })).getAllByRole("radio");
  expect(accents).toHaveLength(10);
  expect(accents.map((radio) => radio.getAttribute("aria-label"))).toEqual(expect.arrayContaining(["Follow the Mac", "Blue", "Ibis scarlet"]));
  expect(accents[0]?.getAttribute("aria-label")).toBe("Follow the Mac");
  expect(accents.at(-1)?.getAttribute("aria-label")).toBe("Ibis scarlet");
  fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
  fireEvent.click(screen.getByRole("radio", { name: "Purple" }));
  fireEvent.click(screen.getByRole("radio", { name: "Ibis mark" }));
  fireEvent.click(screen.getByRole("radio", { name: "Larger" }));
  fireEvent.click(screen.getByRole("switch", { name: "Show what will be pasted" }));
  await act(async () => state.closes[0]!());
  expect(state.preferences).toMatchObject({ appearance: "dark", accent: "purple", search_icon: "ibis", larger_text: true, show_paste_preview: false });
  expect(state.patches.at(0)).toEqual({ appearance: "dark" });
  expect(state.patches.at(1)).toEqual({ accent: "purple" });
  mounted.unmount();
  render(<SettingsApp api={api} initialPage={null} />);
  expect(screen.getByRole("heading", { level: 1 })).toHaveProperty("textContent", "General");
  fireEvent.click(screen.getByRole("option", { name: "Look" }));
  expect(await screen.findByRole("radio", { name: "Purple" })).toHaveProperty("checked", true);
  expect(document.documentElement.dataset.appearance).toBe("dark");
  fireEvent.click(screen.getByRole("radio", { name: "System" }));
  fireEvent.click(screen.getByRole("radio", { name: "Follow the Mac" }));
  fireEvent.click(screen.getByRole("radio", { name: "Magnifier" }));
  fireEvent.click(screen.getByRole("radio", { name: "Default" }));
  await act(async () => state.closes.at(-1)!());
  expect(state.preferences).toMatchObject({ appearance: "system", accent: "system", search_icon: "magnifier", larger_text: false });
});

test("a failed initial preference read explains uncertainty and leaves navigation usable", async () => {
  const { api, state } = fakeApi();
  vi.spyOn(api, "pickerPreferences").mockRejectedValueOnce(new Error("private OS data"));
  render(<SettingsApp api={api} initialPage="look" />);
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Ibis could not read your settings; reopen Settings to try again.");
  expect(screen.queryByRole("radiogroup")).toBeNull();
  fireEvent.click(screen.getByRole("option", { name: "General" }));
  expect(screen.getByRole("heading", { level: 1 })).toHaveProperty("textContent", "General");
  expect(state.patches).toEqual([]);
});

test("native login and automatic update choices persist, and a refused update explains its cause", async () => {
  const { api, state } = fakeApi({ granted: true });
  const mounted = render(<SettingsApp api={api} initialPage="general" />);
  const login = await screen.findByRole("switch", { name: /Open at Login|Start when|Start on Login/ });
  fireEvent.click(login);
  await act(async () => state.closes[0]!());
  expect(login).toHaveProperty("checked", true);
  fireEvent.click(screen.getByRole("option", { name: "About Ibis" }));
  const automatic = await screen.findByRole("switch", { name: "Check for updates automatically" });
  fireEvent.click(automatic);
  await waitFor(() => expect(state.updates).toBe(false));
  vi.spyOn(api, "setUpdateChecks").mockImplementationOnce(async () => { for (const changed of state.automatic) changed(false); return undefined as never; });
  fireEvent.click(automatic);
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Ibis could not save this choice; the saved value is shown.");
  expect(automatic).toHaveProperty("checked", false);
  mounted.unmount();
  render(<SettingsApp api={api} initialPage="general" />);
  expect(await screen.findByRole("switch", { name: /Open at Login|Start when|Start on Login/ })).toHaveProperty("checked", true);
});

test("Look picture radios have keyboard selection, inert real components and no axe violations", async () => {
  const { api } = fakeApi({ granted: true });
  const { container } = render(<SettingsApp api={api} initialPage="look" />);
  const rows = await screen.findByRole("radiogroup", { name: "Rows" });
  const selected = within(rows).getByRole("radio", { name: "Comfortable" });
  act(() => selected.focus());
  fireEvent.keyDown(selected, { key: "ArrowRight" });
  expect(within(rows).getByRole("radio", { name: "Spacious" })).toHaveProperty("checked", true);
  const previews = container.querySelectorAll(".ui-picker-picture-content > div");
  expect(previews.length).toBeGreaterThan(0);
  for (const host of previews) {
    expect(host.hasAttribute("inert")).toBe(true);
    expect(host.shadowRoot?.querySelector(".search-bar input, #results")).toBeTruthy();
  }
  expect((await axe.run(container)).violations.map((x) => x.id)).toEqual([]);
});


test("the Ibis default-format menu saves key or style, restores refusals, and reopens on General", async () => {
  const { api, state } = fakeApi({ granted: true });
  const mounted = render(<SettingsApp api={api} initialPage="citations" />);
  const choose = async (name: string) => {
    fireEvent.click(await screen.findByRole("button", { name: /^Default format / }));
    expect(await screen.findByRole("menu")).toBeTruthy();
    fireEvent.click(screen.getByRole("menuitemradio", { name }));
    await act(async () => state.closes.at(-1)!());
  };
  await choose("Typst");
  expect(state.preferences).toMatchObject({ copy_format: "typst", citation_style: null });
  await choose("APA (7th edition)");
  expect(state.preferences.citation_style).toBe("apa");
  vi.spyOn(api, "setPickerPreferences").mockRejectedValueOnce(new Error("synthetic refusal"));
  await choose("LaTeX \\cite");
  await screen.findByRole("alert");
  expect(screen.getByRole("button", { name: "Default format APA (7th edition)" })).toBeTruthy();
  expect(document.querySelector("select")).toBeNull();
  mounted.unmount();
  render(<SettingsApp api={api} initialPage={null} />);
  expect(screen.getByRole("heading", { level: 1 })).toHaveProperty("textContent", "General");
  fireEvent.click(screen.getByRole("option", { name: "Citations" }));
  expect(await screen.findByRole("button", { name: "Default format APA (7th edition)" })).toBeTruthy();
});


test.each([false, true])("Citations forgets formats with row-local refusal, retry and brief confirmation (pseudo %s)", async (pseudolocale) => {
  const { api, state } = fakeApi({}, { citation_style: "apa" });
  const messages = createInterfaceMessages(english, pseudolocale);
  const refused = deferred<null>();
  const forget = vi.spyOn(api, "forgetAppOutputs").mockReturnValueOnce(refused.promise);
  const view = () => <InterfaceMessagesProvider messages={messages}><SettingsApp api={api} initialPage="citations" /></InterfaceMessagesProvider>;
  const mounted = render(view());
  await screen.findByRole("button", { name: `${messages.text("citations-default")} ${messages.text("citations-format-apa")}` });
  const button = screen.getByRole("button", { name: messages.text("citations-forget-button") });
  const row = within(button.closest<HTMLElement>(".ui-settings-row")!);
  expect(row.getByText(messages.text("citations-forget"))).toBeTruthy();
  expect(row.getByText(messages.text("citations-forget-description"))).toBeTruthy();
  fireEvent.click(button);
  expect(button).toHaveProperty("disabled", true);
  fireEvent.click(button);
  await waitFor(() => expect(forget).toHaveBeenCalledExactlyOnceWith());
  expect(row.queryByRole("status")).toBeNull();
  await act(async () => refused.reject());
  expect(row.getByRole("alert")).toHaveProperty("textContent", messages.text("citations-forget-failed"));
  expect(row.queryByRole("status")).toBeNull();
  expect(button).toHaveProperty("disabled", false);
  vi.useFakeTimers({ shouldAdvanceTime: true });
  fireEvent.click(button);
  await act(async () => state.closes.at(-1)!());
  expect(forget).toHaveBeenCalledTimes(2);
  expect(row.queryByRole("alert")).toBeNull();
  expect(row.getByRole("status")).toHaveProperty("textContent", messages.text("citations-forget-done"));
  expect(state.preferences.citation_style).toBe("apa");
  expect(state.patches).toEqual([]);
  await act(async () => vi.advanceTimersByTime(4000));
  expect(row.queryByRole("status")).toBeNull();
  mounted.unmount();
  render(view());
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
  expect(await screen.findByRole("button", { name: `${messages.text("citations-default")} ${messages.text("citations-format-apa")}` })).toBeTruthy();
});

test.each(["success", "failed I/O"])("closing waits for a reset, and its late %s cannot replace a reopened row's refusal", async (outcome) => {
  const { api, state } = fakeApi();
  const old = deferred<null>();
  vi.spyOn(api, "forgetAppOutputs").mockReturnValueOnce(old.promise).mockRejectedValueOnce(new Error("synthetic I/O failure"));
  const mounted = render(<SettingsApp api={api} initialPage="citations" />);
  fireEvent.click(screen.getByRole("button", { name: "Forget formats" }));
  const closed = vi.fn();
  const closing = state.closes.at(-1)!().then(closed);
  await act(async () => undefined);
  expect(closed).not.toHaveBeenCalled();
  mounted.unmount();
  render(<SettingsApp api={api} initialPage="citations" />);
  fireEvent.click(screen.getByRole("button", { name: "Forget formats" }));
  const problem = await screen.findByRole("alert");
  expect(problem).toHaveProperty("textContent", "Ibis could not forget formats for apps. Try again.");
  await act(async () => { if (outcome === "success") old.resolve(null); else old.reject(); await closing; });
  expect(closed).toHaveBeenCalledOnce();
  expect(problem).toHaveProperty("textContent", "Ibis could not forget formats for apps. Try again.");
  expect(screen.queryByRole("status")).toBeNull();
  fireEvent.click(screen.getByRole("option", { name: "Look" }));
  fireEvent.click(screen.getByRole("option", { name: "Citations" }));
  expect(screen.getByRole("alert")).toHaveProperty("textContent", "Ibis could not forget formats for apps. Try again.");
});

test.each(["general", "shortcuts"])("%s default shortcut offers Undo that restores the previous saved chord", async (page) => {
  const { api, state } = fakeApi();
  state.shortcut = { shortcut: "Alt+K", registered: true, error: null };
  render(<SettingsApp api={api} initialPage={page} />);
  fireEvent.click(await screen.findByRole("button", { name: "Use Default" }));
  fireEvent.click(await screen.findByRole("button", { name: "Undo Restore Default Shortcut" }));
  await act(async () => state.closes[0]!());
  expect(state.shortcut.shortcut).toBe("Alt+K");
  expect(state.shortcutChoices).toEqual([null, "Alt+K"]);
  expect(screen.queryByRole("button", { name: "Undo Restore Default Shortcut" })).toBeNull();
});


test("a read-only refusal names the file constraint beside the restored control", async () => {
  const { api } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="look" />);
  const choice = await screen.findByRole("radio", { name: "Spacious" });
  vi.spyOn(api, "setPickerPreferences").mockRejectedValueOnce("settings-read-only");
  fireEvent.click(choice);
  const problem = await screen.findByRole("alert");
  expect(problem).toHaveProperty("textContent", "The settings file is read-only; the saved value is shown.");
  expect(problem.closest(".ui-settings-row")?.querySelector("#rows")).toBeTruthy();
  expect(screen.getByRole("radio", { name: "Comfortable" })).toHaveProperty("checked", true);
});


test("a failed update read retains the saved switch and explains the uncertainty beside it", async () => {
  const { api } = fakeApi();
  render(<SettingsApp api={api} initialPage="about" />);
  const automatic = await screen.findByRole("switch", { name: "Check for updates automatically" });
  vi.spyOn(api, "updateChecks").mockRejectedValueOnce(new Error("synthetic I/O"));
  fireEvent.focus(window);
  const problem = await screen.findByRole("alert");
  expect(screen.getByRole("switch", { name: "Check for updates automatically" })).toBe(automatic);
  expect(automatic).toHaveProperty("checked", true);
  expect(problem.closest(".ui-settings-row")?.querySelector("#automatic")).toBeTruthy();
});


test.each([
  ["no remembered format", "Default format", "Citations", "default-format"],
  ["next time you paste", "Forget formats for apps", "Citations", "forget-formats"],
  ["Follow the system", "Appearance", "Look", "appearance"],
])("search includes the displayed description %s and reveals its row", async (query, label, page, id) => {
  const { api } = fakeApi({ granted: true });
  render(<SettingsApp api={api} initialPage="general" />);
  fireEvent.change(screen.getByRole("searchbox", { name: "Search Settings" }), { target: { value: query } });
  fireEvent.click(await screen.findByRole("button", { name: label }));
  expect(screen.getByRole("heading", { level: 1 })).toHaveProperty("textContent", page);
  await waitFor(() => expect(document.getElementById(id)?.closest(".ui-settings-row")?.hasAttribute("data-highlighted")).toBe(true));
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

test("a newer shortcut event survives an older General save after switching pages", async () => {
  const { api, state } = fakeApi();
  const save = deferred<ShortcutStatus>();
  vi.spyOn(api, "setShortcut").mockReturnValueOnce(save.promise);
  render(<SettingsApp api={api} initialPage="general" />);
  const recorder = await screen.findByRole("button", { name: /^Open Ibis:/ });
  fireEvent.click(recorder);
  fireEvent.keyDown(recorder, { key: "k", code: "KeyK", altKey: true });
  fireEvent.click(screen.getByRole("option", { name: "Shortcuts" }));
  expect(screen.getByRole("button", { name: /^Open Ibis:/ })).toHaveProperty("disabled", true);
  act(() => state.shortcuts[0]!({ shortcut: "Alt+J", registered: true, error: null }));
  await act(async () => save.resolve({ shortcut: "Alt+K", registered: true, error: null }));
  expect(screen.getByRole("button", { name: /^Open Ibis:/ }).textContent).toBe(shortcutLabel("Alt+J", currentPlatform() === "macos"));
});

test.each(["light", "dark"] as const)("Ibis scarlet persists and reopens in %s appearance", async (appearance) => {
  const { api, state } = fakeApi({}, { appearance });
  const pseudo = createInterfaceMessages(english, true);
  const view = () => <InterfaceMessagesProvider messages={pseudo}><SettingsApp api={api} initialPage="look" /></InterfaceMessagesProvider>;
  const mounted = render(view());
  fireEvent.click(await screen.findByRole("radio", { name: pseudo.text("look-accent-ibis") }));
  await act(async () => state.closes[0]!());
  expect(state.patches).toEqual([{ accent: "ibis" }]);
  expect(document.documentElement.dataset.accent).toBe("ibis");
  mounted.unmount();
  render(view());
  expect(await screen.findByRole("radio", { name: pseudo.text("look-accent-ibis") })).toHaveProperty("checked", true);
  expect(document.documentElement.dataset.appearance).toBe(appearance);
});

test("an Ibis accent refusal restores the saved accent beside its localized cause", async () => {
  const { api } = fakeApi({}, { accent: "blue" });
  const pseudo = createInterfaceMessages(english, true);
  render(<InterfaceMessagesProvider messages={pseudo}><SettingsApp api={api} initialPage="look" /></InterfaceMessagesProvider>);
  vi.spyOn(api, "setPickerPreferences").mockRejectedValueOnce("settings-read-only");
  fireEvent.click(await screen.findByRole("radio", { name: pseudo.text("look-accent-ibis") }));
  const problem = await screen.findByRole("alert");
  expect(problem.textContent).toBe(pseudo.text("settings-read-only"));
  expect(problem.closest(".ui-settings-row")?.querySelector("#accent")).toBeTruthy();
  expect(screen.getByRole("radio", { name: pseudo.text("look-accent-blue") })).toHaveProperty("checked", true);
  expect(document.documentElement.dataset.accent).toBe("blue");
});


test("a delayed accent read cannot replace the newer Ibis choice or its reopened value", async () => {
  const { api, state } = fakeApi({}, { accent: "blue" });
  const old = deferred<PickerPreferences>();
  const mounted = render(<SettingsApp api={api} initialPage="look" />);
  await screen.findByRole("radio", { name: "Blue" });
  const previous = state.preferences;
  vi.spyOn(api, "pickerPreferences").mockReturnValueOnce(old.promise);
  fireEvent.focus(window);
  fireEvent.click(screen.getByRole("radio", { name: "Ibis scarlet" }));
  await act(async () => state.closes[0]!());
  await act(async () => old.resolve(previous));
  expect(screen.getByRole("radio", { name: "Ibis scarlet" })).toHaveProperty("checked", true);
  expect(document.documentElement.dataset.accent).toBe("ibis");
  mounted.unmount();
  render(<SettingsApp api={api} initialPage="look" />);
  expect(await screen.findByRole("radio", { name: "Ibis scarlet" })).toHaveProperty("checked", true);
});

test("About copies the version and says so", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  const pseudo = createInterfaceMessages(english, true);
  render(<InterfaceMessagesProvider messages={pseudo}><SettingsApp api={fakeApi().api} initialPage="about" /></InterfaceMessagesProvider>);
  fireEvent.click(await screen.findByRole("button", { name: pseudo.text("about-copy") }));
  await screen.findByRole("button", { name: pseudo.text("about-copied") });
  expect(writeText).toHaveBeenCalledOnce();
});
