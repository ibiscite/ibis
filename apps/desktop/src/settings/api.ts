// What the Settings window may call; tests replace it with a fake.
import { getVersion } from "@tauri-apps/api/app";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { commands, type PastePermission } from "../bindings";

export type { PastePermission } from "../bindings";

export const settingsApi = {
  pickerPreferences: commands.pickerPreferences,
  setPickerPreferences: commands.setPickerPreferences,
  loginItem: commands.loginItem,
  setLoginItem: commands.setLoginItem,
  updateChecks: commands.updateChecks,
  setUpdateChecks: commands.setUpdateChecks,
  checkForUpdates: commands.checkForUpdates,
  pastePermission: commands.pastePermission as () => Promise<PastePermission>,
  openPastePermission: commands.openPastePermission,
  version: getVersion,
  /** Settings is asked to show a page, such as the paste guide after a refused paste. */
  onPage: (handler: (page: string) => void): Promise<UnlistenFn> => listen<string>("settings:page", (event) => handler(event.payload)),
  /** The saved automatic update choice, after a change from any menu. */
  onUpdateChecks: (handler: (on: boolean) => void): Promise<UnlistenFn> => listen<boolean>("update:automatic", (event) => handler(event.payload)),
};

export type SettingsApi = typeof settingsApi;
