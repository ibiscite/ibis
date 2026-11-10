import { useEffect, useState } from "react";
import { Button } from "../ui/Button";
import { FileDrag } from "../ui/FileDrag";
import "../ui/PasteGuide.css";
import { useInterfaceMessages } from "../i18n";
import type { PastePermission, SettingsApi } from "./api";
import appIcon from "../../src-tauri/icons/128x128.png";

/** How often to look again while waiting for the person in System Settings. */
export const RECHECK_MS = 1_000;
/** After this long without the permission, say how to clear a stale switch. */
export const STALE_HINT_MS = 15_000;

/** A failed read never leaves an old "Allowed" in place. */
export type ReadPermission = PastePermission & { unreadable?: boolean };

/** Whether Ibis may paste, read live while the guide waits (#1292). Coming
 * back to the window reads it at once, so the answer is never stale. */
export function usePastePermission(api: Pick<SettingsApi, "pastePermission">, active: boolean, poll = true) {
  const [permission, setPermission] = useState<ReadPermission | null>(null);
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
      void api.pastePermission().then((next) => { if (live && current === request) setPermission(next); })
        .catch(() => { if (live && current === request) setPermission((old) => old && { ...old, granted: false, unreadable: true }); });
    };
    void read();
    const timer = waiting ? window.setInterval(read, RECHECK_MS) : undefined;
    window.addEventListener("focus", read);
    return () => { live = false; window.clearInterval(timer); window.removeEventListener("focus", read); };
  }, [api, active, visible, waiting]);
  return permission;
}

/** The steps to allow pasting: what macOS asks, where the switch is, and a
 * live answer once both grants are on. Only a deliberate Open action navigates
 * to System Settings; permission reads never request a grant. */
export function PasteGuide({ name, permission, onOpen, onDrag }: {
  name: string; permission: ReadPermission; onOpen: () => void; onDrag: () => Promise<void>;
}) {
  const { text } = useInterfaceMessages();
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
    <div className="ui-paste-guide" data-granted={permission.granted || undefined}>
      <div className="ui-paste-guide-art" aria-hidden="true">
        <div className="ui-paste-guide-row">
          <span className="ui-paste-guide-icon" style={{ backgroundImage: `url(${appIcon})` }} />
          <span className="ui-paste-guide-name">{name}</span>
          <span className="ui-paste-guide-switch" data-on={permission.granted || undefined}><span /></span>
        </div>
        <div className="ui-paste-guide-pane">{text("general-paste-pane", { list: permission.list })}</div>
      </div>
      <div className="ui-paste-guide-text">
        <h3>{text("general-paste-guide-title")}</h3>
        <p className="ui-paste-guide-note">{text("general-paste-guide-why")}</p>
        <ol>{(["general-paste-step-open", "general-paste-step-allow", "general-paste-step-return"] as const)
          .map((id) => <li key={id}>{text(id, { name, list: permission.list })}</li>)}</ol>
        <div className="ui-paste-guide-actions">
          {!permission.granted && <>
            <Button variant="primary" onPress={open}>{text("general-paste-open")}</Button>
            <FileDrag icon={appIcon} label={text("general-paste-drag", { name, list: permission.list })} description={text("general-paste-drag-keyboard")} onPress={open}
              onDrag={() => { setDragFailed(false); void onDrag().catch(() => setDragFailed(true)); }} />
          </>}
          <p className="ui-paste-guide-status" role="status" aria-live="polite" data-done={permission.granted || undefined}>
            {permission.granted ? text("general-paste-granted") : permission.unreadable ? text("general-paste-unreadable") : opened ? text("general-paste-waiting") : ""}
          </p>
        </div>
        {!permission.granted && <p className="ui-paste-guide-note">{text("general-paste-drag-fallback")}</p>}
        {dragFailed && !permission.granted && <p className="ui-paste-guide-note" data-problem role="alert">{text("general-paste-drag-failed")}</p>}
        {slow && !permission.granted && <p className="ui-paste-guide-note">{text("general-paste-stale", { name })}</p>}
      </div>
    </div>
  );
}
