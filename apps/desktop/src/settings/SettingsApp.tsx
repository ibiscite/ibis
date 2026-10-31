import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { KeyFormat, LoginItem, LoginItemProblem, PickerPreferences, ShortcutStatus } from "../bindings";
import { DEFAULT_PICKER_SHORTCUT, KEY_TABLE, keyLabel, shortcutLabel } from "../keys";
import { type MessageId, useInterfaceMessages } from "../i18n";
import { SHORTCUT_ACTIONS, SHORTCUT_COMMANDS, SHORTCUT_REGIONS, type ShortcutProblem, shortcutRefusalMessage } from "../i18n/settings";
import { PreferencePatchQueue } from "../preferencePatchQueue";
import { currentPlatform } from "../platform";
import { Button } from "../ui/Button";
import { SettingsGroup as Group, SettingsRow as Row } from "../ui/SettingsGroup";
import { ShortcutRecorder } from "../ui/ShortcutRecorder";
import { SideNavigation } from "../ui/SideNavigation";
import { useAppearance } from "../ui/appearance";
import { DefaultFormat } from "./DefaultFormat";
import { Switch } from "../ui/Switch";
import { PasteGuide, usePastePermission } from "./PasteGuide";
import type { SettingsApi } from "./api";
import "../ui/SettingsLayout.css";
import "./Settings.css";

import { PAGES, ROWS, rowDetails, type Page } from "./rows";
import { LookPage } from "./LookPage";
const LOGIN_PROBLEMS = {
  unsupported: "general-login-unsupported", needs_approval: "general-login-needs-approval",
  refused: "general-login-refused", unreadable: "general-login-unreadable",
} as const satisfies Record<LoginItemProblem, MessageId>;

/** A page asked for by name; "paste" is General, scrolled to pasting. */
export function pageOf(name: string | null): { page: Page; focus: "paste" | null } {
  if (name === "paste") return { page: "general", focus: "paste" };
  return { page: PAGES.includes(name as Page) ? name as Page : "general", focus: null };
}

/** The done mark: the status colour with a tick, as in the Copied notice. */
function DoneMark() {
  return <svg className="settings-done" viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="10" /><path d="m5.5 10.5 3 3L14.5 7" /></svg>;
}

/** The Settings window: a sidebar of pages, each a stack of titled groups of
 * rows (docs/design.md, "Settings"). Every change applies at once. */
export function SettingsApp({ api, initialPage }: { api: SettingsApi; initialPage: string | null }) {
  const { text } = useInterfaceMessages();
  const platform = currentPlatform();
  const start = pageOf(initialPage);
  const [page, setPage] = useState<Page>(start.page);
  const [focus, setFocus] = useState<"top" | "paste" | null>(start.focus ?? "top");
  const pageRef = useRef<HTMLElement>(null);
  const [preferences, setPreferences] = useState<PickerPreferences | null>(null);
  const [login, setLogin] = useState<LoginItem | null>(null);
  const [updates, setUpdates] = useState<boolean | null>(null);
  const [version, setVersion] = useState("");
  const [failed, setFailed] = useState(false);
  const preferenceProblem = useRef(false);
  const [readProblems, setReadProblems] = useState<ReadonlySet<string>>(() => new Set());
  const readProblem = useCallback((id: string, failed: boolean) => setReadProblems((current) => {
    const next = new Set(current);
    if (failed) next.add(id); else next.delete(id);
    return next;
  }), []);
  const [problemRow, setProblemRow] = useState<{ id: string; message: MessageId } | null>(null);
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
  const latestShortcut = useRef<ShortcutStatus | null>(null);
  const [shortcutSaving, setShortcutSaving] = useState(false);
  const shortcutBusy = useRef(false);
  const shortcutReads = useRef(0);
  const updateReads = useRef(0);
  const loginReads = useRef(0);
  const loginBusy = useRef(false);
  const [loginSaving, setLoginSaving] = useState(false);
  const [updatesSaving, setUpdatesSaving] = useState(false);
  const updateIntent = useRef<boolean | null>(null);
  /** The guide stays to confirm a permission given while it was open. */
  const [guided, setGuided] = useState(false);
  const [detour, setDetour] = useState(initialPage === "paste");
  const [returnFailed, setReturnFailed] = useState(false);
  const [focused, setFocused] = useState(true);
  const queue = useMemo(() => new PreferencePatchQueue({
    read: async () => ({ preferences: await api.pickerPreferences() }),
    write: (patch, key) => api.setPickerPreferences(patch, key),
  }, { restoreOnFailure: true }), [api]);
  useEffect(() => queue.subscribe((event) => {
    if (!live.current) return;
    if (event.type === "error") {
      if (event.source === "read" && !preferenceProblem.current) setFailed(true);
    } else if (event.type === "read" || event.type === "saved") {
      setPreferences(event.value.preferences);
      setFailed(false);
    } else if (event.type === "saving") {
      const operation = edits.current;
      const row = cause.current;
      void event.promise.catch((reason: unknown) => {
        if (live.current && operation === edits.current) {
          preferenceProblem.current = true;
          setFailed(false);
          if (row) setProblemRow({ id: row, message: reason === "settings-read-only" ? "settings-read-only" : "settings-choice-failed" });
          setPreferences(queue.savedPreferences);
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
  const permission = usePastePermission(api, page === "general", preferences?.paste === true);

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
          if (latestShortcut.current?.shortcut !== next.shortcut) setShortcutUndo(null);
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
    const preferenceEvents = api.onPreferencesChanged(() => {
      void queue.refresh(() => active);
    });
    readAll(false);
    void preferenceEvents.then(() => { if (active) void queue.load(() => active); }).catch(() => { if (active) setFailed(true); });
    api.version().then((next) => { if (active) setVersion(next); }).catch(() => undefined);
    // Changes made in the picker or the menu bar while Settings was open.
    window.addEventListener("focus", readAll);
    window.addEventListener("focus", readShortcut);
    const shortcuts = api.onShortcutStatus((next) => {
      shortcutReads.current += 1;
      if (active) { latestShortcut.current = next; setShortcut(next); setShortcutUndo(null); }
    });
    void shortcuts.catch(() => { if (active) setShortcutProblem({ id: "shortcuts-read-failed" }); }).then(() => { if (active) readShortcut(); });
    const page = api.onPage((name) => { const next = pageOf(name); setDetour(name === "paste"); setPage(next.page); setFocus(next.focus ?? "top"); });
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
  }, [api, queue, readProblem]);
  useEffect(() => {
    const scroller = pageRef.current;
    if (!focus || !scroller) return;
    if (focus === "paste") {
      // Wait for both reads before looking for the group. Move only the page,
      // never the document containing the sidebar and native title bar.
      if (!preferences || !permission) return;
      const paste = document.getElementById("paste");
      scroller.scrollTop = paste ? scroller.scrollTop + paste.getBoundingClientRect().top - scroller.getBoundingClientRect().top : 0;
    } else scroller.scrollTop = 0;
    setFocus(null);
  }, [focus, preferences, permission]);

  const change = (next: Partial<PickerPreferences>, key: KeyFormat | null = null) => {
    if (!preferences) return;
    edits.current += 1;
    cause.current = Object.hasOwn(next, "citation_style") || key !== null ? "default-format" : next.appearance !== undefined ? "appearance" : next.accent !== undefined ? "accent"
      : next.density !== undefined ? "rows" : next.larger_text !== undefined ? "text-size"
      : next.search_mark !== undefined ? "search-icon" : next.paste !== undefined ? "paste-label" : "preview";
    setProblemRow(null);
    preferenceProblem.current = false;
    setPreferences({ ...preferences, ...next, ...(key ? { copy_format: key } : {}) });
    setFailed(false);
    queue.change(next, key);
  };
  const changeShortcut = (next: string | null) => {
    if (shortcutBusy.current) return;
    shortcutBusy.current = true;
    const previous = shortcut?.shortcut;
    setShortcutUndo(null);
    const request = ++shortcutReads.current;
    setShortcutSaving(true);
    setShortcutProblem(null);
    void queue.enqueue(() => api.setShortcut(next))
      .then((status) => {
        if (!live.current) return;
        if (request === shortcutReads.current) { latestShortcut.current = status; setShortcut(status); }
        if (next === null && previous && latestShortcut.current?.shortcut === status.shortcut) setShortcutUndo(previous);
      })
      .catch((refusal: unknown) => {
        setShortcutProblem(shortcutRefusalMessage(refusal));
      })
      .finally(() => { shortcutBusy.current = false; setShortcutSaving(false); });
  };
  useEffect(() => {
    if (!shortcutUndo) return;
    const expiry = window.setTimeout(() => setShortcutUndo(null), 8000);
    return () => window.clearTimeout(expiry);
  }, [shortcutUndo]);
  const configureShortcut = () => {
    if (shortcutBusy.current) return;
    shortcutBusy.current = true;
    setShortcutSaving(true);
    setShortcutProblem(null);
    void api.configureShortcuts().catch(() => setShortcutProblem({ id: "shortcuts-configure-failed" }))
      .finally(() => { shortcutBusy.current = false; setShortcutSaving(false); });
  };
  const feedback = (id: string) => ({ highlighted: highlighted === id, problem: readProblems.has(id) ? text("settings-read-failed") : problemRow?.id === id ? text(problemRow.message) : undefined });
  const selectPage = (next: Page) => { highlightOperation.current += 1; setHighlighted(null); setDetour(false); setPage(next); setFocus("top"); };
  const reveal = (row: typeof ROWS[number]) => {
    selectPage(row.page); setQuery(""); setHighlighted(row.id);
    const operation = ++highlightOperation.current;
    window.setTimeout(() => { if (live.current && operation === highlightOperation.current) setHighlighted(null); }, 2000);
  };
  useEffect(() => {
    if (!highlighted || !preferences || !pageRef.current) return;
    const target = document.getElementById(highlighted === "open-shortcut" && page === "general" ? "general-open-shortcut" : highlighted)?.closest(".ui-settings-row");
    if (target) pageRef.current.scrollTop += target.getBoundingClientRect().top - pageRef.current.getBoundingClientRect().top;
  }, [highlighted, page, preferences]);
  const unavailable = (id: string) => {
    return <Row key={id} id={id} {...rowDetails(id, text)} {...feedback(id)}>
      <Button isDisabled aria-labelledby={id} aria-describedby={`${id}-description`}>{text("settings-coming-soon")}</Button>
    </Row>;
  };
  const matches = query.trim() ? ROWS.filter((row) => [row.label, row.description, row.keywords].some((id) => id && text(id, { platform }).toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))) : [];
  const name = login?.name ?? "Ibis";
  const mac = platform === "macos";
  const defaultLabel = shortcutLabel(DEFAULT_PICKER_SHORTCUT, mac);
  const isDefault = shortcut && /^[a-z]$/i.test(shortcut.shortcut.split("+").at(-1)?.trim() ?? "")
    && shortcutLabel(shortcut.shortcut, mac).split("+").sort().join("+") === defaultLabel.split("+").sort().join("+");

  return (
    <div className="settings">
      <div className="settings-titlebar" data-tauri-drag-region />
      <nav className="settings-sidebar" aria-label={text("settings-pages")}>
        <div className="ui-settings-search">
          <input type="search" aria-label={text("settings-search")} placeholder={text("settings-search")} value={query} onChange={(event) => setQuery(event.target.value)} />
        </div>
        {query.trim() ? <div className="ui-side-navigation ui-settings-search-results" aria-live="polite">
          {matches.length ? matches.map((row) => <Button key={row.id} onPress={() => reveal(row)}>{text(row.label, { platform })}</Button>) : <p>{text("settings-no-results")}</p>}
        </div> : <SideNavigation label={text("settings-pages")} value={page} options={PAGES.map((value) => ({ value, label: text(`settings-${value}`), gapBefore: value === "advanced" }))}
          onChange={selectPage} />}
      </nav>
      <main ref={pageRef} className="settings-page" aria-labelledby="settings-page-title">
        <h1 id="settings-page-title" className="settings-page-title">{text(`settings-${page}`)}</h1>
        {failed && <p className="settings-problem" role="alert">{text("settings-read-failed")}</p>}
        {detour && returnFailed && <p className="settings-problem" role="alert">{text("general-paste-return-failed")}</p>}
        {page === "general" && (
          <>
            <Group id="general-shortcut">
              <Row id="general-open-shortcut" label={text("shortcuts-open")} description={shortcut?.desktop?.guidance ?? (shortcut?.shortcut ? shortcutLabel(shortcut.shortcut, mac) : text("shortcuts-description"))} {...feedback("open-shortcut")}>
                {shortcut?.desktop && <span role="status">{shortcut.shortcut || text("shortcuts-not-assigned")}</span>}
                <Button onPress={() => selectPage("shortcuts")}>{text("shortcuts-change")}</Button>
              </Row>
            </Group>
            <Group id="startup">
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
            </Group>
            <Group id="menu-bar-group">
              <Row id="menu-bar" {...rowDetails("menu-bar", text)} {...feedback("menu-bar")}>
                <Switch aria-labelledby="menu-bar" aria-describedby="menu-bar-description" isSelected isDisabled />
              </Row>
            </Group>
            {preferences && (
              <>
                {/* Only systems where Ibis pastes ask for a permission first. */}
                {permission?.pastes && (
                <Group id="paste" title={text("general-paste-title")}>
                  <Row id="paste-label" label={text("general-paste-label")} description={text("general-paste-description")} {...feedback("paste-label")}>
                    <Switch aria-labelledby="paste-label" aria-describedby="paste-label-description" isSelected={preferences.paste}
                      onChange={(on) => change({ paste: on })} />
                  </Row>
                  {preferences.paste && permission?.needed && permission.granted && !guided && (
                    <Row id="paste-allowed" label={text("general-paste-allowed", { list: permission.list })} description={text("general-paste-allowed-note")}>
                      <DoneMark />
                    </Row>
                  )}
                  {preferences.paste && permission?.needed && (!permission.granted || guided) && (
                    <PasteGuide name={name} permission={permission} onOpen={() => { setGuided(true); void api.openPastePermission().then(() => readProblem("paste-label", false)).catch(() => readProblem("paste-label", true)); }} onDrag={api.dragPasteApp} />
                  )}
                </Group>
                )}
              </>
            )}
            <Group id="welcome-group">{unavailable("welcome")}</Group>
          </>
        )}
        {page === "look" && preferences && <LookPage preferences={preferences} change={change} feedback={feedback} />}
        {(page === "libraries" || page === "advanced") && <Group id={`${page}-choices`}>
          {ROWS.filter((row) => row.page === page).map((row) => unavailable(row.id))}
        </Group>}
        {page === "citations" && <Group id="citation-choices">
          {preferences && <Row id="default-format" {...rowDetails("default-format", text)} {...feedback("default-format")}>
            <DefaultFormat preferences={preferences} change={change} />
          </Row>}
          {ROWS.filter((row) => row.page === "citations" && row.id !== "default-format").map((row) => unavailable(row.id))}
        </Group>}
        {page === "shortcuts" && (
          <>
            <Group id="open-ibis">
              <Row id="open-shortcut" label={text("shortcuts-open")} description={shortcut?.desktop?.guidance ?? (shortcut ? text("shortcuts-description") : undefined)}>
                {shortcut?.desktop ? <>
                  <span role="status">{shortcut.shortcut || text("shortcuts-not-assigned")}</span>
                  {shortcut.desktop.configurable && <Button isDisabled={shortcutSaving} onPress={configureShortcut}>{text("shortcuts-change")}</Button>}
                </> : shortcut && <ShortcutRecorder shortcut={shortcut.shortcut} label={text("shortcuts-open")}
                  prompt={text("shortcuts-record")} isDisabled={shortcutSaving} onRecord={changeShortcut} />}
              </Row>
              {shortcut && !shortcut.desktop && !isDefault && (
                <Row id="default-shortcut" label={defaultLabel}>
                  <Button isDisabled={shortcutSaving} onPress={() => changeShortcut(null)}>{text("shortcuts-use-default")}</Button>
                </Row>
              )}
            </Group>
            {shortcutUndo && <Group id="shortcut-undo-group"><Row id="shortcut-undo" label={text("settings-shortcut-restored")}>
              <Button isDisabled={shortcutSaving} onPress={() => changeShortcut(shortcutUndo)}>{text("settings-undo")}</Button>
            </Row></Group>}
            {shortcutProblem && <p className="settings-problem" role="alert">{text(shortcutProblem.id,
              shortcutProblem.command ? { command: text(SHORTCUT_COMMANDS[shortcutProblem.command]) } : undefined)}</p>}
            {shortcut?.error && <p className="settings-problem" role="status">{shortcut.error}</p>}
            <Group id="shortcut-list-choice">{unavailable("choose-shortcut")}</Group>
            <details className="ui-settings-key-list"><summary>{text("shortcuts-in-picker")}</summary>
            <Group id="picker-shortcuts" title={text("shortcuts-in-picker")}>
              {KEY_TABLE.map((binding) => (
                <Row key={binding.action} id={`key-${binding.action}`} label={text(SHORTCUT_ACTIONS[binding.action])} description={text(SHORTCUT_REGIONS[binding.region])}>
                  <kbd>{binding.keys.map((keys) => keyLabel(keys, mac)).join(` ${text("shortcuts-or")} `)}</kbd>
                </Row>
              ))}
            </Group>
            </details>
          </>
        )}
        {page === "about" && (
          <>
            <Group id="version">
              <Row id="version-label" label={text("about-version", { name, version })}
                description={updates === null && !readProblems.has("automatic") ? text("about-unavailable") : undefined} {...feedback("version-label")}>
                {updates !== null && <Button onPress={() => { void api.checkForUpdates().then(() => readProblem("version-label", false)).catch(() => readProblem("version-label", true)); }}>{text("about-check")}</Button>}
              </Row>
            </Group>
            {(updates !== null || readProblems.has("automatic")) && (
              <Group id="updates" title={text("about-updates")}>
                <Row id="automatic" label={text("about-automatic")} description={text("about-automatic-description")} {...feedback("automatic")}>
                  <Switch aria-labelledby="automatic" aria-describedby="automatic-description" isSelected={updates ?? false}
                    isDisabled={updatesSaving || updates === null} onChange={(on) => {
                      setProblemRow(null); updateIntent.current = on; setUpdatesSaving(true); const operation = ++updateReads.current;
                      const saved = updates;
                      void api.setUpdateChecks(on).catch(() => {
                        if (live.current && operation === updateReads.current) { setUpdates(saved); setProblemRow({ id: "automatic", message: "settings-choice-failed" }); }
                      }).finally(() => { if (live.current) setUpdatesSaving(false); });
                    }} />
                </Row>
              </Group>
            )}
            <Group id="about-help">{["report-problem", "suggest-idea", "licences"].map(unavailable)}</Group>
          </>
        )}
      </main>
    </div>
  );
}
