import { useCallback, useEffect, useRef, useState } from "react";
import type { AttachmentTextLibrary, LibraryId } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { listening } from "../nativeEvents";
import { Group, Row, Section } from "../ui/Layout";
import { Switch } from "../ui/Switch";
import type { SettingsApi } from "./api";
import { useSettingsRequests } from "./useSettingsRequests";

export type AttachmentTextApi = Required<Pick<SettingsApi, "attachmentTextLibraries" | "setAttachmentText" | "attachmentTextOutsideLibrary" | "setAttachmentTextOutsideLibrary">>
  & Pick<SettingsApi, "onSettingsChanged" | "onLibraryChanged">;

const OUTSIDE = "outside-library";

export function AttachmentText({ api, highlighted = false }: { api: AttachmentTextApi; highlighted?: boolean }) {
  const { text } = useInterfaceMessages();
  const [libraries, setLibraries] = useState<AttachmentTextLibrary[] | null>(null);
  const [outside, setOutside] = useState<boolean | null>(null);
  const [failed, setFailed] = useState(false);
  const [problems, setProblems] = useState<ReadonlySet<LibraryId>>(new Set());
  const saved = useRef(new Map<LibraryId, boolean>());
  const load = useCallback(() => Promise.all([api.attachmentTextLibraries(), api.attachmentTextOutsideLibrary()]), [api.attachmentTextLibraries, api.attachmentTextOutsideLibrary]);
  const { read, begin, isPending } = useSettingsRequests(load, ([next, outside]) => {
    saved.current = new Map(next.map((library) => [library.id, library.on]));
    saved.current.set(OUTSIDE, outside);
    setOutside(outside);
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
    const update = (on: boolean) => id === OUTSIDE ? setOutside(on) : setLibraries((previous) => previous?.map((library) => library.id === id ? { ...library, on } : library) ?? null);
    setProblems((previous) => { const next = new Set(previous); next.delete(id); return next; });
    update(on);
    const saving = id === OUTSIDE ? api.setAttachmentTextOutsideLibrary(on) : api.setAttachmentText(id, on);
    void saving.then((next) => {
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
      <Row id="attachment-text-outside" label={text("attachment-text-outside-label")}
        description={text("attachment-text-outside-description")} pending={outside === null && !failed}
        problem={problems.has(OUTSIDE) ? text("settings-choice-failed") : outside === null && failed ? text("settings-read-failed") : undefined}>
        {outside === null ? <span className="ui-switch" data-placeholder aria-hidden="true"><span className="ui-switch-track" /></span>
          : <Switch aria-labelledby="attachment-text-outside" aria-describedby="attachment-text-outside-description"
            isSelected={outside} onChange={(on) => change(OUTSIDE, on)} />}
      </Row>
    </Group>
  </Section>;
}
