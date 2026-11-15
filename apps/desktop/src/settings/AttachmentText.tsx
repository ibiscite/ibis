import { useEffect, useRef, useState } from "react";
import type { AttachmentTextLibrary, LibraryId } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { listening } from "../nativeEvents";
import { Group, Row, Section } from "../ui/Layout";
import { Switch } from "../ui/Switch";
import type { SettingsApi } from "./api";
import { useSettingsRequests } from "./useSettingsRequests";

export type AttachmentTextApi = Required<Pick<SettingsApi, "attachmentTextLibraries" | "setAttachmentText">>
  & Pick<SettingsApi, "onSettingsChanged" | "onLibraryChanged">;

export function AttachmentText({ api, highlighted = false }: { api: AttachmentTextApi; highlighted?: boolean }) {
  const { text } = useInterfaceMessages();
  const [libraries, setLibraries] = useState<AttachmentTextLibrary[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [problems, setProblems] = useState<ReadonlySet<LibraryId>>(new Set());
  const saved = useRef(new Map<LibraryId, boolean>());
  const { read, begin, isPending } = useSettingsRequests(api.attachmentTextLibraries, (next) => {
    saved.current = new Map(next.map((library) => [library.id, library.on]));
    setLibraries(next); setFailed(false);
  }, () => setFailed(true));

  useEffect(() => {
    const stopSettings = listening(api.onSettingsChanged, read);
    const stopLibrary = listening(api.onLibraryChanged, read);
    read();
    window.addEventListener("focus", read);
    return () => {
      stopSettings(); stopLibrary(); window.removeEventListener("focus", read);
    };
  }, [read, api.onSettingsChanged, api.onLibraryChanged]);

  const change = (id: LibraryId, on: boolean) => {
    const request = begin(id);
    const update = (on: boolean) => setLibraries((previous) => previous?.map((library) => library.id === id ? { ...library, on } : library) ?? null);
    setProblems((previous) => { const next = new Set(previous); next.delete(id); return next; });
    update(on);
    void api.setAttachmentText(id, on).then((next) => {
      if (!request.current()) return;
      saved.current.set(id, next);
      update(next);
    }, () => {
      if (!request.current()) return;
      update(saved.current.get(id) ?? false);
      setProblems((previous) => new Set(previous).add(id));
    }).finally(() => {
      if (!request.current()) return;
      request.finish();
      if (!isPending()) read();
    });
  };

  return <Section title={text("attachment-text-title")} description={<span id="attachment-text-help">{text("attachment-text-description")}</span>}>
    <Group>
      {libraries === null ? <Row id="attachment-text" label={text("attachment-text-label")} pending={!failed}
        description={text("attachment-text-loading")} problem={failed ? text("settings-read-failed") : undefined} />
        : libraries.length === 0 ? <Row id="attachment-text" label={text("attachment-text-empty")} highlighted={highlighted} />
          : libraries.map((library, index) => {
            const id = index === 0 ? "attachment-text" : `attachment-text-${index}`;
            return <Row key={library.id} id={id} label={text("attachment-text-label")} description={library.name}
              highlighted={highlighted} problem={problems.has(library.id) ? text("settings-choice-failed") : undefined}>
              <Switch aria-labelledby={`${id} ${id}-description`} aria-describedby="attachment-text-help"
                isSelected={library.on} onChange={(on) => change(library.id, on)} />
            </Row>;
          })}
    </Group>
  </Section>;
}
