import { composing } from "../composition";
import { filterPalette } from "../ui/Palette";
import { Details } from "../ui/Details";
import { useSettingUndo } from "../ui/useSettingUndo";
import { useRowReveal } from "../ui/useRowReveal";
import { CliCommandRow } from "./CliCommand";
import { StatusMark } from "../ui/StatusMark";
import { type ReactNode, useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { PageIcon } from "../icons";
import type { CheckResult, LoginItem, LoginItemProblem, PickerPreferences, ShortcutStatus, WindowSize } from "../bindings";
import { type MessageId, useInterfaceMessages } from "../i18n";
import { type ShortcutProblem, shortcutRefusalMessage } from "../i18n/settings";
import { PreferencePatchQueue } from "../preferencePatchQueue";
import { currentPlatform } from "../platform";
import { NoticeSlot } from "../ui/NoticeSlot";
import { useLayer } from "../ui/layers";
import { useScrollEdges } from "../ui/ScrollArea";
import { UndoNotice } from "../ui/UndoNotice";
import { Button } from "../ui/Button";
import { AboutDelight, AboutLine } from "../ui/Delights";
import { localMoment } from "../ui/localMoment";
import { useAltHeld } from "../features/picker/useAltHeld";
import type { DelightFact, DelightView } from "../bindings";
import { ShortcutsPage } from "./ShortcutsPage";
import { Group, Note, Row, Section } from "../ui/Layout";
import { ShortcutSetting } from "../ui/ShortcutSetting";
import { SideNavigation, SideResults } from "../ui/SideNavigation";
import { SettingsSearch } from "../ui/SettingsSearch";
import { useAppearance } from "../ui/appearance";
import { DefaultFormat } from "./DefaultFormat";
import { Switch } from "../ui/Switch";
import { EditorEndpointRow } from "./EditorEndpoint";
import { PasteGuide, usePastePermission } from "./PasteGuide";
import { BackupRows } from "./BackupRows";
import type { SettingsApi } from "./api";
import "../ui/SettingsLayout.css";

import { PAGES, ROWS, settingsRows, rowDetails, preferenceDefault, type Page } from "./rows";
import { LookPage } from "./LookPage";
import { initialPreferences, preferenceState } from "./preferenceState";
const FACT_MESSAGES: Record<DelightFact, MessageId> = { touch: "delight-fact-touch", scarlet: "delight-fact-scarlet", mummies: "delight-fact-mummies", national: "delight-fact-national", flight: "delight-fact-flight" };
const LOGIN_PROBLEMS = {
  unsupported: "general-login-unsupported", needs_approval: "general-login-needs-approval",
  refused: "general-login-refused", unreadable: "general-login-unreadable",
} as const satisfies Record<LoginItemProblem, MessageId>;

/** A page asked for by name; "paste" is General, scrolled to pasting. */
const PAGE_TILES: Record<Page, ReactNode> = { general: <PageIcon name="general" />, look: <PageIcon name="look" />, citations: <PageIcon name="citations" />, shortcuts: <PageIcon name="shortcuts" />, about: <PageIcon name="about" /> };

export function pageOf(name: string | null): { page: Page; focus: "paste" | null } {
  if (name === "paste") return { page: "general", focus: "paste" };
  return { page: PAGES.includes(name as Page) ? name as Page : "general", focus: null };
}

/** The Settings window: a sidebar of pages, each a stack of titled groups of
 * rows (docs/design/settings.md). Every change applies at once. */
/** How far below a page's top a scrolled-to group lands: under the title, which stays put. */
class WindowSizeSaveError extends Error {
  constructor(readonly retryTicket: number | null) { super("Window size could not be saved"); }
}
const underTitle = (page: HTMLElement) => page.querySelector<HTMLElement>(".ui-settings-page-title")?.offsetHeight ?? 0;

export function SettingsApp({ api, initialPage }: { api: SettingsApi; initialPage: string | null }) {
  const { text } = useInterfaceMessages();
  const platform = currentPlatform();
  const start = pageOf(initialPage);
  const [page, setPage] = useState<Page>(start.page);
  const [focus, setFocus] = useState<"top" | "paste" | null>(start.focus ?? "top");
  const pageRef = useRef<HTMLElement>(null);
  useScrollEdges(pageRef);
  const [preference, dispatchPreference] = useReducer(preferenceState, initialPreferences);
  const preferences = preference.value;
  const [login, setLogin] = useState<LoginItem | null>(null);
  const [updates, setUpdates] = useState<boolean | null>(null);
  const latestLogin = useRef<LoginItem | null>(null);
  const latestUpdates = useRef<boolean | null>(null);
  const updateWrite = useRef<{ on: boolean; resolve: () => void; reject: () => void } | null>(null);
  const [version, setVersion] = useState("");
  const [readProblems, setReadProblems] = useState<ReadonlySet<string>>(() => new Set());
  const readProblem = useCallback((id: string, failed: boolean) => setReadProblems((current) => {
    const next = new Set(current);
    if (failed) next.add(id); else next.delete(id);
    return next;
  }), []);
  const [problemRow, setProblemRow] = useState<{ id: string; message: MessageId } | null>(null);
  const [windowSize, setWindowSize] = useState<WindowSize | null>(null);
  const windowSizes = useRef(0);
  const windowUndoRevision = useRef<number | null>(null);
  const latestWindowSize = useRef<WindowSize | null>(null);
  const windowSaved = useRef<{ revision: number; saved: boolean } | null>(null);
  const windowSaveWaiter = useRef<{ revision: number; resolve: () => void; reject: () => void } | null>(null);
  const cause = useRef<string | null>(null);
  const edits = useRef(0);
  const live = useRef(true);
  const [query, setQuery] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);
  const resultsRef = useRef<HTMLDivElement>(null);
  const searchOpener = useRef<HTMLElement | null>(null);
  const [revealTarget, setRevealTarget] = useState<{ id: string; ticket: number } | null>(null);
  // Search is on the view stack (ui/layers) while the keyboard is in the sidebar:
  // Esc clears it, then leaves it for where you were. On the page, Esc stays the
  // page's (the shortcut recorder cancels with it).
  const sidebar = useRef<HTMLElement>(null);
  const [inSidebar, setInSidebar] = useState(false);
  useLayer("search", inSidebar, () => {
    setRevealTarget(null);
    if (query) { setQuery(""); searchRef.current?.focus(); return; }
    const back = searchOpener.current;
    if (back?.isConnected && !back.closest("[hidden], [inert]")) back.focus({ preventScroll: true });
    else sidebar.current?.querySelector<HTMLElement>('[aria-current="page"]')?.focus();
  });
  useRowReveal(revealTarget, pageRef);
  const [highlighted, setHighlighted] = useState<string | null>(null);
  const highlightOperation = useRef(0);
  useEffect(() => { live.current = true; return () => { live.current = false; }; }, []);
  const [shortcut, setShortcut] = useState<ShortcutStatus | null>(null);
  const [shortcutProblem, setShortcutProblem] = useState<ShortcutProblem | null>(null);
  const [shortcutUndo, setShortcutUndo] = useState<string | null>(null);
  const shortcutUndoTicket = useRef(0);
  const clearShortcutUndo = useCallback(() => { shortcutUndoTicket.current += 1; setShortcutUndo(null); }, []);
  const latestShortcut = useRef<ShortcutStatus | null>(null);
  const [shortcutSaving, setShortcutSaving] = useState(false);
  const shortcutBusy = useRef(false);
  const shortcutReads = useRef(0);
  const updateReads = useRef(0);
  const loginReads = useRef(0);
  const loginBusy = useRef(false);
  const [loginSaving, setLoginSaving] = useState(false);
  const [updatesSaving, setUpdatesSaving] = useState(false);
  const [versionCopied, setVersionCopied] = useState(false);
  const [versionCopyFailed, setVersionCopyFailed] = useState(false);
  const [checkResult, setCheckResult] = useState<CheckResult | null>(null);
  const [checking, setChecking] = useState(false);
  const manualCheck = useRef(0);
  const checkRunning = useRef(false);
  /** About's Check for Updates; the tray and menu bar check natively (updater/setting.rs). */
  const checkNow = useCallback(() => {
    if (checkRunning.current) return;
    checkRunning.current = true;
    const operation = ++manualCheck.current;
    setCheckResult(null);
    setChecking(true);
    const finished = (result: CheckResult) => { if (live.current && operation === manualCheck.current) setCheckResult(result); };
    void api.checkForUpdates().then(finished, () => finished("failed"))
      .finally(() => { checkRunning.current = false; if (live.current) setChecking(false); });
  }, [api]);
  const versionCopy = useRef(0);
  // Feedback belongs to this page presentation, independently of delight preferences.
  // biome-ignore lint/correctness/useExhaustiveDependencies: changing pages retires feedback and pending operations.
  useEffect(() => {
    setVersionCopied(false); setVersionCopyFailed(false); versionCopy.current += 1;
    return () => { versionCopy.current += 1; };
  }, [page]);
  const { altHeld } = useAltHeld("option");
  const [about, setAbout] = useState<DelightView | null>(null);
  const [fact, setFact] = useState<DelightFact | null>(null);
  const [dip, setDip] = useState(0);
  const clickAbout = useRef<() => void>(() => {});
  // biome-ignore lint/correctness/useExhaustiveDependencies: a saved switch change retires old authorization answers.
  useEffect(() => {
    let active = true;
    // A click has spent its fact, so a refresh must never retire its answer.
    let reads = 0, clicks = 0;
    let timer: number | undefined;
    setAbout(null); setFact(null);
    const read = (click = false) => {
      if (page !== "about" || !api.delightView) return;
      const operation = click ? ++clicks : ++reads;
      const current = () => active && operation === (click ? clicks : reads);
      const day = localMoment();
      window.clearTimeout(timer);
      void api.delightView(day, null, null, click).then((next) => {
        if (!current()) return;
        if (click && next?.fun) { setDip((value) => value + 1); setFact(next.fact); }
        const now = localMoment();
        if (day.year !== now.year || day.month !== now.month || day.day !== now.day) { read(); return; }
        setAbout(next);
        if (next?.anniversary) {
          const midnight = new Date(); midnight.setHours(24, 0, 0, 0);
          timer = window.setTimeout(() => { if (current()) { setAbout(null); read(); } }, midnight.getTime() - Date.now());
        }
      }).catch(() => undefined);
    };
    const focusRead = () => read();
    clickAbout.current = () => read(true);
    read();
    window.addEventListener("focus", focusRead);
    return () => { active = false; window.clearTimeout(timer); window.removeEventListener("focus", focusRead); clickAbout.current = () => {}; };
  }, [page, api.delightView, preferences?.small_surprises]);
  useEffect(() => { if (about?.fun === false) setDip(0); }, [about?.fun]);
  const updateIntent = useRef<boolean | null>(null);
  /** The guide stays to confirm a permission given while it was open. */
  const [guided, setGuided] = useState(false);
  const [detour, setDetour] = useState(initialPage === "paste");
  const [returnFailed, setReturnFailed] = useState(false);
  const [focused, setFocused] = useState(true);
  const settingUndo = useSettingUndo(!focused, searchRef);
  const clearSettingUndo = useCallback(() => { windowUndoRevision.current = null; settingUndo.clear(); }, [settingUndo.clear]);
  const queue = useMemo(() => new PreferencePatchQueue({
    read: async () => ({ preferences: await api.pickerPreferences() }),
    submit: (patch) => api.savePickerPreferences(patch),
    saved: (receipt) => api.pickerPreferencesSaved(receipt),
  }, { restoreOnFailure: true }), [api]);
  useEffect(() => queue.subscribe((event) => {
    if (!live.current) return;
    if (event.type === "error") {
      if (event.source === "read") dispatchPreference({ type: "read-failed" });
    } else if (event.type === "invalidated") {
      clearSettingUndo();
      dispatchPreference(event);
    } else if (event.type === "read" || event.type === "saved") {
      if (event.type === "read") dispatchPreference({ type: "read", value: event.value.preferences });
      else dispatchPreference({ type: "saved", value: event.value.preferences });
    } else if (event.type === "saving") {
      const operation = edits.current;
      const row = cause.current;
      void event.promise.catch((reason: unknown) => {
        if (live.current && operation === edits.current) {
          dispatchPreference({ type: "refused", saved: queue.savedPreferences,
            problem: { row, message: reason === "settings-read-only" ? "settings-read-only" : "settings-choice-failed" } });
        }
      });
    }
  }), [queue, clearSettingUndo]);
  useAppearance(preferences);
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.largerText = String(preferences?.larger_text ?? false);
    return () => { delete root.dataset.largerText; };
  }, [preferences?.larger_text]);
  // Shortcuts reads it too, to name what Enter does; only General waits for a grant.
  const permission = usePastePermission(api, page === "general" || page === "shortcuts" || Boolean(query.trim()), page === "general" && preferences?.paste === true);
  const enterPastes = permission?.pastes === true && (!permission.needed || permission.granted) && preferences?.paste === true;

  useEffect(() => {
    const focus = () => setFocused(true), blur = () => setFocused(false);
    window.addEventListener("focus", focus);
    window.addEventListener("blur", blur);
    return () => { window.removeEventListener("focus", focus); window.removeEventListener("blur", blur); };
  }, []);
  useEffect(() => {
    if (!detour || !focused || !preferences?.paste || !permission?.granted) return;
    let live = true;
    setReturnFailed(false);
    // Finish queued preferences before leaving Settings, as on a normal close.
    void queue.finishSaving().then(async () => {
      if (live && await api.finishPasteGuide()) setDetour(false);
    }).catch(() => { if (live) setReturnFailed(true); });
    return () => { live = false; };
  }, [api, queue, detour, focused, permission, preferences?.paste]);

  useEffect(() => {
    let active = true;
    const readShortcut = () => {
      if (shortcutBusy.current) return;
      const request = ++shortcutReads.current;
      api.shortcutStatus().then((next) => {
        if (active && request === shortcutReads.current) {
          if (latestShortcut.current?.shortcut !== next.shortcut) clearShortcutUndo();
          latestShortcut.current = next;
          setShortcut(next);
          setShortcutProblem((problem) => problem?.id === "shortcuts-read-failed" ? null : problem);
        }
      }).catch(() => {
        if (active && request === shortcutReads.current) setShortcutProblem({ id: "shortcuts-read-failed" });
      });
    };
    const readAll = (withPreferences: boolean | Event = true) => {
      if (withPreferences !== false) void queue.load(() => active);
      if (!loginBusy.current) {
        const loginRequest = ++loginReads.current;
        api.loginItem().then((next) => { if (active && loginRequest === loginReads.current) { if (latestLogin.current && latestLogin.current.enabled !== next.enabled) clearSettingUndo(); latestLogin.current = next; setLogin(next); readProblem("login", false); } })
          .catch(() => { if (active && loginRequest === loginReads.current) readProblem("login", true); });
      }
      if (updateWrite.current) return;
      const updateRequest = ++updateReads.current;
      api.updateChecks().then((next) => {
        if (active && updateRequest === updateReads.current) { if (latestUpdates.current !== null && latestUpdates.current !== next) clearSettingUndo(); latestUpdates.current = next; setUpdates(next); readProblem("automatic", false); }
      }).catch(() => {
        if (active && updateRequest === updateReads.current) readProblem("automatic", true);
      });
    };
    const preferenceEvents = api.onPreferencesChanged((revision) => {
      void queue.refresh(revision, () => active);
    });
    readAll(false);
    void preferenceEvents.then(() => { if (active) void queue.load(() => active); }).catch(() => { if (active) dispatchPreference({ type: "read-failed" }); });
    api.version().then((next) => { if (active) setVersion(next); }).catch(() => undefined);
    // Changes made in the picker or the menu bar while Settings was open.
    window.addEventListener("focus", readAll);
    window.addEventListener("focus", readShortcut);
    const shortcuts = api.onShortcutStatus((next) => {
      shortcutReads.current += 1;
      if (active) { latestShortcut.current = next; setShortcut(next); clearShortcutUndo(); }
    });
    void shortcuts.catch(() => { if (active) setShortcutProblem({ id: "shortcuts-read-failed" }); }).then(() => { if (active) readShortcut(); });
    const page = api.onPage((name) => { setRevealTarget(null); highlightOperation.current += 1; setHighlighted(null); setQuery(""); const next = pageOf(name); setDetour(name === "paste"); setDip(0); setPage(next.page); setFocus(next.focus ?? "top"); });
    const automatic = api.onUpdateChecks((next) => {
      if (!active) return;
      updateReads.current += 1;
      if (!updateWrite.current) clearSettingUndo();
      latestUpdates.current = next;
      setUpdates(next);
      const pending = updateWrite.current;
      updateWrite.current = null;
      if (pending) { if (pending.on === next) pending.resolve(); else pending.reject(); }
      if (updateIntent.current !== null) {
        setProblemRow(updateIntent.current === next ? null : { id: "automatic", message: "settings-choice-failed" });
        if (updateIntent.current === next) updateIntent.current = null;
      }
      setUpdatesSaving(false);
    });
    const close = api.onCloseRequested(() => queue.finishSaving());
    return () => {
      active = false;
      updateWrite.current?.reject(); updateWrite.current = null;
      window.removeEventListener("focus", readAll);
      window.removeEventListener("focus", readShortcut);
      void preferenceEvents.then((stop) => stop()).catch(() => undefined);
      void shortcuts.then((stop) => stop()).catch(() => undefined);
      void page.then((stop) => stop()).catch(() => undefined);
      void automatic.then((stop) => stop()).catch(() => undefined);
      void close.then((stop) => stop()).catch(() => undefined);
    };
  }, [api, queue, readProblem, clearShortcutUndo, clearSettingUndo]);
  useEffect(() => {
    const scroller = pageRef.current;
    if (!focus || !scroller) return;
    if (focus === "paste") {
      // Wait for both reads before looking for the group. Move only the page,
      // never the document containing the sidebar and native title bar.
      if (!preferences || !permission) return;
      const paste = document.getElementById("paste");
      scroller.scrollTop = paste ? scroller.scrollTop + paste.getBoundingClientRect().top - scroller.getBoundingClientRect().top - underTitle(scroller) : 0;
    } else scroller.scrollTop = 0;
    setFocus(null);
  }, [focus, preferences, permission]);

  useEffect(() => {
    const find = (event: KeyboardEvent) => {
      if (composing(event) || event.key.toLowerCase() !== "f" || event.altKey || event.shiftKey
        || !(platform === "macos" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey)) return;
      event.preventDefault(); event.stopPropagation();
      if (document.activeElement !== searchRef.current && document.activeElement instanceof HTMLElement) searchOpener.current = document.activeElement;
      setRevealTarget(null);
      searchRef.current?.focus(); searchRef.current?.select();
    };
    window.addEventListener("keydown", find, true);
    return () => window.removeEventListener("keydown", find, true);
  }, [platform]);

  const change = (next: Partial<PickerPreferences>, restoring = false): Promise<void> => {
    if (!preferences) return Promise.reject(new Error("Settings unavailable"));
    if (!restoring) clearSettingUndo();
    edits.current += 1;
    cause.current = next.output !== undefined ? "default-format" : next.appearance !== undefined ? "appearance" : next.accent !== undefined ? "accent"
      : next.density !== undefined ? "rows" : next.larger_text !== undefined ? "text-size"
      : next.paste !== undefined ? "paste-label" : next.menu_bar !== undefined ? "menu-bar" : next.small_surprises !== undefined ? "small-surprises" : "recent-citations";
    setProblemRow(null);
    dispatchPreference({ type: "change", value: { ...preferences, ...next } });
    return queue.change(next);
  };
  const changeShortcut = (next: string | null, restoring = false, ticket = shortcutUndoTicket.current): Promise<void> => {
    if (shortcutBusy.current || ticket !== shortcutUndoTicket.current) return Promise.resolve();
    shortcutBusy.current = true;
    const previous = shortcut?.shortcut;
    if (!restoring) { clearShortcutUndo(); clearSettingUndo(); }
    const request = ++shortcutReads.current;
    setShortcutSaving(true);
    setShortcutProblem(null);
    return queue.enqueue(() => api.setShortcut(next))
      .then((status) => {
        if (!live.current) return;
        if (request === shortcutReads.current) { latestShortcut.current = status; setShortcut(status); }
        if (restoring) clearShortcutUndo();
        if (next === null && previous && latestShortcut.current?.shortcut === status.shortcut) {
          shortcutUndoTicket.current += 1; setShortcutUndo(previous);
        }
      })
      .catch((refusal: unknown) => {
        setShortcutProblem(shortcutRefusalMessage(refusal));
        if (restoring) throw refusal;
      })
      .finally(() => { shortcutBusy.current = false; setShortcutSaving(false); });
  };

  const configureShortcut = () => {
    if (shortcutBusy.current) return;
    shortcutBusy.current = true;
    setShortcutSaving(true);
    setShortcutProblem(null);
    void api.configureShortcuts().catch(() => setShortcutProblem({ id: "shortcuts-configure-failed" }))
      .finally(() => { shortcutBusy.current = false; setShortcutSaving(false); });
  };
  const changeLogin = async (on: boolean, restoring = false) => {
    if (loginBusy.current) throw new Error("Login change pending");
    if (!restoring) clearSettingUndo();
    setProblemRow(null); readProblem("login", false); loginBusy.current = true;
    const request = ++loginReads.current;
    setLoginSaving(true);
    try {
      const next = await queue.enqueue(() => api.setLoginItem(on));
      if (!live.current || request !== loginReads.current) throw new Error("Login change superseded");
      latestLogin.current = next; setLogin(next);
      if (next.enabled !== on || next.problem) throw new Error("Login change refused");
    } catch (error) {
      if (live.current && request === loginReads.current) setProblemRow({ id: "login", message: "settings-choice-failed" });
      throw error;
    } finally {
      loginBusy.current = false;
      if (live.current && request === loginReads.current) setLoginSaving(false);
    }
  };
  const changeUpdates = async (on: boolean, restoring = false) => {
    if (updateWrite.current) throw new Error("Update change pending");
    if (!restoring) clearSettingUndo();
    setProblemRow(null); updateIntent.current = on; setUpdatesSaving(true);
    const operation = ++updateReads.current;
    let refuse = () => {};
    const applied = new Promise<void>((resolve, reject) => { refuse = () => reject(new Error("Update change refused")); updateWrite.current = { on, resolve, reject: refuse }; });
    try { await Promise.all([api.setUpdateChecks(on), applied]); }
    catch (error) {
      if (live.current && operation === updateReads.current) {
        refuse(); updateWrite.current = null;
        setProblemRow({ id: "automatic", message: "settings-choice-failed" });
      }
      throw error;
    } finally { if (live.current) setUpdatesSaving(false); }
  };
  const resetPreference = (id: string) => {
    const row = ROWS.find((row) => row.id === id);
    const reset = row && preferences && preferenceDefault(row, preferences);
    const defaultValue = row?.default;
    if (id === "login" && typeof defaultValue === "boolean" && login && login.enabled !== defaultValue && !login.problem) return { label: rowDetails(id, text).label, disabled: loginSaving,
      onReset: () => { void settingUndo.reset(rowDetails(id, text).label, () => changeLogin(defaultValue), () => changeLogin(login.enabled, true)); } };
    if (id === "automatic" && typeof defaultValue === "boolean" && updates !== null && updates !== defaultValue) return { label: rowDetails(id, text).label, disabled: updatesSaving,
      onReset: () => { void settingUndo.reset(rowDetails(id, text).label, () => changeUpdates(defaultValue), () => changeUpdates(updates, true)); } };
    if (id === "window-size" && defaultValue === "medium" && windowSize && windowSize !== defaultValue) return { label: rowDetails(id, text).label,
      onReset: () => {
        let ticket: number | null = null;
        void settingUndo.reset(rowDetails(id, text).label,
          async () => { ticket = await chooseWindowSize(defaultValue, null, true); windowUndoRevision.current = ticket; },
          async () => {
            if (ticket === null) throw new Error("Window Undo unavailable");
            try { await chooseWindowSize(windowSize, ticket, true, true); }
            catch (error) {
              if (error instanceof WindowSizeSaveError) {
                ticket = error.retryTicket; windowUndoRevision.current = ticket;
                if (ticket === null) clearSettingUndo();
              }
              throw error;
            }
          });
      } };
    if (!row || !reset || preference.status === "unknown") return undefined;
    return { label: rowDetails(id, text).label, onReset: () => {
      void settingUndo.reset(rowDetails(id, text).label, async () => {
        await change(reset.patch);
      }, async () => { await change(reset.previous, true); });
    } };
  };
  const feedback = (id: string) => ({ reset: resetPreference(id), highlighted: highlighted === id, problem: readProblems.has(id) ? text("settings-read-failed")
    : preference.status === "refused" && preference.problem.row === id ? text(preference.problem.message)
    : problemRow?.id === id ? text(problemRow.message) : undefined });
  const checkSaid = checkResult === "newest" ? "about-check-newest" : checkResult === "not_run" ? "about-check-busy" : null;
  const selectPage = (next: Page) => { setRevealTarget(null); highlightOperation.current += 1; setHighlighted(null); setDetour(false); setPage(next); setFocus("top"); setDip(0); };
  const reveal = (row: typeof ROWS[number]) => {
    selectPage(row.page); setFocus(null); setQuery(""); setHighlighted(row.id === "about-ibis" && query.trim().toLowerCase() === "thoth" ? "about-name" : row.id);
    if (row.id === "about-ibis" && query.trim().toLowerCase() !== "thoth") setDip(dip + 1);
    const operation = ++highlightOperation.current;
    setRevealTarget({ id: row.id === "open-shortcut" && row.page === "general" ? "general-open-shortcut" : row.id, ticket: operation });
    window.setTimeout(() => { if (live.current && operation === highlightOperation.current) setHighlighted(null); }, 2000);
  };
  // Look reads on showing, so a size dragged meanwhile appears as Custom.
  const readWindowSize = useCallback((request: number) => api.windowSize().then((size) => {
    if (live.current && request === windowSizes.current) { if (latestWindowSize.current !== null && latestWindowSize.current !== size) clearSettingUndo(); latestWindowSize.current = size; setWindowSize(size); readProblem("window-size", size === null); }
  }, () => { if (live.current && request === windowSizes.current) readProblem("window-size", true); }), [api, readProblem, clearSettingUndo]);
  useEffect(() => {
    if (focused && page === "look") void readWindowSize(++windowSizes.current);
  }, [focused, page, readWindowSize]);
  useEffect(() => {
    const stop = api.onWindowSaved((saved, revision) => {
      windowSaved.current = { revision, saved };
      if (windowUndoRevision.current !== null && revision > windowUndoRevision.current) clearSettingUndo();
      const pending = windowSaveWaiter.current;
      if (pending && revision >= pending.revision) {
        windowSaveWaiter.current = null;
        if (saved && revision === pending.revision) pending.resolve(); else pending.reject();
      }
      if (live.current && !saved) setProblemRow({ id: "window-size", message: "settings-choice-failed" });
    });
    return () => { windowSaveWaiter.current?.reject(); windowSaveWaiter.current = null; void stop.then((unlisten) => unlisten()); };
  }, [api, clearSettingUndo]);
  const chooseWindowSize = async (size: WindowSize, undo: number | null = null, waitForSave = false, restoring = false): Promise<number> => {
    if (!restoring) clearSettingUndo();
    else windowUndoRevision.current = null;
    const request = ++windowSizes.current;
    windowSaveWaiter.current?.reject(); windowSaveWaiter.current = null;
    const previous = latestWindowSize.current;
    let revision: number | undefined;
    setProblemRow(null); latestWindowSize.current = size; setWindowSize(size);
    try {
      revision = undo === null ? await api.setWindowSize(size) : await api.setWindowSize("custom", undo);
      if (!live.current || request !== windowSizes.current) throw new Error("Window choice superseded");
      if (waitForSave) {
        if (windowSaved.current && windowSaved.current.revision >= revision) {
          if (windowSaved.current.revision !== revision || !windowSaved.current.saved) throw new Error("Window choice not saved");
        } else {
          await new Promise<void>((resolve, reject) => { windowSaveWaiter.current = { revision: revision!, resolve, reject: () => reject(new Error("Window choice not saved")) }; });
        }
      }
      return revision;
    } catch (error) {
      let retryTicket: number | null = null;
      if (live.current && request === windowSizes.current) {
        if (waitForSave && revision !== undefined) {
          // A reset that did not save returns to its native snapshot, including
          // Custom dimensions. The ticket refuses any later geometry choice.
          try {
            retryTicket = await api.setWindowSize("custom", revision);
            if (live.current && request === windowSizes.current) { latestWindowSize.current = previous; setWindowSize(previous); }
          } catch { void readWindowSize(request); }
        } else void readWindowSize(request);
        if (live.current && request === windowSizes.current) setProblemRow({ id: "window-size", message: "settings-choice-failed" });
      }
      if (waitForSave) throw new WindowSizeSaveError(retryTicket);
      throw error;
    }
  };
  const copyVersion = () => {
    const operation = ++versionCopy.current;
    setVersionCopied(false);
    setVersionCopyFailed(false);
    void navigator.clipboard.writeText(`${name} ${altHeld && about?.build ? about.build : version}`).then(() => {
      if (!live.current || operation !== versionCopy.current) return;
      setVersionCopied(true);
      window.setTimeout(() => { if (live.current && operation === versionCopy.current) setVersionCopied(false); }, 1500);
    }, () => { if (live.current && operation === versionCopy.current) setVersionCopyFailed(true); });
  };
  const availableRows = settingsRows(enterPastes).filter((row) => (row.id !== "menu-bar" || platform === "macos")
    && (row.id !== "paste-label" || permission?.pastes === true)
    && ((row.id !== "automatic" && row.id !== "check-updates") || updates !== null || readProblems.has("automatic"))
    && (row.id !== "cli-command" || Boolean(api.cliStatus && api.setCliInstalled && api.copyCliProfile))
    && (row.id !== "editor-endpoint" || Boolean(api.editorEndpoint && api.setEditorEndpoint && api.copyEditorEndpoint)));
  const matchedIds = new Set(query.trim() ? filterPalette([{ id: "settings", items: availableRows.map((row) => ({ id: row.id,
    label: text(row.label, { platform }), detail: row.description ? text(row.description, { platform }) : undefined,
    keywords: row.keywords ? text(row.keywords, { platform }) : undefined })) }], query).flatMap((group) => group.items.map((item) => item.id)) : []);
  const matches = PAGES.flatMap((page) => availableRows.filter((row) => row.page === page && matchedIds.has(row.id)));
  const name = login?.name ?? "Ibis";
  const undoTicket = shortcutUndoTicket.current;
  const shortcutSetting = <ShortcutSetting key={page} id={page === "general" ? "general-shortcut" : "open-ibis"}
    details={rowDetails("open-shortcut", text)} rowId={page === "general" ? "general-open-shortcut" : "open-shortcut"} status={shortcut} problem={shortcutProblem}
    table={page === "shortcuts"} saving={shortcutSaving} highlighted={highlighted === "open-shortcut"} echo={page === "shortcuts"}
    onRecord={changeShortcut} onConfigure={configureShortcut} />;

  return (
    <div className="settings">
      <div className="settings-titlebar" data-tauri-drag-region />
      <nav ref={sidebar} className="settings-sidebar" aria-label={text("settings-pages")} onFocus={() => setInSidebar(true)}
        onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) setInSidebar(false); }}>
        <SettingsSearch inputRef={searchRef} label={text("settings-search")} value={query}
          onChange={(value) => { setRevealTarget(null); setQuery(value); }}
          onKeyDown={(event) => {
            if ((event.key === "ArrowDown" || event.key === "ArrowUp") && matches.length) {
              event.preventDefault();
              const options = resultsRef.current?.querySelectorAll<HTMLElement>('[role="option"]');
              options?.[event.key === "ArrowDown" ? 0 : options.length - 1]?.focus();
            }
          }} onSubmit={() => { if (matches[0]) reveal(matches[0]); }} />
        {query.trim() ? <SideResults listRef={resultsRef} label={text("settings-results")} status={text("settings-results-count", { count: matches.length })} empty={text("settings-no-results", { query })}
          results={matches.map((row) => ({ id: row.id, label: text(row.label, { platform }), page: row.page, pageLabel: text(`settings-${row.page}`), tile: PAGE_TILES[row.page] }))}
          onChoose={(id) => reveal(matches.find((row) => row.id === id)!)} />
        : <SideNavigation label={text("settings-pages")} value={page} options={PAGES.map((value) => ({ value, label: text(`settings-${value}`), tile: PAGE_TILES[value], gapBefore: value === "about" }))}
          onChange={selectPage} />}
      </nav>
      <NoticeSlot>
        {settingUndo.notice}
        {shortcutUndo && <UndoNotice key={undoTicket} action={text("undo-shortcut")} message={text("settings-shortcut-restored")} paused={!focused}
          onUndo={() => changeShortcut(shortcutUndo, true, undoTicket)}
          onExpire={() => { if (undoTicket === shortcutUndoTicket.current) clearShortcutUndo(); }} />}
      </NoticeSlot>
      <main ref={pageRef} className="settings-page" aria-labelledby="settings-page-title">
        <h1 id="settings-page-title" className={page === "about" ? "visually-hidden" : "ui-settings-page-title"}>{text(`settings-${page}`)}</h1>
        {preference.status === "unknown" && preference.message && <Note problem role="alert">{text(preference.message)}</Note>}
        {detour && returnFailed && <Note problem role="alert">{text("general-paste-return-failed")}</Note>}
        {page === "general" && (
          <>
            {shortcutSetting}
            <Section id="startup" title={text("general-behavior-title")}><Group>
                <Row id="login" {...rowDetails("login", text)} description={login?.problem ? text(LOGIN_PROBLEMS[login.problem], { platform, name }) : rowDetails("login", text).description} {...feedback("login")}>
                  <Switch aria-labelledby="login" aria-describedby="login-description" isSelected={login?.enabled ?? false}
                    onChange={(on) => { void changeLogin(on).catch(() => undefined); }} isDisabled={loginSaving || !login || login.problem === "unsupported" || login.problem === "unreadable"} />
                </Row>
                {platform === "macos" && <Row id="menu-bar" {...rowDetails("menu-bar", text)} {...feedback("menu-bar")}>
                  <Switch aria-labelledby="menu-bar" aria-describedby="menu-bar-description" isSelected={preferences?.menu_bar ?? true}
                    onChange={(on) => change({ menu_bar: on })} isDisabled={!preferences} />
                </Row>}
                <Row id="small-surprises" {...rowDetails("small-surprises", text)} {...feedback("small-surprises")}>
                  <Switch aria-labelledby="small-surprises" aria-describedby="small-surprises-description" isSelected={preferences?.small_surprises ?? true} isDisabled={!preferences} onChange={(on) => change({ small_surprises: on })} />
                </Row>
            </Group></Section>
            {preferences && (
              <>
                {/* Only systems where Ibis pastes ask for a permission first. */}
                {permission?.pastes && (
                <Section id="paste" title={text("general-paste-title")}><Group>
                  <Row id="paste-label" {...rowDetails("paste-label", text)} {...feedback("paste-label")}>
                    <Switch aria-labelledby="paste-label" aria-describedby="paste-label-description" isSelected={preferences.paste}
                      onChange={(on) => change({ paste: on })} />
                  </Row>
                  {preferences.paste && permission?.needed && permission.granted && !guided && (
                    <Row id="paste-allowed" label={text("general-paste-allowed", { list: permission.list })} description={text("general-paste-allowed-note")}>
                      <StatusMark kind="ok" />
                    </Row>
                  )}
                  {preferences.paste && permission?.needed && (!permission.granted || guided) && (
                    <PasteGuide name={name} permission={permission} onOpen={() => { setGuided(true); void api.openPastePermission().then(() => readProblem("paste-label", false)).catch(() => readProblem("paste-label", true)); }} onDrag={api.dragPasteApp} />
                  )}
                </Group></Section>
                )}
              </>
            )}
            {api.cliStatus && api.setCliInstalled && api.copyCliProfile && <CliCommandRow api={{ cliStatus: api.cliStatus, setCliInstalled: api.setCliInstalled, copyCliProfile: api.copyCliProfile }} highlighted={highlighted === "cli-command"} />}
            <Details label={text("settings-details")} reveal={revealTarget}><BackupRows api={api} /></Details>
          </>
        )}
        {page === "look" && preferences && <LookPage preferences={preferences} change={change} feedback={feedback} windowSize={windowSize} chooseWindowSize={(size) => { void chooseWindowSize(size).catch(() => undefined); }} />}
        {page === "citations" && <Section id="citation-choices"><Group>
          {preferences && <Row id="default-format" {...rowDetails("default-format", text)} {...feedback("default-format")}>
            <DefaultFormat preferences={preferences} change={change} />
          </Row>}
          {preferences && <Row id="recent-citations" {...rowDetails("recent-citations", text)} {...feedback("recent-citations")}>
            <Switch aria-labelledby="recent-citations" aria-describedby="recent-citations-description" isSelected={preferences.recent_citations ?? true} onChange={(on) => change({ recent_citations: on })} />
          </Row>}
          {api.editorEndpoint && api.setEditorEndpoint && api.copyEditorEndpoint && <EditorEndpointRow resetSetting={settingUndo.reset} onEdit={clearSettingUndo} highlighted={highlighted === "editor-endpoint"}
            api={{ editorEndpoint: api.editorEndpoint, setEditorEndpoint: api.setEditorEndpoint, copyEditorEndpoint: api.copyEditorEndpoint }} />}
        </Group></Section>}
        {page === "shortcuts" && <ShortcutsPage opening={shortcutSetting} enterPastes={enterPastes} reveal={revealTarget} highlighted={highlighted} />}
        {page === "about" && (
          <>
            {/* Centred, as System Settings' About (maintainer, 2026-11-10, option C):
                the icon, name and version, Check for Updates under them, then the
                lines; the groups below are the same cards as every page. */}
            <div id="about-ibis" className="ui-about-hero">
              <AboutDelight dip={dip} enabled={about?.fun ?? false} onClick={() => clickAbout.current()} />
              <p className="ui-about-name">{name}</p>
              <Button variant="quiet" size="small" preventFocusOnPress onPress={copyVersion}>
                {text(versionCopied ? "about-copied" : "about-version", { version: altHeld && about?.build ? about.build : version })}
              </Button>
              {versionCopyFailed && <Note problem role="alert">{text("about-copy-failed")}</Note>}
              {/* Under the version, as Mac apps place it (decision 0038); its outcome follows it. */}
              {(updates !== null || readProblems.has("automatic")) && <div id="check-updates" className="ui-about-check" data-highlighted={highlighted === "check-updates" || undefined}>
                <Button aria-disabled={checking || undefined} onPress={checkNow}>{text(checking ? "about-checking" : "about-check")}</Button>
                <p className="ui-quiet-status" role="status">{checking ? text("about-checking") : checkSaid && text(checkSaid)}</p>
                {checkResult === "failed" && <Note problem role="alert">{text("about-check-failed")}</Note>}
              </div>}
              <p className="ui-about-mission">{text("about-mission")}</p>
              <AboutLine said={fact !== null} highlighted={highlighted === "about-name"}>{text(fact ? FACT_MESSAGES[fact] : about?.anniversary ? "delight-anniversary" : "delight-name")}</AboutLine>
            </div>
            <Section id="about-updates" title={text("about-updates")}><Group>
              <Row id="automatic" {...rowDetails("automatic", text)} description={updates === null && !readProblems.has("automatic") ? text("about-unavailable") : rowDetails("automatic", text).description} {...feedback("automatic")}>
                {(updates !== null || readProblems.has("automatic")) && <Switch aria-labelledby="automatic" aria-describedby="automatic-description" isSelected={updates ?? false}
                    isDisabled={updatesSaving || updates === null} onChange={(on) => { void changeUpdates(on).catch(() => undefined); }} />}
              </Row>
            </Group></Section>
            <div className="ui-about-links" role="group" aria-label={text("about-links")}>
              {(["website", "source", "problem", "licences"] as const).map((link) => <Button key={link} variant="quiet" size="small" onPress={() => { void api.openAboutLink(link); }}>{text(`about-${link}`)}</Button>)}
            </div>
            <p className="ui-about-free">{text("about-free")}</p>
          </>
        )}
      </main>
    </div>
  );
}
