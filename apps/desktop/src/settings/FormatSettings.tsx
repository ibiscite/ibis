import { useEffect, useRef, useState } from "react";
import type { FormatSettingsView } from "../bindings";
import { outputName, listedOutput } from "../format/outputs";
import { useInterfaceMessages } from "../i18n";
import { Button } from "../ui/Button";
import { Group, Row, Section } from "../ui/Layout";
import type { SettingsApi } from "./api";
import { rowDetails } from "./rows";

/** App choices and writings come from the engine's settings snapshot. A late
 * read or forget never replaces a more recent snapshot or a reopened page. */
export function useFormatSettings(api: SettingsApi) {
  const [view, setView] = useState<FormatSettingsView | null>(null);
  const [problem, setProblem] = useState(false);
  const [forgetFailed, setForgetFailed] = useState(false);
  const [openFailed, setOpenFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const request = useRef(0);
  const openRequest = useRef(0);
  const refresh = useRef<(() => Promise<void>) | null>(null);
  useEffect(() => {
    if (!api.formatSettings) return () => { openRequest.current += 1; };
    setBusy(false);
    let alive = true;
    const read = () => {
      const ticket = ++request.current;
      return api.formatSettings!().then((next) => {
        if (!alive || ticket !== request.current) return;
        setView(next); setProblem(false);
      }, () => { if (alive && ticket === request.current) setProblem(true); });
    };
    refresh.current = read;
    const events = api.onSettingsChanged(read);
    void events.then(read, read);
    window.addEventListener("focus", read);
    return () => { alive = false; openRequest.current += 1; refresh.current = null; request.current += 1; window.removeEventListener("focus", read); void events.then((stop) => stop(), () => {}); };
  }, [api]);
  const forget = (id: string) => {
    if (!api.forgetAppFormat || !api.formatSettings || busy) return;
    const read = refresh.current;
    ++request.current;
    setBusy(true);
    void api.forgetAppFormat(id).then(() => {
      if (refresh.current === read) { setForgetFailed(false); return read?.(); }
    }, () => { if (refresh.current === read) setForgetFailed(true); }).finally(() => {
      if (refresh.current === read) setBusy(false);
    });
  };
  const openSettingsFile = api.openSettingsFile ? () => {
    const ticket = ++openRequest.current;
    void api.openSettingsFile!().then(() => { if (ticket === openRequest.current) setOpenFailed(false); },
      () => { if (ticket === openRequest.current) setOpenFailed(true); });
  } : undefined;
  return { view, problem, busy, forget, openSettingsFile, forgetFailed, openFailed,
    dismissForget: () => setForgetFailed(false), dismissOpen: () => setOpenFailed(false) };
}

export function FormatSettings({ settings, highlighted }: { settings: ReturnType<typeof useFormatSettings>; highlighted: string | null }) {
  const { text } = useInterfaceMessages();
  const { view, problem, busy, forget, openSettingsFile, forgetFailed, openFailed, dismissForget, dismissOpen } = settings;
  return <>
    {view && view.apps.length > 0 && <Section id="remembered-formats" title={text("citations-apps")}><Group>
      {view.apps.map((app) => <Row key={app.id} id={`format-${app.id}`} label={app.app} description={listedOutput(app.format) ? outputName(listedOutput(app.format)!) : text("citations-unlisted")}>
        <Button isDisabled={busy} onPress={() => forget(app.id)}>{text("citations-forget")}</Button>
      </Row>)}
    </Group></Section>}
    {view && view.writings.length > 0 && <Section id="saved-writings" title={text("citations-writings")}><Group>
      {view.writings.map((writing) => <Row key={writing.id} id={`writing-${writing.id}`} label={writing.name}
        description={`${listedOutput(writing.output) ? outputName(listedOutput(writing.output)!) : text("citations-unlisted")}${writing.bibliography ? ` · ${writing.bibliography}` : ""}`} />)}
    </Group></Section>}
    {openSettingsFile && <Section id="settings-file"><Group><Row id="open-settings-file" {...rowDetails("open-settings-file", text)} highlighted={highlighted === "open-settings-file"}>
      <Button onPress={openSettingsFile}>{text("settings-open-file")}</Button>
    </Row></Group></Section>}
    {problem && <p role="alert">{text("settings-read-failed")}</p>}
    {forgetFailed && <p role="alert">{text("settings-forget-failed")} <Button size="small" onPress={dismissForget}>{text("picker-dismiss")}</Button></p>}
    {openFailed && <p role="alert">{text("settings-open-failed")} <Button size="small" onPress={dismissOpen}>{text("picker-dismiss")}</Button></p>}
  </>;
}
