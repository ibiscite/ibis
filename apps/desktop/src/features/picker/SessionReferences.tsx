import { useCallback, useEffect, useRef, useState } from "react";
import type { PickerApi } from "../../App";
import type { SessionBibliography, SessionEdit, SessionReferencesView } from "../../api";
import { outputName, type OutputId } from "../../format/outputs";
import { isolate, say } from "../../i18n";
import { Button } from "../../ui/Button";
import { Sheet, SheetList, SheetRow } from "../../ui/Sheet";
import { idKey } from "./choice";
import { abbreviatedAuthors, tileName } from "./rows";

type Api = Required<Pick<PickerApi, "sessionReferences" | "editSessionReferences" | "sessionBibliography" | "copySessionBibliography" | "exportSessionBibliography">>;

const changed = (error: unknown) => typeof error === "object" && error !== null && (error as { code?: string }).code === "snapshot_changed";
/** Native refusals name what to do; anything else gets this sheet's own sentence. */
const said = (error: unknown, fallback: string) => {
  const { code, message } = typeof error === "object" && error !== null ? error as { code?: unknown; message?: unknown } : {};
  return code === "failed" && typeof message === "string" && message ? message : fallback;
};

/** The references copied this session, editable, and their bibliography. Native
    code owns the list; each answer counts only if it is still the newest asked. */
export function SessionReferencesDialog({ api, style, onClose }: { api: Api; style: OutputId; onClose: () => void }) {
  const { sessionReferences, editSessionReferences, sessionBibliography, copySessionBibliography, exportSessionBibliography } = api;
  const [view, setView] = useState<SessionReferencesView | null>(null);
  const [bibliography, setBibliography] = useState<SessionBibliography | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const listing = useRef(0);
  const formatting = useRef(0);

  const show = useCallback((next: SessionReferencesView) => {
    setView(next);
    const request = ++formatting.current;
    setBibliography(null);
    sessionBibliography(style).then(
      (formatted) => { if (request === formatting.current) setBibliography(formatted); },
      (error) => { if (request === formatting.current) setProblem(said(error, say("session-not-formatted"))); });
  }, [sessionBibliography, style]);
  const load = useCallback((read: () => Promise<SessionReferencesView>) => {
    const request = ++listing.current;
    setProblem(null);
    read().then((next) => { if (request === listing.current) show(next); },
      () => { if (request === listing.current) setProblem(say("session-not-read")); });
  }, [show]);
  useEffect(() => {
    load(sessionReferences);
    return () => { listing.current += 1; formatting.current += 1; };
  }, [load, sessionReferences]);

  const edit = (change: SessionEdit) => {
    setStatus(null);
    load(() => editSessionReferences(change));
  };
  /** Copy or save what is on screen; a list that moved on is shown again instead. */
  const deliver = (act: () => Promise<string | null>, failure: string) => {
    setStatus(null);
    setProblem(null);
    act().then((done) => { if (done) setStatus(done); }, (error) => {
      if (changed(error)) { setProblem(say("session-changed")); load(sessionReferences); } else setProblem(said(error, failure));
    });
  };
  const hash = bibliography?.content_hash;
  const undo = view?.undo;
  const undoLabel = !undo ? null : undo.kind === "reset" ? say("session-undo-reset")
    : (undo.reference ? say("session-undo-remove-named", { name: isolate(tileName(undo.reference)) }) : say("session-undo-remove"));
  const name = outputName(style);
  return <Sheet title={say("session-title")} closeLabel={say("session-close")} description={say("session-about")} onClose={onClose}
    leadingActions={<>
      {undoLabel && <Button label={undoLabel} onPress={() => edit({ kind: "undo" })} />}
      {view && view.rows.length + view.absent > 0 && <Button label={say("session-reset")} onPress={() => edit({ kind: "reset" })} />}
    </>}
    actions={<>
      <Button label={say("session-save")} isDisabled={!hash} onPress={() => deliver(async () => {
        const saved = await exportSessionBibliography(style, hash!);
        return saved === null ? null : say("session-saved", { name: isolate(saved) });
      }, say("session-not-saved"))} />
      <Button variant="primary" label={say("session-copy")} isDisabled={!hash} onPress={() => deliver(async () => {
        await copySessionBibliography(style, hash!);
        return say("session-copied");
      }, say("session-not-copied"))} />
    </>}>
    {view && view.rows.length === 0 && view.absent === 0 && <p>{say("session-empty")}</p>}
    {view && view.rows.length > 0 && <SheetList label={say("session-title")}>
      {view.rows.map((row) => <SheetRow key={idKey(row.id)}>
        <span>
          <span dir="auto">{row.title}</span>
          <span className="ui-sheet-note" dir="auto">{[abbreviatedAuthors(row) ? row.first_author : row.authors, row.year].filter((part) => part !== null && part !== "").join(", ")}</span>
        </span>
        <Button size="small" label={say("session-remove")} aria-label={say("session-remove-named", { name: isolate(tileName(row)) })} onPress={() => edit({ kind: "remove", id: row.id })} />
      </SheetRow>)}
    </SheetList>}
    {view && view.absent > 0 && <p role="status">{say("session-absent", { count: view.absent })}</p>}
    {view?.overflowed && <p role="status">{say("session-full")}</p>}
    {bibliography && <>
      <h3>{say("session-bibliography-in", { style: isolate(name) })}</h3>
      <p className="ui-sheet-output" dir="auto">{bibliography.text}</p>
      {bibliography.warnings.map((warning) => <p key={warning} role="status">{warning}</p>)}
      {bibliography.left_out.length > 0 && <p role="status">{say("session-left-out", { names: bibliography.left_out.map(isolate).join(", ") })}</p>}
    </>}
    {problem && <p role="alert">{problem}</p>}
    {status && <p role="status">{status}</p>}
  </Sheet>;
}
