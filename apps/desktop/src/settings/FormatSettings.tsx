import { useEffect, useRef, useState } from "react";
import type { FormatSettingsView } from "../bindings";
import { outputName, listedOutput } from "../format/outputs";
import { useInterfaceMessages } from "../i18n";
import { Button } from "../ui/Button";
import { SettingsGroup as Group, SettingsRow as Row } from "../ui/SettingsGroup";
import type { SettingsApi } from "./api";

/** App choices and writings come from the engine's settings snapshot. A late
 * read or forget never replaces a more recent snapshot or a reopened page. */
export function FormatSettings({ api, onKeyVisibility }: { api: SettingsApi; onKeyVisibility: (visible: boolean) => void }) {
  const { text } = useInterfaceMessages();
  const [view, setView] = useState<FormatSettingsView | null>(null);
  const [problem, setProblem] = useState(false);
  const [busy, setBusy] = useState(false);
  const request = useRef(0);
  useEffect(() => {
    if (!api.formatSettings) return;
    let alive = true;
    const read = () => {
      const ticket = ++request.current;
      void api.formatSettings!().then((next) => {
        if (!alive || ticket !== request.current) return;
        setView(next); setProblem(false); onKeyVisibility(next.key_output_visible);
      }, () => { if (alive && ticket === request.current) setProblem(true); });
    };
    const events = api.onPreferencesChanged(read);
    void events.then(read, read);
    window.addEventListener("focus", read);
    return () => { alive = false; request.current += 1; window.removeEventListener("focus", read); void events.then((stop) => stop()); };
  }, [api, onKeyVisibility]);
  const forget = (id: string) => {
    if (!api.forgetAppFormat || !api.formatSettings || busy) return;
    const ticket = ++request.current;
    setBusy(true); setProblem(false);
    void api.forgetAppFormat(id).then(() => api.formatSettings!()).then((next) => {
      if (ticket === request.current) { setView(next); onKeyVisibility(next.key_output_visible); }
    }, () => { if (ticket === request.current) setProblem(true); }).finally(() => setBusy(false));
  };
  return <>
    {view && view.apps.length > 0 && <Group id="remembered-formats" title={text("citations-apps")}>
      {view.apps.map((app) => <Row key={app.id} id={`format-${app.id}`} label={app.app} description={listedOutput(app.format) ? outputName(listedOutput(app.format)!) : text("citations-unlisted")} changed>
        <Button isDisabled={busy} onPress={() => forget(app.id)}>{text("citations-forget")}</Button>
      </Row>)}
    </Group>}
    {view && view.writings.length > 0 && <Group id="saved-writings" title={text("citations-writings")}>
      {view.writings.map((writing) => <Row key={writing.id} id={`writing-${writing.id}`} label={writing.name}
        description={`${listedOutput(writing.output) ? outputName(listedOutput(writing.output)!) : text("citations-unlisted")}${writing.bibliography ? ` · ${writing.bibliography}` : ""}`} />)}
    </Group>}
    {api.openSettingsFile && <Group id="settings-file"><Row id="open-settings-file" label={text("settings-open-file")} description={text("settings-file-description")}>
      <Button onPress={() => { void api.openSettingsFile!().catch(() => setProblem(true)); }}>{text("settings-open-file")}</Button>
    </Row></Group>}
    {problem && <p role="alert">{text("settings-read-failed")}</p>}
  </>;
}
