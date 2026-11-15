import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import type { AgentApp, Problem } from "../bindings";
import { type MessageId, useInterfaceMessages } from "../i18n";
import { Button } from "../ui/Button";
import { Group, Note, Row, Section } from "../ui/Layout";

type Api = {
  agentApps: () => Promise<AgentApp[]>;
  connectAgentApp: (id: string, replace: boolean, expected: string) => Promise<AgentApp[]>;
  disconnectAgentApp: (id: string, expected: string) => Promise<AgentApp[]>;
};
const STATES = {
  "not-connected": "ai-apps-not-connected", connected: "ai-apps-connected", stale: "ai-apps-stale",
  different: "ai-apps-different", plugin: "ai-apps-plugin",
} as const satisfies Partial<Record<AgentApp["state"], MessageId>>;
const ACTIONS = {
  "not-connected": "ai-apps-connect", connected: "ai-apps-disconnect", stale: "ai-apps-repair", different: "ai-apps-replace",
} as const satisfies Partial<Record<AgentApp["state"], MessageId>>;

/** Visit/focus reads and changes share one sequence: an old observation cannot
 * replace the list returned by a newer action. This component exists only on its page. */
export function AiApps({ api, highlighted, refresh = 0 }: { api: Api; highlighted: string | null; refresh?: number }) {
  const { text } = useInterfaceMessages();
  const [apps, setApps] = useState<AgentApp[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<{ id?: string; detail?: string } | null>(null);
  const sequence = useRef(0);
  const changing = useRef(false);
  const live = useRef(false);
  const pendingRead = useRef(false);
  useEffect(() => {
    live.current = true;
    return () => { live.current = false; sequence.current += 1; };
  }, []);
  const read = useCallback(() => {
    if (!live.current || document.visibilityState === "hidden") return;
    if (changing.current) { pendingRead.current = true; return; }
    const request = ++sequence.current;
    void api.agentApps().then((next) => {
      if (request === sequence.current) { setApps(next); setFailed((previous) => previous?.detail ? previous : null); }
    }, () => { if (request === sequence.current) setFailed((previous) => previous ?? {}); });
  }, [api.agentApps]);
  useEffect(() => {
    // A command install/remove is another observation trigger on this page.
    void refresh;
    read();
    // A window opened hidden reads once it is shown, even without focus.
    window.addEventListener("focus", read);
    document.addEventListener("visibilitychange", read);
    return () => { window.removeEventListener("focus", read); document.removeEventListener("visibilitychange", read); };
  }, [read, refresh]);
  const change = (app: AgentApp) => {
    if (changing.current) return;
    const request = ++sequence.current;
    changing.current = true;
    setBusy(true); setFailed(null);
    const operation = app.state === "connected" ? api.disconnectAgentApp(app.id, app.revision) : api.connectAgentApp(app.id, app.state === "different", app.revision);
    void operation.then((next) => {
      if (request === sequence.current) setApps(next);
    }, (error: unknown) => {
      if (request === sequence.current) {
        const problem = typeof error === "object" && error !== null ? error as Partial<Problem> : null;
        setFailed({ id: app.id, detail: problem?.message?.args?.detail });
      }
    }).finally(() => {
      changing.current = false;
      if (request === sequence.current) setBusy(false);
      if (pendingRead.current) { pendingRead.current = false; read(); }
    });
  };
  return <Section id="agent-apps" title={text("settings-ai-apps")}>
    <Group>
      {!apps && <Row label={text("ai-apps-checking")} pending={!failed} />}
      {apps?.map((app) => {
        const description = app.state === "blocked" || app.state === "by-hand" ? app.detail : text(STATES[app.state]);
        const action = app.state in ACTIONS ? ACTIONS[app.state as keyof typeof ACTIONS] : null;
        const detail = failed?.id === app.id ? failed.detail : undefined;
        const note = detail ?? (app.state === "different" ? app.diff : null);
        return <Row key={app.id} id={`agent-app-${app.id}`} label={app.name} description={<>
          {description}
          {app.needs_command && !app.ibis_on_path && app.state !== "plugin" && <> {text("ai-apps-command-needed")}</>}
        </>} highlighted={highlighted === "agent-apps"}>
          {action && <Button aria-labelledby={`agent-app-${app.id}-action agent-app-${app.id}`} isDisabled={busy} onPress={() => change(app)}><span id={`agent-app-${app.id}-action`}>{text(action)}</span></Button>}
          {note && <Note problem={Boolean(detail)}><code dir="ltr">{note.split("\n").map((line, index) => <Fragment key={`${index}:${line}`}>{index > 0 && <br />}<span>{line}</span></Fragment>)}</code></Note>}
        </Row>;
      })}
    </Group>
    {failed && <Note problem role="alert">{text("ai-apps-failed")}</Note>}
    <Note>{text("ai-apps-kept")}</Note>
  </Section>;
}
