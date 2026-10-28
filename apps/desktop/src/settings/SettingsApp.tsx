import { useEffect, useRef, useState, type ReactNode } from "react";
import type { Density, LoginItem, PickerPreferences } from "../bindings";
import { densityNames, loginItemText, settingsText } from "../messages";
import { currentPlatform } from "../platform";
import { Button } from "../ui/Button";
import { Segmented } from "../ui/Segmented";
import { Switch } from "../ui/Switch";
import { PasteGuide, usePastePermission } from "./PasteGuide";
import type { SettingsApi } from "./api";
import "./Settings.css";

type Page = "general" | "about";
const PAGES: readonly Page[] = ["general", "about"];
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

/** One row: a label and optional description, then one control at the end. */
function Row({ id, label, description, children }: { id: string; label: string; description?: string; children?: ReactNode }) {
  return (
    <div className="settings-row" data-described={description ? true : undefined}>
      <div className="settings-row-text">
        <span id={id} className="settings-label">{label}</span>
        {description && <span id={`${id}-description`} className="settings-description">{description}</span>}
      </div>
      {children}
    </div>
  );
}

function Group({ id, title, children }: { id?: string; title?: string; children: ReactNode }) {
  return (
    <section className="settings-group" id={id} aria-labelledby={title ? `${id}-title` : undefined}>
      {title && <h2 id={`${id}-title`} className="settings-group-title">{title}</h2>}
      <div className="settings-card">{children}</div>
    </section>
  );
}

/** The Settings window: a sidebar of pages, each a stack of titled groups of
 * rows (docs/design.md, "Settings"). Every change applies at once. */
export function SettingsApp({ api, initialPage }: { api: SettingsApi; initialPage: string | null }) {
  const platform = currentPlatform();
  const start = pageOf(initialPage);
  const [page, setPage] = useState<Page>(start.page);
  const [focus, setFocus] = useState(start.focus);
  const [preferences, setPreferences] = useState<PickerPreferences | null>(null);
  const [login, setLogin] = useState<LoginItem | null>(null);
  const [updates, setUpdates] = useState<boolean | null>(null);
  const [version, setVersion] = useState("");
  const [failed, setFailed] = useState(false);
  /** The guide stays to confirm a permission given while it was open. */
  const [guided, setGuided] = useState(false);
  const saving = useRef<Promise<unknown>>(Promise.resolve());
  const generation = useRef(0);
  const pending = useRef(0);
  const permission = usePastePermission(api, page === "general");

  useEffect(() => {
    const readAll = () => {
      const started = generation.current;
      const idle = pending.current === 0;
      api.pickerPreferences().then((next) => {
        if (idle && started === generation.current && pending.current === 0) setPreferences(next);
      }).catch(() => setFailed(true));
      api.loginItem().then(setLogin).catch(() => setLogin(null));
      api.updateChecks().then(setUpdates).catch(() => setUpdates(null));
    };
    readAll();
    api.version().then(setVersion).catch(() => undefined);
    // Changes made in the picker or the menu bar while Settings was open.
    window.addEventListener("focus", readAll);
    const page = api.onPage((name) => { const next = pageOf(name); setPage(next.page); setFocus(next.focus); });
    const automatic = api.onUpdateChecks(setUpdates);
    const close = api.onCloseRequested(async () => {
      // Give queued choices time to save; a stuck command must not trap the window.
      let timer: number | undefined;
      const drain = async () => { while (pending.current) await saving.current; };
      await Promise.race([
        drain().catch(() => undefined),
        new Promise<void>((resolve) => { timer = window.setTimeout(resolve, 2_000); }),
      ]);
      window.clearTimeout(timer);
    });
    return () => {
      window.removeEventListener("focus", readAll);
      void page.then((stop) => stop()).catch(() => undefined);
      void automatic.then((stop) => stop()).catch(() => undefined);
      void close.then((stop) => stop()).catch(() => undefined);
    };
  }, [api]);
  useEffect(() => {
    if (focus !== "paste" || !preferences) return;
    document.getElementById("paste")?.scrollIntoView?.({ block: "start" });
    setFocus(null);
  }, [focus, preferences]);

  const change = (next: Partial<PickerPreferences>) => {
    if (!preferences) return;
    const chosen = { ...preferences, ...next };
    const edited = ++generation.current;
    pending.current += 1;
    setPreferences(chosen);
    setFailed(false);
    // Saved in the order chosen, so quick changes end as the last one shows.
    saving.current = saving.current.catch(() => undefined)
      .then(() => api.setPickerPreferences(chosen, null))
      .finally(() => { pending.current -= 1; })
      .catch(() => {
        setFailed(true);
        if (pending.current !== 0 || edited !== generation.current) return;
        void api.pickerPreferences().then((saved) => {
          if (pending.current === 0 && edited === generation.current) setPreferences(saved);
        }).catch(() => undefined);
      });
  };
  const name = login?.name ?? "Ibis";

  return (
    <div className="settings">
      <nav className="settings-sidebar" aria-label={settingsText.pages}>
        <div className="settings-titlebar" data-tauri-drag-region />
        {PAGES.map((each) => (
          <button key={each} type="button" className="settings-nav" aria-current={each === page ? "page" : undefined}
            onClick={() => setPage(each)}>{settingsText[each]}</button>
        ))}
      </nav>
      <main className="settings-page" aria-labelledby="settings-page-title">
        <div className="settings-titlebar" data-tauri-drag-region />
        <h1 id="settings-page-title" className="settings-page-title">{settingsText[page]}</h1>
        {failed && <p className="settings-problem" role="alert">{settingsText.failed}</p>}
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
                    <PasteGuide name={name} permission={permission} onOpen={() => { setGuided(true); void api.openPastePermission(); }} />
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
