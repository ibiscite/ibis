import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import type { WritingProjectEdit, WritingProjectsView, WritingProjectView } from "../../api";
import { writingProjectText as text } from "../../messages";
import { Button } from "../../ui/Button";
import { IconButton } from "../../ui/IconButton";
import { TextField } from "../../ui/TextField";
import { Progress } from "../../ui/Progress";
import { useLayer } from "../../ui/layers";
import { outputName, type Choice } from "./choice";
import { choiceOf, outputOf } from "./useDestination";
import { WritingProjectTaskStatus } from "./ProjectContextStatus";
import "../../ui/Overlay.css";
import "../../ui/KeysSheet.css";
import "./WritingProjects.css";

export type WritingProjectsApi = {
  writingProjects: () => Promise<WritingProjectsView>;
  editWritingProject: (context: string, edit: WritingProjectEdit) => Promise<WritingProjectsView>;
  chooseProject?: () => Promise<unknown>;
  stopProject?: () => Promise<unknown>;
  onProjectChanged?: (listener: () => void) => Promise<() => void>;
};

type Form = { kind: "new"; name: string } | { kind: "rename"; id: string; revision: string; name: string }
  | { kind: "remove"; id: string; revision: string; name: string };

// Native project refusals are fixed sentences and never include file paths or source text.
const failure = (error: unknown, fallback: string) => typeof error === "string" && error ? error : fallback;

/** An explicit project switch for the captured app. Native code saves the
 * project, its format and its bibliography together; the sheet never guesses
 * from a document or displays a native path. */
export function WritingProjectsDialog({ api, format, onClose }: { api: WritingProjectsApi; format: Choice; onClose: () => void }) {
  const { writingProjects, editWritingProject, onProjectChanged } = api;
  const dialog = useRef<HTMLDivElement>(null);
  const title = useId(), about = useId();
  const [view, setView] = useState<WritingProjectsView | null>(null);
  const [form, setForm] = useState<Form | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [conflict, setConflict] = useState<{ id: string; project: WritingProjectView | null } | null>(null);
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
  useEffect(() => { dialog.current?.focus(); }, []);
  useEffect(() => {
    if (!busy) { setBusyVisible(false); return; }
    const timer = window.setTimeout(() => setBusyVisible(true), 400);
    return () => window.clearTimeout(timer);
  }, [busy]);
  useLayer("sheet", true, close);
  // A saved edit may remove the focused control; keep the next key in the sheet.
  // biome-ignore lint/correctness/useExhaustiveDependencies: inspect focus after each edit or form change.
  useLayoutEffect(() => {
    const focused = document.activeElement;
    if (!dialog.current?.contains(focused) || (focused as HTMLButtonElement).disabled) dialog.current?.focus();
  }, [view, form?.kind, busy]);

  const read = useCallback(() => {
    if (mutation.current !== null) return;
    const request = ++reads.current;
    void writingProjects().then((next) => {
      if (alive.current && request === reads.current) setView(next);
    }, (error) => { if (alive.current && request === reads.current) setProblem(failure(error, text.notRead)); });
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
    if (mutation.current !== null || !view || view.active?.saved_problem) return;
    const owner = {};
    mutation.current = owner;
    reads.current += 1;
    setBusy(true);
    setProblem(null);
    void editWritingProject(view.context, change).then((next) => {
      if (!alive.current || mutation.current !== owner) return;
      setView(next);
      setForm(null);
      setConflict(null);
      if (leave) close();
    }, async (error) => {
      if (!alive.current || mutation.current !== owner) return;
      setProblem(failure(error, text.notSaved));
      if (!("id" in change) || !("revision" in change)) return;
      // A refusal may mean another client saved first. Read its definition
      // without replacing the user's input or rebasing their edit silently.
      try {
        const latest = await writingProjects();
        if (!alive.current || mutation.current !== owner) return;
        setView(latest);
        const project = latest.projects.find((item) => item.id === change.id) ?? null;
        if (project?.revision !== change.revision) setConflict({ id: change.id, project });
      } catch { /* Keep the task and the original refusal when rereading fails. */ }
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
    void action().then(read, (error) => { if (alive.current) setProblem(failure(error, text.bibliographyNotSaved)); });
  };
  const selected = view?.active;
  const selectedFormat = selected ? choiceOf(selected.output) : null;
  const editingDisabled = busy || Boolean(selected?.saved_problem);
  const saveName = () => {
    if (!form || form.kind === "remove" || !form.name.trim() || editingDisabled || conflict?.project === null) return;
    edit(form.kind === "new" ? { kind: "create", name: form.name.trim(), output: outputOf(format) }
      : { kind: "rename", id: form.id, revision: conflict?.id === form.id ? conflict.project?.revision ?? form.revision : form.revision, name: form.name.trim() });
  };
  return <div className="keys-backdrop" onPointerDown={(event) => { if (event.target === event.currentTarget) close(); }}>
    <div ref={dialog} role="dialog" aria-modal="true" tabIndex={-1} aria-labelledby={title} aria-describedby={about}
      className="keys-dialog writing-projects-dialog ui-overlay ui-sheet"
      onKeyDown={(event) => {
        if (event.key !== "Tab") return;
        const focusable = [...(dialog.current?.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), [tabindex='0']") ?? [])];
        const at = focusable.indexOf(document.activeElement as HTMLElement);
        event.preventDefault();
        focusable[at < 0 ? event.shiftKey ? focusable.length - 1 : 0 : (at + (event.shiftKey ? -1 : 1) + focusable.length) % focusable.length]?.focus();
      }}>
      <div className="keys-heading">
        <h2 id={title}>{text.title}</h2>
        <IconButton aria-label={text.close} title={text.close} aria-keyshortcuts="Escape" onPress={close} />
      </div>
      <div className="writing-projects-body keys-scroll scroll-area" aria-busy={busy || undefined}>
        <p id={about} className="writing-projects-note">{text.about}</p>
        {view?.session_only && <p className="writing-projects-note">{text.sessionOnly}</p>}
        {!view && !problem && <p role="status">{text.loading}</p>}
        {view && <>
          <Button label={text.none} aria-pressed={view.selected === null} data-selected={view.selected === null || undefined} isDisabled={editingDisabled} onPress={() => edit({ kind: "select", id: null }, true)} />
          {view.projects.length === 0 && !selected?.saved_problem && <p>{text.empty}</p>}
          <ul className="writing-projects-list" aria-label={text.title}>
            {view.projects.map((project) => {
              const choice = choiceOf(project.output);
              return <li key={project.id}>
                <Button label={project.name} aria-label={text.use(project.name)} aria-pressed={project.id === view.selected}
                  data-selected={project.id === view.selected || undefined}
                  isDisabled={editingDisabled || choice === null} onPress={() => edit({ kind: "select", id: project.id }, true)} />
                {project.id === view.selected && <span className="writing-projects-note">{text.selectedLabel}</span>}
                <span className="writing-projects-note" dir="auto">{choice ? outputName(choice) : text.unsupported}</span>
              </li>;
            })}
          </ul>
          {selected && <section className="writing-projects-current" aria-label={text.selected}>
            <h3 dir="auto">{selected.name}</h3>
            <WritingProjectTaskStatus project={selected} disabled={editingDisabled} onApply={selected.pending
              ? () => edit({ kind: "apply", id: selected.id, revision: selected.pending!.revision }) : undefined} />
            {selectedFormat && <p>{text.format(outputName(selectedFormat))}</p>}
            <p dir="auto">{selected.bibliography ? text.bibliography(selected.bibliography) : text.noBibliography}</p>
            {selected.pending && !selected.retained && <section className="writing-projects-updates" aria-label={text.savedUpdates}>
              <h4>{text.savedUpdates}</h4>
              <p>{text.format(choiceOf(selected.pending.output) ? outputName(choiceOf(selected.pending.output)!) : text.unsupported)}</p>
              <p dir="auto">{selected.pending.bibliography ? text.bibliography(selected.pending.bibliography) : text.noBibliography}</p>
            </section>}
            <div className="writing-projects-actions">
              {!selected.retained && <>
                {api.chooseProject && <Button label={text.chooseBibliography} isDisabled={editingDisabled} onPress={() => bibliography(api.chooseProject)} />}
                {selected.bibliography && api.stopProject && <Button label={text.removeBibliography} isDisabled={editingDisabled} onPress={() => bibliography(api.stopProject)} />}
                <Button label={text.rename} isDisabled={editingDisabled} onPress={() => { setConflict(null); setForm({ kind: "rename", id: selected.id, revision: selected.revision, name: selected.name }); }} />
                <Button label={text.remove} isDisabled={editingDisabled} onPress={() => { setConflict(null); setForm({ kind: "remove", id: selected.id, revision: selected.revision, name: selected.name }); }} />
              </>}
            </div>
          </section>}
          {!form && <Button label={text.create} isDisabled={editingDisabled} onPress={() => { setProblem(null); setConflict(null); setForm({ kind: "new", name: "" }); }} />}
        </>}
        {conflict && <section className="writing-projects-conflict" aria-label={text.conflictTitle}>
          <h3>{text.conflictTitle}</h3>
          {conflict.project ? <>
            <p dir="auto">{conflict.project.name}</p>
            <p>{text.format(choiceOf(conflict.project.output) ? outputName(choiceOf(conflict.project.output)!) : text.unsupported)}</p>
            <p dir="auto">{conflict.project.bibliography ? text.bibliography(conflict.project.bibliography) : text.noBibliography}</p>
            <p>{text.conflictReview}</p>
          </> : <p>{text.conflictDeleted}</p>}
        </section>}
        {form && <div className="writing-projects-form">
          {form.kind === "remove" ? <>
            <p>{text.removeQuestion(form.name)}</p>
            <div className="writing-projects-actions">
              <Button label={text.cancel} isDisabled={busy} onPress={() => { setForm(null); setConflict(null); }} />
              <Button label={conflict ? text.removeUpdated : text.confirmRemove} isDisabled={editingDisabled || conflict?.project === null} onPress={() => edit({ kind: "remove", id: form.id, revision: conflict?.id === form.id ? conflict.project?.revision ?? form.revision : form.revision })} />
            </div>
          </> : <form onSubmit={(event) => { event.preventDefault(); saveName(); }}>
            <TextField label={text.name} value={form.name} maxLength={80} isDisabled={editingDisabled} autoFocus onChange={(name) => setForm({ ...form, name })} />
            {form.kind === "new" && <p className="writing-projects-note">{text.startsWith(outputName(format))}</p>}
            <div className="writing-projects-actions">
              <Button label={text.cancel} isDisabled={busy} onPress={() => { setForm(null); setConflict(null); }} />
              <Button variant="primary" label={form.kind === "new" ? text.confirmCreate : conflict ? text.saveMyName : text.saveName} isDisabled={editingDisabled || !form.name.trim() || conflict?.project === null} onPress={saveName} />
            </div>
          </form>}
        </div>}
        <div className="writing-projects-status">
          {busyVisible && <Progress label={text.saving} />}
          {problem && <p role="alert">{problem}</p>}
        </div>
      </div>
    </div>
  </div>;
}
