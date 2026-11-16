// What the Settings window may call; tests replace it with a fake.
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { commands, type PastePermission, type WindowSize } from "../bindings";
import { events, type Stop } from "../nativeEvents";

export type { PastePermission } from "../bindings";

export const settingsApi = {
  waitingProposals: commands.waitingProposals,
  applyProposal: commands.applyProposal,
  rejectProposal: commands.rejectProposal,
  agentApps: commands.agentApps,
  connectAgentApp: commands.connectAgentApp,
  disconnectAgentApp: commands.disconnectAgentApp,
  attachmentTextLibraries: commands.attachmentTextLibraries,
  setAttachmentText: commands.setAttachmentText,
  onLibraryChanged: events.onLibraryChanged,
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
  onEditorEndpoint: events.onEditorEndpoint,
  backUpSettings: commands.backUpSettings,
  chooseSettingsBackup: commands.chooseSettingsBackup,
  restoreSettingsBackup: commands.restoreSettingsBackup,
  cancelSettingsRestore: commands.cancelSettingsRestore,
  formatSettings: commands.formatSettings,
  forgetAppFormat: commands.forgetAppFormat,
  openSettingsFile: commands.openSettingsFile,
  onShortcutStatus: events.onShortcutStatus,
  version: getVersion,
  onPreferencesChanged: (handler: (revision: number, fields?: string[]) => void) => events.onPreferencesChanged((change) => handler(change.revision, change.fields)),
  /** The format snapshot includes app choices and writings as well as preferences. */
  onSettingsChanged: async (handler: () => void): Promise<Stop> => {
    const stopPreferences = await events.onPreferencesChanged(handler);
    try {
      const stopWritings = await events.onWritingsChanged(handler);
      return () => { stopPreferences(); stopWritings(); };
    } catch (error) {
      stopPreferences();
      throw error;
    }
  },
  /** The window size writer's outcome; a refused write is the only news here. */
  onWindowSaved: (handler: (saved: boolean, revision: number) => void) => events.onWindowSaved(({ saved, revision }) => handler(saved, revision)),
  onPage: events.onSettingsPage,
  onUpdateChecks: events.onUpdateChecks,
  onCloseRequested: (handler: () => Promise<void>): Promise<Stop> => getCurrentWindow().onCloseRequested(handler),
};

type Optional = "waitingProposals" | "applyProposal" | "rejectProposal" | "attachmentTextLibraries" | "setAttachmentText" | "onLibraryChanged" | "agentApps" | "connectAgentApp" | "disconnectAgentApp" | "cliStatus" | "setCliInstalled" | "copyCliProfile" | "delightView" | "editorEndpoint" | "setEditorEndpoint" | "copyEditorEndpoint" | "onEditorEndpoint" | "formatSettings" | "forgetAppFormat" | "openSettingsFile";
export type SettingsApi = Omit<typeof settingsApi, Optional> & Partial<Pick<typeof settingsApi, Optional>>;
