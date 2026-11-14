import { useEffect, useRef, useState } from "react";
import type { EndpointProblem, EndpointStatus } from "../bindings";
import { type MessageId, useInterfaceMessages } from "../i18n";
import { Button } from "../ui/Button";
import { Note, Row } from "../ui/Layout";
import { Switch } from "../ui/Switch";
import { rowDetails } from "./rows";
import { useSettingUndo } from "../ui/useSettingUndo";

export type EditorEndpointApi = {
  editorEndpoint: () => Promise<EndpointStatus>;
  setEditorEndpoint: (on: boolean) => Promise<EndpointStatus>;
  copyEditorEndpoint: () => Promise<null>;
  onEditorEndpoint?: (listener: (status: EndpointStatus) => void) => Promise<() => void>;
};

const PROBLEMS: Record<EndpointProblem, MessageId> = {
  "port-in-use": "citations-editor-port-in-use", "bad-setting": "citations-editor-bad-setting", "not-saved": "citations-editor-not-saved",
};
const sameStatus = (a: EndpointStatus, b: EndpointStatus) => a.on === b.on && a.url === b.url && a.problem === b.problem;

/** Settings › Citations › Editor endpoint (#42). Off until switched on; the
 * address holds the token that opens the picker, so it is shown only here. */
export function EditorEndpointRow({ api, highlighted, resetSetting, onEdit }: {
  api: EditorEndpointApi; highlighted: boolean;
  resetSetting?: ReturnType<typeof useSettingUndo>["reset"]; onEdit?: () => void;
}) {
  const { text } = useInterfaceMessages();
  const [status, setStatus] = useState<EndpointStatus | null>(null);
  const [failed, setFailed] = useState(false);
  const [copied, setCopied] = useState(false);
  const asked = useRef(0);
  const latestStatus = useRef<EndpointStatus | null>(null);
  const pending = useRef<{ ticket: number; expected: boolean; observed: EndpointStatus | null } | null>(null);
  const mounted = useRef(true);
  const fallback = useRef<HTMLElement | null>(null);
  const localUndo = useSettingUndo(false, fallback);
  const reset = resetSetting ?? localUndo.reset;
  // Only the newest answer counts: a slow read cannot undo a newer switch.
  const take = (request: () => Promise<EndpointStatus>, expected?: boolean) => {
    const ticket = ++asked.current;
    pending.current = expected === undefined ? null : { ticket, expected, observed: null };
    if (mounted.current) { setFailed(false); setCopied(false); }
    return request().then((next) => {
      if (!mounted.current || ticket !== asked.current) throw new Error("Endpoint change superseded");
      const observed = pending.current?.observed;
      if (observed && !sameStatus(observed, next)) {
        asked.current += 1;
        onEdit?.(); localUndo.clear();
        throw new Error("Endpoint change superseded");
      }
      latestStatus.current = next; setStatus(next);
      if (expected !== undefined && (next.on !== expected || next.problem)) throw new Error("Endpoint change refused");
    }).catch((error: unknown) => { if (mounted.current && ticket === asked.current) setFailed(true); throw error; })
      .finally(() => { if (pending.current?.ticket === ticket) pending.current = null; });
  };
  // biome-ignore lint/correctness/useExhaustiveDependencies: read once when the page opens.
  useEffect(() => { mounted.current = true; void take(api.editorEndpoint).catch(() => undefined); return () => { mounted.current = false; asked.current += 1; }; }, [api.editorEndpoint]);
  useEffect(() => {
    let alive = true;
    const events = api.onEditorEndpoint?.((next) => {
      if (!alive) return;
      // An acknowledgement of this save may arrive before its command reply.
      // A different live choice retires the operation and its possible Undo.
      if (pending.current?.expected !== next.on || next.problem) {
        asked.current += 1;
        pending.current = null;
        const previous = latestStatus.current;
        if (!previous || !sameStatus(previous, next)) {
          onEdit?.(); localUndo.clear();
        }
      } else pending.current.observed = next;
      latestStatus.current = next;
      setStatus(next); setFailed(false); setCopied(false);
    });
    return () => { alive = false; void events?.then((stop) => stop()); };
  }, [api.onEditorEndpoint, onEdit, localUndo.clear]);
  const change = (on: boolean, restoring = false) => {
    if (!restoring) { onEdit?.(); localUndo.clear(); }
    return take(() => api.setEditorEndpoint(on), on);
  };
  const problem = status?.problem ? PROBLEMS[status.problem] : failed ? "citations-editor-not-saved" : undefined;
  return <>
    <Row id="editor-endpoint" {...rowDetails("editor-endpoint", text)} highlighted={highlighted} problem={problem ? text(problem) : undefined}
      reset={status?.on ? { label: rowDetails("editor-endpoint", text).label,
        onReset: () => { void reset(rowDetails("editor-endpoint", text).label, () => change(false), () => change(true, true)); } } : undefined}>
      <Switch aria-labelledby="editor-endpoint" aria-describedby="editor-endpoint-description" isDisabled={!status && !failed}
        isSelected={status?.on ?? false} onChange={(on) => { void change(on).catch(() => undefined); }} />
    </Row>
    {status?.on && status.url && <Row id="editor-endpoint-address" label={text("citations-editor-address")} description={text("citations-editor-address-description")}>
      <Button aria-describedby="editor-endpoint-address-description" onPress={() => {
        setCopied(false);
        void api.copyEditorEndpoint().then(() => setCopied(true), () => setFailed(true));
      }}>{text(copied ? "citations-editor-copied" : "citations-editor-copy")}</Button>
      <Note><code dir="ltr">{status.url}</code></Note>
    </Row>}
    {!resetSetting && localUndo.notice}
  </>;
}
