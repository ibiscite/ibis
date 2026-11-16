import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import type { commands, ProposalResult, WaitingProposal } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { Button } from "../ui/Button";
import { Buttons, Group, Note, Row, Section } from "../ui/Layout";
import { useSettingsRequests } from "./useSettingsRequests";

type Api = Pick<typeof commands, "waitingProposals" | "applyProposal" | "rejectProposal" | "rejectAppProposals">;
type Shown = WaitingProposal & { result?: ProposalResult; error?: string };

/** Applied rows keep their receipt until leaving this page. */
export function ProposedChanges({ api, highlighted }: { api: Api; highlighted: string | null }) {
  const { text, locale } = useInterfaceMessages();
  const [proposals, setProposals] = useState<Shown[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const { read: reload, begin, isPending } = useSettingsRequests(api.waitingProposals, (next) => {
    setProposals((previous) => {
      const receipts = previous?.filter((row) => row.result?.status === "applied") ?? [];
      const ids = new Set(receipts.map((row) => row.id));
      return [...next.filter((row) => !ids.has(row.id)).map((row) => ({ ...previous?.find((old) => old.id === row.id), ...row })), ...receipts];
    });
    setFailed(false);
  }, () => setFailed(true));
  const pendingRead = useRef(false);
  const read = useCallback(() => {
    if (document.visibilityState === "hidden") return;
    if (isPending()) pendingRead.current = true;
    else reload();
  }, [reload, isPending]);
  useEffect(() => {
    read();
    window.addEventListener("focus", read);
    document.addEventListener("visibilitychange", read);
    return () => { window.removeEventListener("focus", read); document.removeEventListener("visibilitychange", read); };
  }, [read]);
  const change = (proposal: Shown, action: "apply" | "reject" | "reject-app") => {
    if (isPending()) return;
    const request = begin(proposal.id);
    setBusy(true);
    const update = (patch: Partial<Shown>) => setProposals((rows) => rows?.map((row) => row.id === proposal.id ? { ...row, ...patch } : row) ?? null);
    update({ error: undefined });
    const operation = action === "apply" ? api.applyProposal(proposal.id) : action === "reject" ? api.rejectProposal(proposal.id) : api.rejectAppProposals(proposal.id);
    void operation.then((result) => {
      if (!request.current()) return;
      if (result.status === "rejected") setProposals((rows) => rows?.filter((row) => (action === "reject-app" ? row.source_app !== proposal.source_app || row.result?.status === "applied" : row.id !== proposal.id)) ?? null);
      else update({ result });
    }, (error: unknown) => {
      if (request.current()) update({ error: typeof error === "string" ? error : text("proposals-failed") });
    }).finally(() => {
      if (!request.current()) return;
      setBusy(false);
      request.finish();
      if (pendingRead.current) { pendingRead.current = false; read(); }
    });
  };
  const apps = new Map<string, { proposal: Shown; count: number }>();
  for (const proposal of proposals ?? []) {
    if (proposal.result?.status === "applied") continue;
    const app = apps.get(proposal.source_app);
    if (app) app.count++;
    else apps.set(proposal.source_app, { proposal, count: 1 });
  }
  return <Section id="proposed-changes" title={text("proposals-title")}>
    <Group>
      {!proposals && <Row label={text("proposals-checking")} pending={!failed} />}
      {proposals?.length === 0 && <Row label={text("proposals-empty")} highlighted={highlighted === "proposed-changes"} />}
      {proposals?.map((proposal) => {
        const id = `proposal-${proposal.id}`;
        const applied = proposal.result?.status === "applied";
        const refusal = proposal.error ?? (proposal.result?.status === "refused" ? proposal.result.sentence : proposal.refusal);
        const preview = proposal.before || proposal.after
          ? [...(proposal.before?.split("\n").map((line) => `- ${line}`) ?? []), ...(proposal.after?.split("\n").map((line) => `+ ${line}`) ?? [])].join("\n")
          : proposal.lines.join("\n");
        return <Row key={proposal.id} id={id} label={text("proposals-from", { app: proposal.source_app, library: proposal.library })}
          description={<>{proposal.summary} · {new Date(proposal.created_ms).toLocaleString(locale)}</>} highlighted={highlighted === "proposed-changes"}>
          {!applied && <Buttons>
            <Button aria-labelledby={`${id}-apply ${id}`} isDisabled={busy} onPress={() => change(proposal, "apply")}><span id={`${id}-apply`}>{text("proposals-apply")}</span></Button>
            <Button aria-labelledby={`${id}-reject ${id}`} isDisabled={busy} onPress={() => change(proposal, "reject")}><span id={`${id}-reject`}>{text("proposals-reject")}</span></Button>
          </Buttons>}
          {preview && <Note><code dir="ltr">{preview.split("\n").map((line, index) => <Fragment key={`${index}:${line}`}>{index > 0 && <br />}<span>{line}</span></Fragment>)}</code></Note>}
          {proposal.result?.status === "applied" ? <Note role="status">{text("proposals-applied", { name: proposal.result.backup })}</Note>
            : refusal && <Note problem role="alert">{refusal}</Note>}
        </Row>;
      })}
      {[...apps.values()].filter(({ count }) => count > 1).map(({ proposal, count }) => {
        const id = `proposal-app-${proposal.id}`;
        return <Row key={proposal.source_app} id={id} label={text("proposals-app", { app: proposal.source_app, count })}
          highlighted={highlighted === "proposed-changes"}>
          <Button aria-labelledby={`${id}-reject ${id}`} isDisabled={busy} onPress={() => change(proposal, "reject-app")}>
            <span id={`${id}-reject`}>{text("proposals-reject-app")}</span>
          </Button>
        </Row>;
      })}
    </Group>
    {failed && <Note problem role="alert">{text("proposals-read-failed")}</Note>}
  </Section>;
}
