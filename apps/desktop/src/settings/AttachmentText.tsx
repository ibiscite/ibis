import { useEffect, useRef, useState } from "react";
import type { AttachmentTextLibrary, LibraryId } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { listening } from "../nativeEvents";
import { Group, Row, Section } from "../ui/Layout";
import { Switch } from "../ui/Switch";
import type { SettingsApi } from "./api";

export type AttachmentTextApi = Required<Pick<SettingsApi, "attachmentTextLibraries" | "setAttachmentText">>
  & Pick<SettingsApi, "onSettingsChanged" | "onLibraryChanged">;

/** One device-only choice per open library, shared with the command line. */
export function AttachmentText({ api, highlighted = false }: { api: AttachmentTextApi; highlighted?: boolean }) {
  const { text } = useInterfaceMessages();
  const [libraries, setLibraries] = useState<AttachmentTextLibrary[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [problems, setProblems] = useState<ReadonlySet<LibraryId>>(new Set());
  const sequence = useRef(0);
  const pending = useRef(new Map<LibraryId, number>());
  const saved = useRef(new Map<LibraryId, boolean>());
  const mounted = useRef(false);
  const refresh = useRef(() => {});

  useEffect(() => {
    mounted.current = true;
    const read = () => {
      if (pending.current.size) return;
      const ticket = ++sequence.current;
      void api.attachmentTextLibraries().then((next) => {
        if (!mounted.current || ticket !== sequence.current) return;
        saved.current = new Map(next.map((library) => [library.id, library.on]));
        setLibraries(next); setFailed(false);
      }, () => { if (mounted.current && ticket === sequence.current) setFailed(true); });
    };
    refresh.current = read;
    const stopSettings = listening(api.onSettingsChanged, read);
    const stopLibrary = listening(api.onLibraryChanged, read);
    read();
    window.addEventListener("focus", read);
    return () => {
      mounted.current = false; sequence.current += 1; pending.current.clear();
      refresh.current = () => {};
      stopSettings(); stopLibrary(); window.removeEventListener("focus", read);
    };
  }, [api.attachmentTextLibraries, api.onSettingsChanged, api.onLibraryChanged]);

  const change = (id: LibraryId, on: boolean) => {
    const ticket = ++sequence.current;
    pending.current.set(id, ticket);
    setProblems((previous) => { const next = new Set(previous); next.delete(id); return next; });
    setLibraries((previous) => previous?.map((library) => library.id === id ? { ...library, on } : library) ?? null);
    const current = () => mounted.current && pending.current.get(id) === ticket;
    void api.setAttachmentText(id, on).then((next) => {
      if (!current()) return;
      saved.current.set(id, next);
      setLibraries((previous) => previous?.map((library) => library.id === id ? { ...library, on: next } : library) ?? null);
    }, () => {
      if (!current()) return;
      setLibraries((previous) => previous?.map((library) => library.id === id ? { ...library, on: saved.current.get(id) ?? false } : library) ?? null);
      setProblems((previous) => new Set(previous).add(id));
    }).finally(() => {
      if (!current()) return;
      pending.current.delete(id);
      if (!pending.current.size) refresh.current();
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
