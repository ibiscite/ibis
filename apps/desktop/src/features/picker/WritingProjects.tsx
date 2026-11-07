import { useCallback, useEffect, useRef, useState } from "react";
import type { WritingProjectEdit, WritingProjectsView } from "../../api";
import { isolate, say } from "../../i18n";
import { Button } from "../../ui/Button";
import { TextField } from "../../ui/TextField";
import { Progress } from "../../ui/Progress";
import { Sheet, SheetButtons, SheetList, SheetRow, SheetSection } from "../../ui/Sheet";
import { listedOutput, outputName, type OutputId } from "../../format/outputs";

export type WritingProjectsApi = {
  writingProjects: () => Promise<WritingProjectsView>;
  editWritingProject: (context: string, edit: WritingProjectEdit) => Promise<WritingProjectsView>;
  chooseProject?: () => Promise<unknown>;
  stopProject?: () => Promise<unknown>;
  onProjectChanged?: (listener: () => void) => Promise<() => void>;
};

type Form = { kind: "new"; name: string } | { kind: "rename"; id: string; name: string }
  | { kind: "remove"; id: string; name: string };

// Native project refusals are fixed sentences and never include file paths or source text.
const failure = (error: unknown, fallback: string) => typeof error === "string" && error ? error : fallback;

/** An explicit project switch for the captured app. Native code saves the
 * project, its format and its bibliography together; the sheet never guesses
 * from a document or displays a native path. */
export function WritingProjectsDialog({ api, format, onClose }: { api: WritingProjectsApi; format: OutputId; onClose: () => void }) {
  const { writingProjects, editWritingProject, onProjectChanged } = api;
  const [view, setView] = useState<WritingProjectsView | null>(null);
  const [form, setForm] = useState<Form | null>(null);
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
    void writingProjects().then((next) => {
      if (alive.current && request === reads.current) setView(next);
    }, (error) => { if (alive.current && request === reads.current) setProblem(failure(error, say("writing-project-not-read"))); });
  }, [writingProjects]);
  useEffect(() => {
    alive.current = true;
    read();
    const listening = onProjectChanged?.(read);
    // A bibliography choice may finish before subscription setup completes.
    void listening?.then(read);
    return () => {
      alive.current = false;
      reads.current += 1;
      mutation.current = null;
      void listening?.then((stop) => stop());
    };
  }, [read, onProjectChanged]);

  const edit = (change: WritingProjectEdit, leave = false) => {
    if (mutation.current !== null || !view) return;
    const owner = {};
    mutation.current = owner;
    reads.current += 1;
    setBusy(true);
    setProblem(null);
    void editWritingProject(view.context, change).then((next) => {
      if (!alive.current || mutation.current !== owner) return;
      setView(next);
      setForm(null);
      if (leave) close();
    }, (error) => {
      if (!alive.current || mutation.current !== owner) return;
      setProblem(failure(error, say("writing-project-not-saved")));
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
    void action().then(read, (error) => { if (alive.current) setProblem(failure(error, say("writing-project-bibliography-not-saved"))); });
  };
  const selected = view?.active;
  const selectedFormat = selected ? listedOutput(selected.output) : null;
  const saveName = () => {
    if (!form || form.kind === "remove" || !form.name.trim() || busy) return;
    edit(form.kind === "new" ? { kind: "create", name: form.name.trim(), output: format }
      : { kind: "rename", id: form.id, name: form.name.trim() });
  };
  return <Sheet title={say("writing-project-title")} closeLabel={say("writing-project-close")} description={say("writing-project-about")} busy={busy} onClose={close}>
        {view?.session_only && <p className="ui-sheet-note">{say("writing-project-session-only")}</p>}
        {!view && !problem && <p role="status">{say("writing-project-loading")}</p>}
        {view && <>
          <Button label={say("writing-project-none")} aria-pressed={view.selected === null} data-selected={view.selected === null || undefined} isDisabled={busy} onPress={() => edit({ kind: "select", id: null }, true)} />
          {view.projects.length === 0 && <p>{say("writing-project-empty")}</p>}
          <SheetList label={say("writing-project-title")}>
            {view.projects.map((project) => {
              const choice = listedOutput(project.output);
              return <SheetRow key={project.id}>
                <Button label={project.name} aria-label={say("writing-project-use", { name: project.name })} aria-pressed={project.id === view.selected}
                  data-selected={project.id === view.selected || undefined}
                  isDisabled={busy || choice === null} onPress={() => edit({ kind: "select", id: project.id }, true)} />
                {project.id === view.selected && <span className="ui-sheet-note">{say("writing-project-selected-label")}</span>}
                <span className="ui-sheet-note" dir="auto">{choice ? outputName(choice) : say("writing-project-unsupported")}</span>
              </SheetRow>;
            })}
          </SheetList>
          {selected && <SheetSection label={say("writing-project-selected")} heading={<h3 dir="auto">{selected.name}</h3>}>
            {selectedFormat && <p>{say("writing-project-format", { name: outputName(selectedFormat) })}</p>}
            <p dir="auto">{selected.bibliography ? say("writing-project-bibliography", { name: selected.bibliography }) : say("writing-project-no-bibliography")}</p>
            <SheetButtons>
              {api.chooseProject && <Button label={say("writing-project-choose-bibliography")} isDisabled={busy} onPress={() => bibliography(api.chooseProject)} />}
              {selected.bibliography && api.stopProject && <Button label={say("writing-project-remove-bibliography")} isDisabled={busy} onPress={() => bibliography(api.stopProject)} />}
              <Button label={say("writing-project-rename")} isDisabled={busy} onPress={() => setForm({ kind: "rename", id: selected.id, name: selected.name })} />
              <Button label={say("writing-project-remove")} isDisabled={busy} onPress={() => setForm({ kind: "remove", id: selected.id, name: selected.name })} />
            </SheetButtons>
          </SheetSection>}
          {!form && <Button label={say("writing-project-create")} isDisabled={busy} onPress={() => { setProblem(null); setForm({ kind: "new", name: "" }); }} />}
        </>}
        {form && <div>
          {form.kind === "remove" ? <>
            <p>{say("writing-project-remove-question", { name: isolate(form.name) })}</p>
            <SheetButtons>
              <Button label={say("writing-project-cancel")} isDisabled={busy} onPress={() => setForm(null)} />
              <Button label={say("writing-project-confirm-remove")} isDisabled={busy} onPress={() => edit({ kind: "remove", id: form.id })} />
            </SheetButtons>
          </> : <form onSubmit={(event) => { event.preventDefault(); saveName(); }}>
            <TextField label={say("writing-project-name")} value={form.name} maxLength={80} isDisabled={busy} autoFocus onChange={(name) => setForm({ ...form, name })} />
            {form.kind === "new" && <p className="ui-sheet-note">{say("writing-project-starts-with", { format: outputName(format) })}</p>}
            <SheetButtons>
              <Button label={say("writing-project-cancel")} isDisabled={busy} onPress={() => setForm(null)} />
              <Button variant="primary" label={form.kind === "new" ? say("writing-project-confirm-create") : say("writing-project-save-name")} isDisabled={busy || !form.name.trim()} onPress={saveName} />
            </SheetButtons>
          </form>}
        </div>}
        <div className="ui-sheet-status">
          {busyVisible && <Progress label={say("writing-project-saving")} />}
          {problem && <p role="alert">{problem}</p>}
        </div>
  </Sheet>;
}
