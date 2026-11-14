import { Status } from "../../ui/Status";
import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import type { WritingEdit, WritingsView } from "../../api";
import { isolate, problemText, say } from "../../i18n";
import { Button } from "../../ui/Button";
import { TextField } from "../../ui/TextField";
import { Progress } from "../../ui/Progress";
import { Buttons, ChoiceRow, Group, Note, Row, Section } from "../../ui/Layout";
import { Sheet, SheetStatus, type SheetMemory } from "../../ui/Sheet";
import { listedOutput, outputName, type OutputId } from "../../format/outputs";

export type WritingsApi = {
  writings: () => Promise<WritingsView>;
  editWriting: (context: string, edit: WritingEdit) => Promise<WritingsView>;
  chooseProject?: () => Promise<unknown>;
  stopProject?: () => Promise<unknown>;
  onProjectChanged?: (listener: () => void) => Promise<() => void>;
  onWritingsChanged?: (listener: () => void) => Promise<() => void>;
};

type Form = { kind: "new"; name: string } | { kind: "rename"; id: string; name: string }
  | { kind: "remove"; id: string; name: string };
export type WritingDraft = { form: Form | null; place: RefObject<SheetMemory | null> };


/** An explicit project switch for the captured app. Native code saves the
 * project, its format and its bibliography together; the sheet never guesses
 * from a document or displays a native path. */
export function WritingsDialog({ api, format, onClose, draft }: { api: WritingsApi; format: OutputId; onClose: () => void; draft?: RefObject<WritingDraft> }) {
  const { writings, editWriting, onProjectChanged, onWritingsChanged } = api;
  const [view, setView] = useState<WritingsView | null>(null);
  const [form, setForm] = useState<Form | null>(draft?.current.form ?? null);
  const changeForm = (next: Form | null) => { if (draft) draft.current.form = next; setForm(next); };
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [busyVisible, setBusyVisible] = useState(false);
  const reads = useRef(0);
  const mutation = useRef<object | null>(null);
  const alive = useRef(true);
  const close = () => {
    // Retire ownership immediately: React may not unmount this sheet until
    // after a late native completion has reached the microtask queue.
    alive.current = false;
    reads.current += 1;
    mutation.current = null;
    onClose();
  };
  useEffect(() => {
    if (!busy) { setBusyVisible(false); return; }
    const timer = window.setTimeout(() => setBusyVisible(true), 400);
    return () => window.clearTimeout(timer);
  }, [busy]);

  const read = useCallback(() => {
    if (mutation.current !== null) return;
    const request = ++reads.current;
    void writings().then((next) => {
      if (alive.current && request === reads.current) { setView(next); setProblem(null); }
    }, (error) => { if (alive.current && request === reads.current) setProblem(problemText(error, say("writing-not-read"))); });
  }, [writings]);
  useEffect(() => {
    alive.current = true;
    read();
    const listening = onProjectChanged?.(read);
    const writingsChanged = onWritingsChanged?.(read);
    // A bibliography choice may finish before subscription setup completes.
    void listening?.then(read);
    return () => {
      alive.current = false;
      reads.current += 1;
      mutation.current = null;
      void listening?.then((stop) => stop());
      void writingsChanged?.then((stop) => stop());
    };
  }, [read, onProjectChanged, onWritingsChanged]);

  const edit = (change: WritingEdit, leave = false) => {
    if (mutation.current !== null || !view) return;
    const owner = {};
    mutation.current = owner;
    reads.current += 1;
    setBusy(true);
    setProblem(null);
    void editWriting(view.context, change).then((next) => {
      if (!alive.current || mutation.current !== owner) return;
      setView(next);
      changeForm(null);
      if (leave) close();
    }, (error) => {
      if (!alive.current || mutation.current !== owner) return;
      setProblem(problemText(error, say("writing-not-saved")));
    })
      .finally(() => {
        if (!alive.current || mutation.current !== owner) return;
        mutation.current = null;
        setBusy(false);
      });
  };
  const bibliography = (action: (() => Promise<unknown>) | undefined) => {
    if (!action || busy) return;
    setProblem(null);
    void action().then(read, (error) => { if (alive.current) setProblem(problemText(error, say("writing-bibliography-not-saved"))); });
  };
  const selected = view?.active;
  const saveName = () => {
    if (!form || form.kind === "remove" || !form.name.trim() || busy) return;
    edit(form.kind === "new" ? { kind: "create", name: form.name.trim(), output: format }
      : { kind: "rename", id: form.id, name: form.name.trim() });
  };
  const selectedFormat = selected && listedOutput(selected.output);
  const creating = !form && view ? [{ label: say("writing-create"), disabled: busy, onPress: () => { setProblem(null); changeForm({ kind: "new", name: "" }); } }] : undefined;
  return <Sheet title={say("writing-title")} closeLabel={say("writing-close")} description={say("writing-about")} busy={busy} onClose={close}
    leadingActions={creating} memory={draft?.current.place}>
        {view?.session_only && <p>{say("writing-session-only")}</p>}
        {!view && !problem && <Status as="p">{say("writing-loading")}</Status>}
        {view && <>
          {view.writings.length === 0 && <p>{say("writing-empty")}</p>}
          <Group label={say("writing-title")}>
            <ChoiceRow label={say("writing-none")} selected={view.selected === null} disabled={busy} onPress={() => edit({ kind: "select", id: null }, true)} />
            {view.writings.map((project) => {
              const choice = listedOutput(project.output);
              return <ChoiceRow key={project.id} label={project.name} accessibleLabel={say("writing-use", { name: project.name })}
                description={choice ? outputName(choice) : say("writing-unsupported")} selected={project.id === view.selected}
                disabled={busy || choice === null} onPress={() => edit({ kind: "select", id: project.id }, true)} />;
            })}
          </Group>
          {selected && <Section label={say("writing-selected")} title={selected.name} level={3}>
            {selectedFormat && <Note>{say("writing-format", { name: outputName(selectedFormat) })}</Note>}
            <Group label={say("writing-selected")}>
              <Row item label={selected.bibliography ? say("writing-bibliography", { name: selected.bibliography }) : say("writing-no-bibliography")}>
                {api.chooseProject && <Button size="small" label={say("writing-choose-bibliography")} isDisabled={busy} onPress={() => bibliography(api.chooseProject)} />}
                {selected.bibliography && api.stopProject && <Button size="small" label={say("writing-remove-bibliography")} isDisabled={busy} onPress={() => bibliography(api.stopProject)} />}
              </Row>
            </Group>
            <Buttons>
              <Button label={say("writing-rename")} isDisabled={busy} onPress={() => changeForm({ kind: "rename", id: selected.id, name: selected.name })} />
              <Button label={say("writing-remove")} isDisabled={busy} onPress={() => changeForm({ kind: "remove", id: selected.id, name: selected.name })} />
            </Buttons>
          </Section>}
        </>}
        {form && <Section label={form.kind === "remove" ? say("writing-remove") : say("writing-name")}>
          {form.kind === "remove" ? <>
            <p>{say("writing-remove-question", { name: isolate(form.name) })}</p>
            <Buttons>
              <Button label={say("writing-cancel")} isDisabled={busy} onPress={() => changeForm(null)} />
              <Button label={say("writing-confirm-remove")} isDisabled={busy} onPress={() => edit({ kind: "remove", id: form.id })} />
            </Buttons>
          </> : <form className="ui-section" onSubmit={(event) => { event.preventDefault(); saveName(); }}>
            <TextField label={say("writing-name")} value={form.name} maxLength={80} isDisabled={busy} autoFocus onChange={(name) => changeForm({ ...form, name })} />
            {form.kind === "new" && <Note>{say("writing-starts-with", { format: outputName(format) })}</Note>}
            <Buttons>
              <Button label={say("writing-cancel")} isDisabled={busy} onPress={() => changeForm(null)} />
              <Button variant="primary" label={form.kind === "new" ? say("writing-confirm-create") : say("writing-save-name")} isDisabled={busy || !form.name.trim()} onPress={saveName} />
            </Buttons>
          </form>}
        </Section>}
        <SheetStatus>
          {busyVisible && <Progress label={say("writing-saving")} />}
          {problem && <Status as="p" role="alert">{problem}</Status>}
        </SheetStatus>
  </Sheet>;
}
