import { StatusMark } from "../ui/StatusMark";
import { type ReactNode, useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { PageIcon } from "../icons";
import type { CheckResult, LoginItem, LoginItemProblem, PickerPreferences, ShortcutStatus, WindowSize } from "../bindings";
import { type MessageId, useInterfaceMessages } from "../i18n";
import { type ShortcutProblem, shortcutRefusalMessage } from "../i18n/settings";
import { PreferencePatchQueue } from "../preferencePatchQueue";
import { currentPlatform } from "../platform";
import { NoticeSlot } from "../ui/NoticeSlot";
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

import { PAGES, ROWS, rowDetails, type Page } from "./rows";
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
  const cause = useRef<string | null>(null);
  const edits = useRef(0);
  const live = useRef(true);
  const [query, setQuery] = useState("");
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
  const { altHeld } = useAltHeld();
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
  }), [queue]);
  useAppearance(preferences);
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.largerText = String(preferences?.larger_text ?? false);
    return () => { delete root.dataset.largerText; };
  }, [preferences?.larger_text]);
  // Shortcuts reads it too, to name what Enter does; only General waits for a grant.
  const permission = usePastePermission(api, page === "general" || page === "shortcuts", page === "general" && preferences?.paste === true);
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
        api.loginItem().then((next) => { if (active && loginRequest === loginReads.current) { setLogin(next); readProblem("login", false); } })
          .catch(() => { if (active && loginRequest === loginReads.current) readProblem("login", true); });
      }
      const updateRequest = ++updateReads.current;
      api.updateChecks().then((next) => {
        if (active && updateRequest === updateReads.current) { setUpdates(next); readProblem("automatic", false); }
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
    const page = api.onPage((name) => { const next = pageOf(name); setDetour(name === "paste"); setDip(0); setPage(next.page); setFocus(next.focus ?? "top"); });
    const automatic = api.onUpdateChecks((next) => {
      if (!active) return;
      updateReads.current += 1;
      setUpdates(next);
      if (updateIntent.current !== null) {
        setProblemRow(updateIntent.current === next ? null : { id: "automatic", message: "settings-choice-failed" });
        if (updateIntent.current === next) updateIntent.current = null;
      }
      setUpdatesSaving(false);
    });
    const close = api.onCloseRequested(() => queue.finishSaving());
    return () => {
      active = false;
      window.removeEventListener("focus", readAll);
      window.removeEventListener("focus", readShortcut);
      void preferenceEvents.then((stop) => stop()).catch(() => undefined);
      void shortcuts.then((stop) => stop()).catch(() => undefined);
      void page.then((stop) => stop()).catch(() => undefined);
      void automatic.then((stop) => stop()).catch(() => undefined);
      void close.then((stop) => stop()).catch(() => undefined);
    };
  }, [api, queue, readProblem, clearShortcutUndo]);
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

  const change = (next: Partial<PickerPreferences>) => {
    if (!preferences) return;
    edits.current += 1;
    cause.current = next.output !== undefined ? "default-format" : next.appearance !== undefined ? "appearance" : next.accent !== undefined ? "accent"
      : next.density !== undefined ? "rows" : next.larger_text !== undefined ? "text-size"
      : next.paste !== undefined ? "paste-label" : next.menu_bar !== undefined ? "menu-bar" : next.small_surprises !== undefined ? "small-surprises" : "recent-citations";
    setProblemRow(null);
    dispatchPreference({ type: "change", value: { ...preferences, ...next } });
    queue.change(next);
  };
  const changeShortcut = (next: string | null, restoring = false, ticket = shortcutUndoTicket.current): Promise<void> => {
    if (shortcutBusy.current || ticket !== shortcutUndoTicket.current) return Promise.resolve();
    shortcutBusy.current = true;
    const previous = shortcut?.shortcut;
    if (!restoring) clearShortcutUndo();
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
  const feedback = (id: string) => ({ highlighted: highlighted === id, problem: readProblems.has(id) ? text("settings-read-failed")
    : preference.status === "refused" && preference.problem.row === id ? text(preference.problem.message)
    : problemRow?.id === id ? text(problemRow.message) : undefined });
  const checkSaid = checkResult === "newest" ? "about-check-newest" : checkResult === "not_run" ? "about-check-busy" : null;
  const selectPage = (next: Page) => { highlightOperation.current += 1; setHighlighted(null); setDetour(false); setPage(next); setFocus("top"); setDip(0); };
  const reveal = (row: typeof ROWS[number]) => {
    selectPage(row.page); setQuery(""); setHighlighted(row.id === "about-ibis" && query.trim().toLowerCase() === "thoth" ? "about-name" : row.id);
    if (row.id === "about-ibis" && query.trim().toLowerCase() !== "thoth") setDip(dip + 1);
    const operation = ++highlightOperation.current;
    window.setTimeout(() => { if (live.current && operation === highlightOperation.current) setHighlighted(null); }, 2000);
  };
  useEffect(() => {
    if (!highlighted || !preferences || !pageRef.current) return;
    const target = document.getElementById(highlighted === "open-shortcut" && page === "general" ? "general-open-shortcut" : highlighted)?.closest(".ui-row, tr, .ui-about-check");
    if (target) pageRef.current.scrollTop += target.getBoundingClientRect().top - pageRef.current.getBoundingClientRect().top - underTitle(pageRef.current);
  }, [highlighted, page, preferences]);
  // Look reads on showing, so a size dragged meanwhile appears as Custom.
  const readWindowSize = useCallback((request: number) => api.windowSize().then((size) => {
    if (live.current && request === windowSizes.current) { setWindowSize(size); readProblem("window-size", size === null); }
  }, () => { if (live.current && request === windowSizes.current) readProblem("window-size", true); }), [api, readProblem]);
  useEffect(() => {
    if (focused && page === "look") void readWindowSize(++windowSizes.current);
  }, [focused, page, readWindowSize]);
  useEffect(() => {
    const stop = api.onWindowSaved((saved) => {
      if (live.current && !saved) setProblemRow({ id: "window-size", message: "settings-choice-failed" });
    });
    return () => { void stop.then((unlisten) => unlisten()); };
  }, [api]);
  const chooseWindowSize = (size: WindowSize) => {
    const request = ++windowSizes.current;
    setProblemRow(null); setWindowSize(size);
    api.setWindowSize(size).catch(() => {
      if (!live.current || request !== windowSizes.current) return;
      setProblemRow({ id: "window-size", message: "settings-choice-failed" });
      void readWindowSize(request);
    });
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
  const matches = query.trim() ? ROWS.filter((row) => (row.id !== "menu-bar" || platform === "macos") && [row.label, row.description, row.keywords].some((id) => id && text(id, { platform }).toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))) : [];
  const name = login?.name ?? "Ibis";
  const undoTicket = shortcutUndoTicket.current;
  const shortcutSetting = <ShortcutSetting key={page} id={page === "general" ? "general-shortcut" : "open-ibis"}
    rowId={page === "general" ? "general-open-shortcut" : "open-shortcut"} status={shortcut} problem={shortcutProblem}
    table={page === "shortcuts"} saving={shortcutSaving} highlighted={highlighted === "open-shortcut"} echo={page === "shortcuts"}
    onRecord={changeShortcut} onConfigure={configureShortcut} />;

  return (
    <div className="settings">
      <div className="settings-titlebar" data-tauri-drag-region />
      <nav className="settings-sidebar" aria-label={text("settings-pages")}>
        <SettingsSearch label={text("settings-search")} value={query} onChange={setQuery} onSubmit={() => { if (matches[0]) reveal(matches[0]); }} />
        {query.trim() ? <SideResults label={text("settings-results")} status={text("settings-results-count", { count: matches.length })} empty={text("settings-no-results")}
          results={matches.map((row) => ({ id: row.id, label: text(row.label, { platform }), page: row.page, pageLabel: text(`settings-${row.page}`), tile: PAGE_TILES[row.page] }))}
          onChoose={(id) => reveal(matches.find((row) => row.id === id)!)} />
        : <SideNavigation label={text("settings-pages")} value={page} options={PAGES.map((value) => ({ value, label: text(`settings-${value}`), tile: PAGE_TILES[value], gapBefore: value === "about" }))}
          onChange={selectPage} />}
      </nav>
      <NoticeSlot>
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
            <Section id="startup"><Group>
                <Row id="login" label={text("general-login-label", { platform })} description={login?.problem ? text(LOGIN_PROBLEMS[login.problem], { platform, name }) : text("general-login-description")} {...feedback("login")}>
                  <Switch aria-labelledby="login" aria-describedby="login-description" isSelected={login?.enabled ?? false}
                    onChange={(on) => {
                      setProblemRow(null); readProblem("login", false); loginBusy.current = true;
                      const request = ++loginReads.current;
                      setLoginSaving(true);
                      void queue.enqueue(() => api.setLoginItem(on)).then((next) => { if (live.current && request === loginReads.current) setLogin(next); })
                        .catch(() => { if (live.current && request === loginReads.current) setProblemRow({ id: "login", message: "settings-choice-failed" }); })
                        .finally(() => { if (live.current && request === loginReads.current) setLoginSaving(false); loginBusy.current = false; });
                    }} isDisabled={loginSaving || !login || login.problem === "unsupported" || login.problem === "unreadable"} />
                </Row>
                {platform === "macos" && <Row id="menu-bar" label={text("general-menu-bar-label")} description={text("general-menu-bar-description")} {...feedback("menu-bar")}>
                  <Switch aria-labelledby="menu-bar" aria-describedby="menu-bar-description" isSelected={preferences?.menu_bar ?? true}
                    onChange={(on) => change({ menu_bar: on })} isDisabled={!preferences} />
                </Row>}
                <Row id="small-surprises" label={text("general-small-surprises-label")} description={text("general-small-surprises-description")} {...feedback("small-surprises")}>
                  <Switch aria-labelledby="small-surprises" aria-describedby="small-surprises-description" isSelected={preferences?.small_surprises ?? true} isDisabled={!preferences} onChange={(on) => change({ small_surprises: on })} />
                </Row>
            </Group></Section>
            {preferences && (
              <>
                {/* Only systems where Ibis pastes ask for a permission first. */}
                {permission?.pastes && (
                <Section id="paste" title={text("general-paste-title")}><Group>
                  <Row id="paste-label" label={text("general-paste-label")} description={text("general-paste-description", { platform })} {...feedback("paste-label")}>
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
            <BackupRows api={api} />
          </>
        )}
        {page === "look" && preferences && <LookPage preferences={preferences} change={change} feedback={feedback} windowSize={windowSize} chooseWindowSize={chooseWindowSize} />}
        {page === "citations" && <Section id="citation-choices"><Group>
          {preferences && <Row id="default-format" {...rowDetails("default-format", text)} {...feedback("default-format")}>
            <DefaultFormat preferences={preferences} change={change} />
          </Row>}
          {preferences && <Row id="recent-citations" {...rowDetails("recent-citations", text)} {...feedback("recent-citations")}>
            <Switch aria-labelledby="recent-citations" aria-describedby="recent-citations-description" isSelected={preferences.recent_citations ?? true} onChange={(on) => change({ recent_citations: on })} />
          </Row>}
          {api.editorEndpoint && api.setEditorEndpoint && api.copyEditorEndpoint && <EditorEndpointRow highlighted={highlighted === "editor-endpoint"}
            api={{ editorEndpoint: api.editorEndpoint, setEditorEndpoint: api.setEditorEndpoint, copyEditorEndpoint: api.copyEditorEndpoint }} />}
        </Group></Section>}
        {page === "shortcuts" && <ShortcutsPage opening={shortcutSetting} enterPastes={enterPastes} />}
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
                {checkResult === "failed" && <p className="ui-row-problem" role="alert">{text("about-check-failed")}</p>}
              </div>}
              <p className="ui-about-mission">{text("about-mission")}</p>
              <AboutLine said={fact !== null} highlighted={highlighted === "about-name"}>{text(fact ? FACT_MESSAGES[fact] : about?.anniversary ? "delight-anniversary" : "delight-name")}</AboutLine>
            </div>
            <Section id="about-updates" title={text("about-updates")}><Group>
              <Row id="automatic" label={text("about-automatic")} description={updates === null && !readProblems.has("automatic") ? text("about-unavailable") : text("about-automatic-description")} {...feedback("automatic")}>
                {(updates !== null || readProblems.has("automatic")) && <Switch aria-labelledby="automatic" aria-describedby="automatic-description" isSelected={updates ?? false}
                    isDisabled={updatesSaving || updates === null} onChange={(on) => {
                      setProblemRow(null); updateIntent.current = on; setUpdatesSaving(true); const operation = ++updateReads.current;
                      const saved = updates;
                      void api.setUpdateChecks(on).catch(() => {
                        if (live.current && operation === updateReads.current) { setUpdates(saved); setProblemRow({ id: "automatic", message: "settings-choice-failed" }); }
                      }).finally(() => { if (live.current) setUpdatesSaving(false); });
                    }} />}
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
