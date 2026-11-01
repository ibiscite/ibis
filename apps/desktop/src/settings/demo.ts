import type { DelightFact, PickerPreferences, ShortcutStatus } from "../bindings";
import { DEFAULT_PICKER_SHORTCUT } from "../keys";
import type { SettingsApi } from "./api";

/** Explicit development-only fixture: no native commands, files or network. */
export function demoApi(): SettingsApi {
  let preferences: PickerPreferences = { citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true, search_icon: "ibis", paste: true, menu_bar: true, appearance: "system", accent: "system" };
  let shortcut: ShortcutStatus = { shortcut: DEFAULT_PICKER_SHORTCUT, registered: true, error: null };
  let enabled = false, automatic = false;
  let revision = 0;
  let clicks = 0, factIndex = -1;
  const facts: DelightFact[] = ["thoth", "touch", "scarlet", "mummies", "national", "flight"];
  const updateListeners = new Set<(on: boolean) => void>();
  const listeners = new Set<(revision: number) => void>();
  const noop = async () => undefined as never;
  const subscribe = async () => () => undefined;
  return {
    delightView: async (_now, _presentation, _revision, click) => {
      const fun = preferences.small_surprises ?? true;
      if (fun && click && ++clicks % 5 === 0) factIndex = (factIndex + 1) % 6;
      return { fun, fact: fun && factIndex >= 0 ? facts[factIndex]! : null, status: null, library: null, birthday: false, peck: false, anniversary: false, revision, build: "0.2.1 · invented · 2026-11-01" };
    },
    pickerPreferences: async () => preferences,
    preferenceRevision: async () => ++revision,
    setPickerPreferences: async (patch, key, revision) => { preferences = { ...preferences, ...patch, ...(key ? { copy_format: key } : {}) }; for (const changed of listeners) changed(revision); return { preferences, copy_format: preferences.copy_format ?? "pandoc" }; },
    windowSize: async () => "medium",
    setWindowSize: async () => null,
    onWindowSaved: subscribe,
    forgetAppOutputs: async () => null,
    loginItem: async () => ({ name: "Ibis", enabled, problem: null }),
    setLoginItem: async (on) => { enabled = on; return { name: "Ibis", enabled, problem: null }; },
    updateChecks: async () => automatic,
    setUpdateChecks: async (on) => { automatic = on; for (const changed of updateListeners) changed(on); return undefined as never; },
    checkForUpdates: noop,
    openAboutLink: noop,
    pastePermission: async () => ({ pastes: true, needed: true, granted: true, list: "Accessibility" }),
    openPastePermission: noop, dragPasteApp: noop, finishPasteGuide: async () => true,
    shortcutStatus: async () => shortcut,
    setShortcut: async (value) => { shortcut = { ...shortcut, shortcut: value ?? DEFAULT_PICKER_SHORTCUT }; return shortcut; },
    configureShortcuts: async () => null, version: async () => "0.2.1",
    onShortcutStatus: subscribe, onPage: subscribe, onUpdateChecks: async (changed) => { updateListeners.add(changed); return () => { updateListeners.delete(changed); }; }, onCloseRequested: subscribe,
    onPreferencesChanged: async (changed) => { listeners.add(changed); return () => { listeners.delete(changed); }; },
  };
}
