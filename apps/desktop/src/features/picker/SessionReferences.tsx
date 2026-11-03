import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { PickerApi } from "../../App";
import type { SessionBibliography, SessionEdit, SessionReferencesView } from "../../api";
import { STYLE_NAMES, type StyleId } from "../../format/styles";
import { sessionText } from "../../messages";
import { Button } from "../../ui/Button";
import { IconButton } from "../../ui/IconButton";
import { idKey } from "./choice";
import { abbreviatedAuthors, tileName } from "./rows";
import "../../ui/Overlay.css";
import "../../ui/KeysSheet.css";
import "./SessionReferences.css";

type Api = Required<Pick<PickerApi, "sessionReferences" | "editSessionReferences" | "sessionBibliography" | "copySessionBibliography" | "exportSessionBibliography">>;

const changed = (error: unknown) => typeof error === "object" && error !== null && (error as { code?: string }).code === "snapshot_changed";
/** Native refusals name what to do; anything else gets this sheet's own sentence. */
const said = (error: unknown, fallback: string) => {
  const { code, message } = typeof error === "object" && error !== null ? error as { code?: unknown; message?: unknown } : {};
  return code === "failed" && typeof message === "string" && message ? message : fallback;
};

/** The references copied this session, editable, and their bibliography. Native
    code owns the list; each answer counts only if it is still the newest asked. */
export function SessionReferencesDialog({ api, style, onClose }: { api: Api; style: StyleId; onClose: () => void }) {
  const { sessionReferences, editSessionReferences, sessionBibliography, copySessionBibliography, exportSessionBibliography } = api;
  const dialog = useRef<HTMLDivElement>(null);
  const [view, setView] = useState<SessionReferencesView | null>(null);
  const [bibliography, setBibliography] = useState<SessionBibliography | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const listing = useRef(0);
  const formatting = useRef(0);
  useEffect(() => { dialog.current?.focus(); }, []);
  // A removed row takes its focused button with it. The sheet keeps the keyboard,
  // or the next key would act on the picker behind it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: each new answer can remove the focused control.
  useLayoutEffect(() => {
    const focused = document.activeElement;
    if (!dialog.current?.contains(focused) || (focused as HTMLButtonElement).disabled) dialog.current?.focus();
  }, [view, bibliography]);

  const show = useCallback((next: SessionReferencesView) => {
    setView(next);
    const request = ++formatting.current;
    setBibliography(null);
    sessionBibliography(style).then(
      (formatted) => { if (request === formatting.current) setBibliography(formatted); },
      (error) => { if (request === formatting.current) setProblem(said(error, sessionText.notFormatted)); });
  }, [sessionBibliography, style]);
  const load = useCallback((read: () => Promise<SessionReferencesView>) => {
    const request = ++listing.current;
    setProblem(null);
    read().then((next) => { if (request === listing.current) show(next); },
      () => { if (request === listing.current) setProblem(sessionText.notRead); });
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
      if (changed(error)) { setProblem(sessionText.changed); load(sessionReferences); } else setProblem(said(error, failure));
    });
  };
  const hash = bibliography?.content_hash;
  const undo = view?.undo;
  const undoLabel = !undo ? null : undo.kind === "reset" ? sessionText.undoReset
    : sessionText.undoRemove(undo.reference ? tileName(undo.reference) : null);
  const name = STYLE_NAMES[style];
  return <div className="keys-backdrop" onPointerDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
    <div ref={dialog} role="dialog" aria-modal="true" tabIndex={-1} aria-labelledby="session-title" aria-describedby="session-about"
      className="keys-dialog session-dialog ui-overlay ui-sheet"
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape") {
          e.preventDefault();
          onClose();
        } else if (e.key === "Tab") {
          const focusable = [...(dialog.current?.querySelectorAll<HTMLElement>("button:not([disabled]), [tabindex='0']") ?? [])];
          const at = focusable.indexOf(document.activeElement as HTMLElement);
          e.preventDefault();
          focusable[at < 0 ? e.shiftKey ? focusable.length - 1 : 0 : (at + (e.shiftKey ? -1 : 1) + focusable.length) % focusable.length]?.focus();
        }
      }}>
      <div className="keys-heading">
        <h2 id="session-title">{sessionText.title}</h2>
        <IconButton aria-label={sessionText.close} title={sessionText.close} aria-keyshortcuts="Escape" onPress={onClose} />
      </div>
      <div className="session-body scroll-area" tabIndex={0} role="group" aria-labelledby="session-title">
        <p id="session-about" className="session-note">{sessionText.about}</p>
        {view && view.rows.length === 0 && view.absent === 0 && <p>{sessionText.empty}</p>}
        {view && view.rows.length > 0 && <ul className="session-list" aria-label={sessionText.title}>
          {view.rows.map((row) => <li key={idKey(row.id)}>
            <span className="session-reference">
              <span className="session-title" dir="auto">{row.title}</span>
              <span className="session-note" dir="auto">{[abbreviatedAuthors(row) ? row.first_author : row.authors, row.year].filter((part) => part !== null && part !== "").join(", ")}</span>
            </span>
            <Button size="small" label={sessionText.remove} aria-label={sessionText.removeNamed(tileName(row))} onPress={() => edit({ kind: "remove", id: row.id })} />
          </li>)}
        </ul>}
        {view && view.absent > 0 && <p role="status">{sessionText.absent(view.absent)}</p>}
        {view?.overflowed && <p role="status">{sessionText.full}</p>}
        {bibliography && <>
          <h3 className="session-heading">{sessionText.bibliographyIn(name)}</h3>
          <p className="session-bibliography" dir="auto">{bibliography.text}</p>
          {bibliography.warnings.map((warning) => <p key={warning} role="status">{warning}</p>)}
          {bibliography.left_out.length > 0 && <p role="status">{sessionText.leftOut(bibliography.left_out)}</p>}
        </>}
        {problem && <p role="alert">{problem}</p>}
        {status && <p role="status">{status}</p>}
      </div>
      <div className="session-actions">
        {undoLabel && <Button label={undoLabel} onPress={() => edit({ kind: "undo" })} />}
        {view && view.rows.length + view.absent > 0 && <Button label={sessionText.reset} onPress={() => edit({ kind: "reset" })} />}
        <span className="session-spacer" />
        <Button label={sessionText.save} isDisabled={!hash} onPress={() => deliver(async () => {
          const saved = await exportSessionBibliography(style, hash!);
          return saved === null ? null : sessionText.saved(saved);
        }, sessionText.notSaved)} />
        <Button variant="primary" label={sessionText.copy} isDisabled={!hash} onPress={() => deliver(async () => {
          await copySessionBibliography(style, hash!);
          return sessionText.copied;
        }, sessionText.notCopied)} />
      </div>
    </div>
  </div>;
}
