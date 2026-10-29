import { useEffect, useState } from "react";
import { Button } from "../ui/Button";
import { FileDrag } from "../ui/FileDrag";
import { settingsText } from "../messages";
import type { PastePermission, SettingsApi } from "./api";
import appIcon from "../../src-tauri/icons/128x128.png";

const text = settingsText.paste;
/** How often to look again while waiting for the person in System Settings. */
export const RECHECK_MS = 1_000;
/** After this long without the permission, say how to clear a stale switch. */
export const STALE_HINT_MS = 15_000;

/** Whether Ibis may paste, read live while the guide waits (#1292). Coming
 * back to the window reads it at once, so the answer is never stale. */
export function usePastePermission(api: Pick<SettingsApi, "pastePermission">, active: boolean, poll = true) {
  const [permission, setPermission] = useState<PastePermission | null>(null);
  const [visible, setVisible] = useState(!document.hidden);
  useEffect(() => {
    const changed = () => setVisible(!document.hidden);
    document.addEventListener("visibilitychange", changed);
    return () => document.removeEventListener("visibilitychange", changed);
  }, []);
  // Poll only while waiting for the switch; focus covers later changes.
  const waiting = poll && permission?.needed === true && !permission.granted;
  useEffect(() => {
    if (!active || !visible) return;
    let live = true;
    let request = 0;
    const read = () => {
      if (document.hidden) return;
      const current = ++request;
      void api.pastePermission().then((next) => { if (live && current === request) setPermission(next); }).catch(() => undefined);
    };
    void read();
    const timer = waiting ? window.setInterval(read, RECHECK_MS) : undefined;
    window.addEventListener("focus", read);
    return () => { live = false; window.clearInterval(timer); window.removeEventListener("focus", read); };
  }, [api, active, visible, waiting]);
  return permission;
}

/** The steps to allow pasting: what macOS asks, where the switch is, and a
 * live answer once it is on. Clicking Open clears Ibis's stale switch first. */
export function PasteGuide({ name, permission, onOpen, onDrag }: {
  name: string; permission: PastePermission; onOpen: () => void; onDrag: () => Promise<void>;
}) {
  const [opened, setOpened] = useState(false);
  const [slow, setSlow] = useState(false);
  const [dragFailed, setDragFailed] = useState(false);
  const open = () => { setOpened(true); setSlow(false); setDragFailed(false); onOpen(); };
  useEffect(() => {
    if (!opened || permission.granted) return;
    const timer = window.setTimeout(() => setSlow(true), STALE_HINT_MS);
    return () => window.clearTimeout(timer);
  }, [opened, permission.granted]);
  return (
    <div className="paste-guide" data-granted={permission.granted || undefined}>
      <div className="paste-guide-art" aria-hidden="true">
        <div className="paste-guide-row">
          <span className="paste-guide-icon" style={{ backgroundImage: `url(${appIcon})` }} />
          <span className="paste-guide-name">{name}</span>
          <span className="paste-guide-switch" data-on={permission.granted || undefined}><span /></span>
        </div>
        <div className="paste-guide-pane">{`Privacy & Security › ${permission.list}`}</div>
      </div>
      <div className="paste-guide-text">
        <h3>{text.guideTitle}</h3>
        <p className="settings-description">{text.guideWhy}</p>
        <ol>{text.steps(name, permission.list).map((step) => <li key={step}>{step}</li>)}</ol>
        <div className="paste-guide-actions">
          {!permission.granted && <>
            <Button variant="primary" onPress={open}>{text.open}</Button>
            <FileDrag icon={appIcon} label={text.drag(name, permission.list)} description={text.dragKeyboard} onPress={open}
              onDrag={() => { setDragFailed(false); void onDrag().catch(() => setDragFailed(true)); }} />
          </>}
          <p className="paste-guide-status" role="status" aria-live="polite" data-done={permission.granted || undefined}>
            {permission.granted ? text.granted : opened ? text.waiting : ""}
          </p>
        </div>
        {!permission.granted && <p className="settings-description">{text.dragFallback}</p>}
        {dragFailed && !permission.granted && <p className="settings-problem" role="alert">{text.dragFailed}</p>}
        {slow && !permission.granted && <p className="settings-description">{text.stale(name)}</p>}
      </div>
    </div>
  );
}
