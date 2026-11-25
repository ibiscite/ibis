import { DEFAULT_PREFERENCES, type AgentApp, type CommandStatus, type DelightFact, type PickerPreferences, type ShortcutStatus, type WindowSize } from "../bindings";
import { DEFAULT_PICKER_SHORTCUT } from "../keys";
import { exampleProposals } from "../testing/proposals";
import type { SettingsApi } from "./api";

/** Explicit development-only fixture: no native commands, files or network. */
export function demoApi(initialPreferences: Partial<PickerPreferences> = {}): SettingsApi {
  let proposals = exampleProposals();
  let preferences: PickerPreferences = { output: "pandoc", density: DEFAULT_PREFERENCES.density, larger_text: false, paste: true, menu_bar: true, appearance: "system", accent: "system", interface_language: "system", speaking: "en-US", ...initialPreferences };
  let shortcut: ShortcutStatus = { shortcut: DEFAULT_PICKER_SHORTCUT, registered: true, error: null };
  let enabled = false, automatic = false;
  let attachmentText = false;
  let outsideLibrary = false;
  let revision = 0;
  let factIndex = -1;
  let windowSize: WindowSize = "medium", previousWindowSize: WindowSize = "medium", windowRevision = 0;
  const windowListeners = new Set<(saved: boolean, revision: number) => void>();
  const facts: DelightFact[] = ["touch", "scarlet", "mummies", "national", "flight"];
  const updateListeners = new Set<(on: boolean) => void>();
  const listeners = new Set<(revision: number) => void>();
  const submitted = new Map<number, Partial<PickerPreferences>>();
  const noop = async () => undefined as never;
  let command: CommandStatus = { state: "not-installed", version: null, used_path: null, profile_line: null, removable: false, installable: true };
  const app = (id: string, name: string, state: AgentApp["state"], extra: Partial<AgentApp> = {}): AgentApp =>
    ({ id, name, state, found: true, detail: "", diff: null, needs_command: false, ibis_on_path: true, revision: id, ...extra });
  let agents: AgentApp[] = [
    app("claude-desktop", "Claude Desktop", "not-connected", { needs_command: true }),
    app("claude-code", "Claude Code", "connected"),
    app("cursor", "Cursor", "different", { diff: '- "ibis": {"command":"ibis","args":["mcp","--file","invented.bib"]}\n+ "ibis": {"command":"ibis","args":["mcp","--app-libraries"]}' }),
    app("chatgpt", "ChatGPT desktop", "by-hand", { detail: "In ChatGPT desktop: Settings › MCP servers › Add server › STDIO, command ibis, arguments mcp --app-libraries." }),
  ];
  const subscribe = async () => () => undefined;
  return {
    attachmentTextLibraries: async () => [{ id: "lib_invented_demo_library", name: "Sample references", on: attachmentText }],
    setAttachmentText: async (id, on) => { if (id !== "lib_invented_demo_library") throw new Error("Unknown library"); attachmentText = on; return on; },
    attachmentTextOutsideLibrary: async () => outsideLibrary,
    setAttachmentTextOutsideLibrary: async (on) => { outsideLibrary = on; return on; },
    onLibraryChanged: subscribe,
    waitingProposals: async () => proposals,
    applyProposal: async (id) => { proposals = proposals.filter((proposal) => proposal.id !== id); return { status: "applied", backup: "signals.bib.ibis-backup-1" }; },
    rejectAppProposals: async (id) => { const app = proposals.find((proposal) => proposal.id === id)?.source_app; proposals = proposals.filter((proposal) => proposal.source_app !== app); return { status: "rejected" }; },
    rejectProposal: async (id) => { proposals = proposals.filter((proposal) => proposal.id !== id); return { status: "rejected" }; },
    delightView: async (_now, _presentation, _revision, click) => {
      const fun = preferences.small_surprises ?? DEFAULT_PREFERENCES.small_surprises;
      if (fun && click) factIndex = (factIndex + 1) % facts.length;
      return { fun, fact: fun && click ? facts[factIndex]! : null, status: null, peck: false, anniversary: false, practice: false, revision, build: "0.2.1 · invented · 2026-11-01" };
    },
    pickerPreferences: async () => preferences,
    savePickerPreferences: async (patch) => { const receipt = ++revision; submitted.set(receipt, patch as Partial<PickerPreferences>); return receipt; },
    pickerPreferencesSaved: async (receipt) => {
      preferences = { ...preferences, ...submitted.get(receipt) };
      submitted.delete(receipt);
      for (const changed of listeners) changed(receipt);
      return { preferences };
    },
    windowSize: async () => windowSize,
    setWindowSize: async (size, undo = null) => {
      if (undo !== null && undo !== windowRevision) throw new Error("Window choice superseded");
      const previous = windowSize;
      windowSize = undo === null ? size : previousWindowSize;
      previousWindowSize = previous;
      const revision = ++windowRevision;
      for (const saved of windowListeners) saved(true, revision);
      return revision;
    },
    onWindowSaved: async (saved) => { windowListeners.add(saved); return () => { windowListeners.delete(saved); }; },
    // The demo has no files: backing up and restoring are cancelled at once.
    backUpSettings: async () => null, chooseSettingsBackup: async () => null,
    restoreSettingsBackup: async () => null, cancelSettingsRestore: async () => null,
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
    onSettingsChanged: async (changed) => { listeners.add(changed); return () => { listeners.delete(changed); }; },
    agentApps: async () => agents,
    connectAgentApp: async (id) => { agents = agents.map((app) => app.id === id ? { ...app, state: "connected", diff: null, revision: `${app.revision}+` } : app); return agents; },
    disconnectAgentApp: async (id) => { agents = agents.map((app) => app.id === id ? { ...app, state: "not-connected", revision: `${app.revision}-` } : app); return agents; },
    cliStatus: async () => command,
    setCliInstalled: async (installed) => (command = { ...command, state: installed ? "matching" : "not-installed", version: installed ? "0.2.1" : null, installable: !installed, removable: installed }),
    copyCliProfile: noop,
  };
}
