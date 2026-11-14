// What the Settings window may call; tests replace it with a fake.
import { getVersion } from "@tauri-apps/api/app";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { commands, type EndpointStatus, type PastePermission, type PreferenceChange, type ShortcutStatus, type WindowSize } from "../bindings";

export type { PastePermission } from "../bindings";

export const settingsApi = {
  cliStatus: commands.cliStatus,
  setCliInstalled: commands.setCliInstalled,
  copyCliProfile: commands.copyCliProfile,
  delightView: commands.delightView,
  pickerPreferences: commands.pickerPreferences,
  savePickerPreferences: commands.savePickerPreferences,
  pickerPreferencesSaved: commands.pickerPreferencesSaved,
  windowSize: commands.windowSize,
  setWindowSize: (size: WindowSize, undo: number | null = null) => commands.setWindowSize(size, undo),
  loginItem: commands.loginItem,
  setLoginItem: commands.setLoginItem,
  updateChecks: commands.updateChecks,
  setUpdateChecks: commands.setUpdateChecks,
  checkForUpdates: commands.checkForUpdates,
  openAboutLink: commands.openAboutLink,
  pastePermission: commands.pastePermission as () => Promise<PastePermission>,
  openPastePermission: commands.openPastePermission,
  dragPasteApp: async (): Promise<void> => { await commands.dragPasteApp(); },
  finishPasteGuide: commands.finishPasteGuide,
  shortcutStatus: commands.shortcutStatus,
  setShortcut: commands.setShortcut,
  configureShortcuts: commands.configureShortcuts,
  editorEndpoint: commands.editorEndpoint,
  setEditorEndpoint: commands.setEditorEndpoint,
  copyEditorEndpoint: commands.copyEditorEndpoint,
  onEditorEndpoint: (handler: (status: EndpointStatus) => void): Promise<UnlistenFn> => listen<EndpointStatus>("editor:endpoint", (event) => handler(event.payload)),
  backUpSettings: commands.backUpSettings,
  chooseSettingsBackup: commands.chooseSettingsBackup,
  restoreSettingsBackup: commands.restoreSettingsBackup,
  cancelSettingsRestore: commands.cancelSettingsRestore,
  formatSettings: commands.formatSettings,
  forgetAppFormat: commands.forgetAppFormat,
  openSettingsFile: commands.openSettingsFile,
  onShortcutStatus: (handler: (status: ShortcutStatus) => void): Promise<UnlistenFn> => listen<ShortcutStatus>("shortcut:status", (event) => handler(event.payload)),
  version: getVersion,
  /** Settings is asked to show a page, such as the paste guide after a refused paste. */
  onPreferencesChanged: (handler: (revision: number, fields?: string[]) => void): Promise<UnlistenFn> => listen<PreferenceChange>("preferences:changed", (event) => handler(event.payload.revision, event.payload.fields)),
  /** The format snapshot includes app choices and writings as well as preferences. */
  onSettingsChanged: async (handler: () => void): Promise<UnlistenFn> => {
    const stopPreferences = await listen("preferences:changed", handler);
    try {
      const stopWritings = await listen("writings:changed", handler);
      return () => { stopPreferences(); stopWritings(); };
    } catch (error) {
      stopPreferences();
      throw error;
    }
  },
  /** The window size writer's outcome; a refused write is the only news here. */
  onWindowSaved: (handler: (saved: boolean, revision: number) => void): Promise<UnlistenFn> => listen<{ saved: boolean; revision: number }>("picker:window-saved", (event) => handler(event.payload.saved, event.payload.revision)),
  onPage: (handler: (page: string) => void): Promise<UnlistenFn> => listen<string>("settings:page", (event) => handler(event.payload)),
  /** The saved automatic update choice, after a change from any menu. */
  onUpdateChecks: (handler: (on: boolean) => void): Promise<UnlistenFn> => listen<boolean>("update:automatic", (event) => handler(event.payload)),
  onCloseRequested: (handler: () => Promise<void>): Promise<UnlistenFn> => getCurrentWindow().onCloseRequested(handler),
};

type Optional = "cliStatus" | "setCliInstalled" | "copyCliProfile" | "delightView" | "editorEndpoint" | "setEditorEndpoint" | "copyEditorEndpoint" | "onEditorEndpoint" | "formatSettings" | "forgetAppFormat" | "openSettingsFile";
export type SettingsApi = Omit<typeof settingsApi, Optional> & Partial<Pick<typeof settingsApi, Optional>>;
