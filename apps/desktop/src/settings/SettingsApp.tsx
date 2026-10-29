import { useEffect, useMemo, useRef, useState } from "react";
import type { Density, LoginItem, PickerPreferences, ShortcutStatus } from "../bindings";
import { DEFAULT_PICKER_SHORTCUT, KEY_TABLE, keyLabel, shortcutLabel } from "../keys";
import { densityNames, keyText, loginItemText, settingsText, shortcutRefusalText } from "../messages";
import { PreferencePatchQueue } from "../preferencePatchQueue";
import { currentPlatform } from "../platform";
import { Button } from "../ui/Button";
import { Segmented } from "../ui/Segmented";
import { SettingsGroup as Group, SettingsRow as Row } from "../ui/SettingsGroup";
import { ShortcutRecorder } from "../ui/ShortcutRecorder";
import { SideNavigation } from "../ui/SideNavigation";
import { Switch } from "../ui/Switch";
import { PasteGuide, usePastePermission } from "./PasteGuide";
import type { SettingsApi } from "./api";
import "./Settings.css";

type Page = "general" | "shortcuts" | "about";
const PAGES: readonly Page[] = ["general", "shortcuts", "about"];
const DENSITIES: readonly Density[] = ["compact", "comfortable", "spacious"];

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
  const [shortcut, setShortcut] = useState<ShortcutStatus | null>(null);
  const [shortcutProblem, setShortcutProblem] = useState<string | null>(null);
  const [shortcutSaving, setShortcutSaving] = useState(false);
  const shortcutBusy = useRef(false);
  const shortcutReads = useRef(0);
  const updateReads = useRef(0);
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
    if (event.type === "error") setFailed(true);
    else if (event.type === "read") setPreferences(event.value.preferences);
    else if (event.type === "saved") setPreferences(event.value.preferences);
  }), [queue]);
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
          setShortcut(next);
          setShortcutProblem((problem) => problem === settingsText.shortcut.readFailed ? null : problem);
        }
      }).catch(() => {
        if (active && request === shortcutReads.current) setShortcutProblem(settingsText.shortcut.readFailed);
      });
    };
    const readAll = () => {
      void queue.load(() => active);
      api.loginItem().then(setLogin).catch(() => setLogin(null));
      const updateRequest = ++updateReads.current;
      api.updateChecks().then((next) => {
        if (active && updateRequest === updateReads.current) setUpdates(next);
      }).catch(() => {
        if (active && updateRequest === updateReads.current) setUpdates(null);
      });
    };
    readAll();
    api.version().then(setVersion).catch(() => undefined);
    // Changes made in the picker or the menu bar while Settings was open.
    window.addEventListener("focus", readAll);
    window.addEventListener("focus", readShortcut);
    const shortcuts = api.onShortcutStatus((next) => {
      shortcutReads.current += 1;
      if (active) setShortcut(next);
    });
    void shortcuts.catch(() => { if (active) setShortcutProblem(settingsText.shortcut.readFailed); }).then(() => { if (active) readShortcut(); });
    const page = api.onPage((name) => { const next = pageOf(name); setDetour(name === "paste"); setPage(next.page); setFocus(next.focus ?? "top"); });
    const automatic = api.onUpdateChecks((next) => {
      if (!active) return;
      updateReads.current += 1;
      setUpdates(next);
    });
    const close = api.onCloseRequested(() => queue.finishSaving());
    return () => {
      active = false;
      window.removeEventListener("focus", readAll);
      window.removeEventListener("focus", readShortcut);
      void shortcuts.then((stop) => stop()).catch(() => undefined);
      void page.then((stop) => stop()).catch(() => undefined);
      void automatic.then((stop) => stop()).catch(() => undefined);
      void close.then((stop) => stop()).catch(() => undefined);
    };
  }, [api, queue]);
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

  const change = (next: Partial<PickerPreferences>) => {
    if (!preferences) return;
    setPreferences({ ...preferences, ...next });
    setFailed(false);
    queue.change(next);
  };
  const changeShortcut = (next: string | null) => {
    if (shortcutBusy.current) return;
    shortcutBusy.current = true;
    const request = ++shortcutReads.current;
    setShortcutSaving(true);
    setShortcutProblem(null);
    void queue.enqueue(() => api.setShortcut(next))
      .then((status) => { if (request === shortcutReads.current) setShortcut(status); })
      .catch((refusal: unknown) => {
        setShortcutProblem(shortcutRefusalText(refusal));
      })
      .finally(() => { shortcutBusy.current = false; setShortcutSaving(false); });
  };
  const configureShortcut = () => {
    if (shortcutBusy.current) return;
    shortcutBusy.current = true;
    setShortcutSaving(true);
    setShortcutProblem(null);
    void api.configureShortcuts().catch(() => setShortcutProblem(settingsText.shortcut.configureFailed))
      .finally(() => { shortcutBusy.current = false; setShortcutSaving(false); });
  };
  const name = login?.name ?? "Ibis";
  const mac = platform === "macos";
  const defaultLabel = shortcutLabel(DEFAULT_PICKER_SHORTCUT, mac);
  const isDefault = shortcut && /^[a-z]$/i.test(shortcut.shortcut.split("+").at(-1)?.trim() ?? "")
    && shortcutLabel(shortcut.shortcut, mac).split("+").sort().join("+") === defaultLabel.split("+").sort().join("+");

  return (
    <div className="settings">
      <div className="settings-titlebar" data-tauri-drag-region />
      <nav className="settings-sidebar" aria-label={settingsText.pages}>
        <SideNavigation label={settingsText.pages} value={page} options={PAGES.map((value) => ({ value, label: settingsText[value] }))}
          onChange={(next) => { setDetour(false); setPage(next); setFocus("top"); }} />
      </nav>
      <main ref={pageRef} className="settings-page" aria-labelledby="settings-page-title">
        <h1 id="settings-page-title" className="settings-page-title">{settingsText[page]}</h1>
        {failed && <p className="settings-problem" role="alert">{settingsText.failed}</p>}
        {detour && returnFailed && <p className="settings-problem" role="alert">{settingsText.paste.returnFailed}</p>}
        {page === "general" && (
          <>
            {login && !(login.problem === "unsupported") && (
              <Group id="startup">
                <Row id="login" label={loginItemText.label(platform)} description={login.problem ? loginItemText.problem(login.problem, platform, name) : settingsText.login.description}>
                  <Switch aria-labelledby="login" aria-describedby="login-description" isSelected={login.enabled}
                    onChange={(on) => { api.setLoginItem(on).then(setLogin).catch(() => setFailed(true)); }} />
                </Row>
              </Group>
            )}
            {preferences && (
              <>
                {/* Only systems where Ibis pastes ask for a permission first. */}
                {permission?.pastes && (
                <Group id="paste" title={settingsText.groups.paste}>
                  <Row id="paste-label" label={settingsText.paste.label} description={settingsText.paste.description}>
                    <Switch aria-labelledby="paste-label" aria-describedby="paste-label-description" isSelected={preferences.paste}
                      onChange={(on) => change({ paste: on })} />
                  </Row>
                  {preferences.paste && permission?.needed && permission.granted && !guided && (
                    <Row id="paste-allowed" label={settingsText.paste.allowed(permission.list)} description={settingsText.paste.allowedNote}>
                      <DoneMark />
                    </Row>
                  )}
                  {preferences.paste && permission?.needed && (!permission.granted || guided) && (
                    <PasteGuide name={name} permission={permission} onOpen={() => { setGuided(true); void api.openPastePermission(); }} onDrag={api.dragPasteApp} />
                  )}
                </Group>
                )}
                <Group id="picker" title={settingsText.groups.picker}>
                  <Row id="rows" label={settingsText.picker.rows}>
                    <Segmented label={settingsText.picker.rows} value={preferences.density}
                      options={DENSITIES.map((density) => ({ value: density, label: densityNames[density] }))}
                      onChange={(density) => change({ density })} />
                  </Row>
                  <Row id="larger-text" label={settingsText.picker.largerText}>
                    <Switch aria-labelledby="larger-text" isSelected={preferences.larger_text} onChange={(on) => change({ larger_text: on })} />
                  </Row>
                  <Row id="preview" label={settingsText.picker.preview} description={settingsText.picker.previewDescription}>
                    <Switch aria-labelledby="preview" aria-describedby="preview-description" isSelected={preferences.show_paste_preview}
                      onChange={(on) => change({ show_paste_preview: on })} />
                  </Row>
                  <Row id="search-mark" label={settingsText.picker.searchMark} description={settingsText.picker.searchMarkDescription}>
                    <Switch aria-labelledby="search-mark" aria-describedby="search-mark-description" isSelected={preferences.search_mark}
                      onChange={(on) => change({ search_mark: on })} />
                  </Row>
                </Group>
              </>
            )}
          </>
        )}
        {page === "shortcuts" && (
          <>
            <Group id="open-ibis">
              <Row id="open-shortcut" label={settingsText.shortcut.open} description={shortcut?.desktop?.guidance ?? (shortcut ? settingsText.shortcut.description : undefined)}>
                {shortcut?.desktop ? <>
                  <span role="status">{shortcut.shortcut || settingsText.shortcut.notAssigned}</span>
                  {shortcut.desktop.configurable && <Button isDisabled={shortcutSaving} onPress={configureShortcut}>{settingsText.shortcut.change}</Button>}
                </> : shortcut && <ShortcutRecorder shortcut={shortcut.shortcut} label={settingsText.shortcut.open}
                  prompt={settingsText.shortcut.record} isDisabled={shortcutSaving} onRecord={changeShortcut} />}
              </Row>
              {shortcut && !shortcut.desktop && !isDefault && (
                <Row id="default-shortcut" label={defaultLabel}>
                  <Button isDisabled={shortcutSaving} onPress={() => changeShortcut(null)}>{settingsText.shortcut.useDefault}</Button>
                </Row>
              )}
            </Group>
            {shortcutProblem && <p className="settings-problem" role="alert">{shortcutProblem}</p>}
            {shortcut?.error && <p className="settings-problem" role="status">{shortcut.error}</p>}
            <Group id="picker-shortcuts" title={settingsText.shortcut.inPicker}>
              {KEY_TABLE.map((binding) => (
                <Row key={binding.action} id={`key-${binding.action}`} label={keyText.actions[binding.action]} description={keyText.regions[binding.region]}>
                  <kbd>{binding.keys.map((keys) => keyLabel(keys, mac)).join(` ${keyText.or} `)}</kbd>
                </Row>
              ))}
            </Group>
          </>
        )}
        {page === "about" && (
          <>
            <Group id="version">
              <Row id="version-label" label={settingsText.updates.version(name, version)}
                description={updates === null ? settingsText.updates.unavailable : undefined}>
                {updates !== null && <Button onPress={() => { void api.checkForUpdates(); }}>{settingsText.updates.check}</Button>}
              </Row>
            </Group>
            {updates !== null && (
              <Group id="updates" title={settingsText.groups.updates}>
                <Row id="automatic" label={settingsText.updates.automatic} description={settingsText.updates.automaticDescription}>
                  <Switch aria-labelledby="automatic" aria-describedby="automatic-description" isSelected={updates}
                    onChange={(on) => { setUpdates(on); void api.setUpdateChecks(on); }} />
                </Row>
              </Group>
            )}
          </>
        )}
      </main>
    </div>
  );
}
