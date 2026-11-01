// What the Settings window may call; tests replace it with a fake.
import { getVersion } from "@tauri-apps/api/app";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { commands, type KeyFormat, type PastePermission, type PickerPreferences, type PreferenceChange, type ShortcutStatus } from "../bindings";

export type { PastePermission } from "../bindings";

export const settingsApi = {
  pickerPreferences: commands.pickerPreferences,
  preferenceRevision: commands.preferenceRevision,
  setPickerPreferences: (preferences: Partial<PickerPreferences>, copyFormat: KeyFormat | null, revision: number) => commands.setPickerPreferences(preferences, copyFormat, revision),
  windowSize: commands.windowSize,
  setWindowSize: commands.setWindowSize,
  forgetAppOutputs: commands.forgetAppOutputs,
  loginItem: commands.loginItem,
  setLoginItem: commands.setLoginItem,
  updateChecks: commands.updateChecks,
  setUpdateChecks: commands.setUpdateChecks,
  checkForUpdates: commands.checkForUpdates,
  pastePermission: commands.pastePermission as () => Promise<PastePermission>,
  openPastePermission: commands.openPastePermission,
  dragPasteApp: async (): Promise<void> => { await commands.dragPasteApp(); },
  finishPasteGuide: commands.finishPasteGuide,
  shortcutStatus: commands.shortcutStatus,
  setShortcut: commands.setShortcut,
  configureShortcuts: commands.configureShortcuts,
  onShortcutStatus: (handler: (status: ShortcutStatus) => void): Promise<UnlistenFn> => listen<ShortcutStatus>("shortcut:status", (event) => handler(event.payload)),
  version: getVersion,
  /** Settings is asked to show a page, such as the paste guide after a refused paste. */
  onPreferencesChanged: (handler: (revision: number) => void): Promise<UnlistenFn> => listen<PreferenceChange>("preferences:changed", (event) => handler(event.payload.revision)),
  onPage: (handler: (page: string) => void): Promise<UnlistenFn> => listen<string>("settings:page", (event) => handler(event.payload)),
  /** The saved automatic update choice, after a change from any menu. */
  onUpdateChecks: (handler: (on: boolean) => void): Promise<UnlistenFn> => listen<boolean>("update:automatic", (event) => handler(event.payload)),
  onCloseRequested: (handler: () => Promise<void>): Promise<UnlistenFn> => getCurrentWindow().onCloseRequested(handler),
};

export type SettingsApi = typeof settingsApi;
