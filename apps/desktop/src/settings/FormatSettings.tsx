import { useEffect, useRef, useState } from "react";
import type { FormatSettingsView, Problem } from "../bindings";
import { outputName, listedOutput } from "../format/outputs";
import { problemText, useInterfaceMessages } from "../i18n";
import { Button } from "../ui/Button";
import { Group, Row, Section } from "../ui/Layout";
import type { SettingsApi } from "./api";
import { rowDetails } from "./rows";
import { Status } from "../ui/Status";
import { listening } from "../nativeEvents";

/** App choices and writings come from the engine's settings snapshot. A late
 * read never replaces a more recent one. */
export function useFormatSettings(api: Pick<SettingsApi, "formatSettings" | "onSettingsChanged" | "forgetAppFormat" | "openSettingsFile">) {
  const [view, setView] = useState<FormatSettingsView | null>(null);
  // Why the file could not be read: the engine's Problem, as the picker gets it.
  const [problem, setProblem] = useState<unknown>(null);
  const [forgetFailed, setForgetFailed] = useState(false);
  const [openFailed, setOpenFailed] = useState(false);
  const [busy, setBusy] = useState(false);
  const request = useRef(0);
  const openRequest = useRef(0);
  const refresh = useRef<(() => Promise<void>) | null>(null);
  useEffect(() => {
    if (!api.formatSettings) return;
    const read = () => {
      const ticket = ++request.current;
      return api.formatSettings!().then((next) => {
        if (ticket !== request.current) return;
        setView(next); setProblem(null);
      }, (reason: unknown) => { if (ticket === request.current) setProblem(reason ?? new Error("settings unread")); });
    };
    refresh.current = read;
    const stop = listening(api.onSettingsChanged, read, { ready: read, failed: read });
    window.addEventListener("focus", read);
    return () => { refresh.current = null; request.current += 1; window.removeEventListener("focus", read); stop(); };
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

export function FormatSettings({ settings }: { settings: ReturnType<typeof useFormatSettings> }) {
  const { text } = useInterfaceMessages();
  const { view, busy, forget, forgetFailed, dismissForget } = settings;
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
    {forgetFailed && <Status role="alert">{text("settings-forget-failed")} <Button size="small" onPress={dismissForget}>{text("picker-dismiss")}</Button></Status>}
  </>;
}

/** Why settings.json could not be read, in the engine's own words (the picker
 * shows the same Problem), with its one recovery: open the file to repair it. */
export function SettingsFileProblem({ settings }: { settings: ReturnType<typeof useFormatSettings> }) {
  const { text } = useInterfaceMessages();
  const { problem, openSettingsFile } = settings;
  if (problem === null) return null;
  const review = typeof problem === "object" && (problem as Partial<Problem>).recovery?.action === "review-settings";
  return <Status tone="warning" role="alert">{problemText(problem, text("settings-read-failed"), text, "settings")}
    {review && openSettingsFile && <> <Button size="small" onPress={openSettingsFile}>{text("settings-open")}</Button></>}</Status>;
}

/** Opening settings.json, in General's Advanced group. */
export function SettingsFileRow({ settings, highlighted }: { settings: ReturnType<typeof useFormatSettings>; highlighted: string | null }) {
  const { text } = useInterfaceMessages();
  const { openSettingsFile, openFailed, dismissOpen } = settings;
  if (!openSettingsFile) return null;
  return <><Row id="open-settings-file" {...rowDetails("open-settings-file", text)} highlighted={highlighted === "open-settings-file"}>
    <Button onPress={openSettingsFile}>{text("settings-open")}</Button>
  </Row>
  {openFailed && <Status role="alert">{text("settings-open-failed")} <Button size="small" onPress={dismissOpen}>{text("picker-dismiss")}</Button></Status>}</>;
}
