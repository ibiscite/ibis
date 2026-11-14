import { Status } from "../../ui/Status";
import { renderDisplay } from "../../format/display";
import type * as nativeApi from "../../api";
import { useCallback, useEffect, useRef, useState } from "react";
import type { SessionBibliography, SessionEdit, SessionReferencesView } from "../../api";
import { isCancelled } from "../../api";
import { outputName, type OutputId } from "../../format/outputs";
import { isolate, problemText, say } from "../../i18n";
import { Button } from "../../ui/Button";
import { Group, Row, Section } from "../../ui/Layout";
import { Sheet, SheetOutput } from "../../ui/Sheet";
import { idKey } from "./choice";
import { abbreviatedAuthors, tileName } from "./rows";

type Api = Pick<typeof nativeApi, "sessionReferences" | "editSessionReferences" | "sessionBibliography" | "copySessionBibliography" | "exportSessionBibliography">;

const changed = (error: unknown) => typeof error === "object" && error !== null && (error as { code?: string }).code === "snapshot_changed";

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
  const activeFormatting = useRef<AbortController | null>(null);
  const delivery = useRef(0);
  const activeDelivery = useRef<AbortController | null>(null);
  const pendingDelivery = useRef(false);

  const show = useCallback((next: SessionReferencesView) => {
    setView(next);
    const request = ++formatting.current;
    activeFormatting.current?.abort();
    const controller = new AbortController();
    activeFormatting.current = controller;
    setBibliography(null);
    sessionBibliography(style, controller.signal).then(
      (formatted) => { if (request === formatting.current) setBibliography(formatted); },
      (error) => { if (request === formatting.current) setProblem(problemText(error, say("session-not-formatted"))); });
  }, [sessionBibliography, style]);
  const load = useCallback((read: () => Promise<SessionReferencesView>, problem: string | null = null) => {
    formatting.current += 1;
    activeFormatting.current?.abort();
    delivery.current += 1;
    activeDelivery.current?.abort();
    pendingDelivery.current = false;
    const request = ++listing.current;
    setProblem(problem);
    read().then((next) => { if (request === listing.current) show(next); },
      (error) => { if (request === listing.current) setProblem(problemText(error, say("session-not-read"))); });
  }, [show]);
  useEffect(() => {
    load(sessionReferences);
    return () => {
      listing.current += 1; formatting.current += 1; delivery.current += 1;
      activeFormatting.current?.abort(); activeDelivery.current?.abort();
      pendingDelivery.current = false;
    };
  }, [load, sessionReferences]);

  const edit = (change: SessionEdit) => {
    setStatus(null);
    const wasDelivering = pendingDelivery.current;
    load(() => editSessionReferences(change), wasDelivering ? say("session-changed") : null);
  };
  /** Copy or save what is on screen; a list that moved on is shown again instead. */
  const deliver = (act: (signal: AbortSignal) => Promise<string | null>, failure: string) => {
    const request = ++delivery.current;
    activeDelivery.current?.abort();
    const controller = new AbortController();
    activeDelivery.current = controller;
    const owns = () => {
      if (request !== delivery.current) return false;
      pendingDelivery.current = false;
      return true;
    };
    setStatus(null);
    setProblem(null);
    pendingDelivery.current = true;
    act(controller.signal).then((done) => { if (owns() && done) setStatus(done); }, (error) => {
      if (!owns() || isCancelled(error)) return;
      if (changed(error)) load(sessionReferences, say("session-changed")); else setProblem(problemText(error, failure));
    });
  };
  const hash = bibliography?.content_hash;
  const undo = view?.undo;
  const undoLabel = !undo ? null : undo.kind === "reset" ? say("session-undo-reset")
    : (undo.reference ? say("session-undo-remove-named", { name: isolate(tileName(undo.reference)) }) : say("session-undo-remove"));
  const name = outputName(style);
  return <Sheet title={say("session-title")} closeLabel={say("session-close")} description={say("session-about")} onClose={onClose}
    leadingActions={[
      ...undoLabel ? [{ label: undoLabel, onPress: () => edit({ kind: "undo" }) }] : [],
      ...view && view.rows.length + view.absent > 0 ? [{ label: say("session-reset"), onPress: () => edit({ kind: "reset" }) }] : [],
    ]}
    actions={[
      { label: say("session-save"), disabled: !hash, onPress: () => deliver(async (signal) => {
        const saved = await exportSessionBibliography(style, hash!, signal);
        return saved === null ? null : say("session-saved", { name: isolate(saved) });
      }, say("session-not-saved")) },
      { label: say("session-copy"), primary: true, disabled: !hash, onPress: () => deliver(async (signal) => {
        await copySessionBibliography(style, hash!, signal);
        return say("session-copied");
      }, say("session-not-copied")) },
    ]}>
    {view && view.rows.length === 0 && view.absent === 0 && <p>{say("session-empty")}</p>}
    {view && view.rows.length > 0 && <Group label={say("session-title")}>
      {view.rows.map((row) => <Row item key={idKey(row.id)} label={row.title}
        description={[abbreviatedAuthors(row) ? row.first_author : row.authors, row.year].filter((part) => part !== null && part !== "").join(", ")}>
        <Button size="small" label={say("session-remove")} aria-label={say("session-remove-named", { name: isolate(tileName(row)) })} onPress={() => edit({ kind: "remove", id: row.id })} />
      </Row>)}
    </Group>}
    {view && view.absent > 0 && <Status as="p">{say("session-absent", { count: view.absent })}</Status>}
    {view?.overflowed && <Status as="p">{say("session-full")}</Status>}
    {bibliography && <Section title={say("session-bibliography-in", { style: isolate(name) })} id="session-bibliography" level={3}>
      <SheetOutput>{renderDisplay(bibliography.display)}</SheetOutput>
      {bibliography.warnings.map((warning) => <Status as="p" key={warning}>{warning}</Status>)}
      {bibliography.left_out.length > 0 && <Status as="p">{say("session-left-out", { names: bibliography.left_out.map(isolate).join(", ") })}</Status>}
    </Section>}
    {problem && <Status as="p" role="alert">{problem}</Status>}
    {status && <Status as="p">{status}</Status>}
  </Sheet>;
}
