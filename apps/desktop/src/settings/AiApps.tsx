import { Fragment, useEffect, useRef, useState } from "react";
import type { AgentApp } from "../bindings";
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
export function AiApps({ api, highlighted }: { api: Api; highlighted: string | null }) {
  const { text } = useInterfaceMessages();
  const [apps, setApps] = useState<AgentApp[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const sequence = useRef(0);
  const changing = useRef(false);
  useEffect(() => {
    let live = true;
    const read = () => {
      if (changing.current || document.visibilityState === "hidden") return;
      const request = ++sequence.current;
      void api.agentApps().then((next) => {
        if (live && request === sequence.current) { setApps(next); setFailed(false); }
      }, () => { if (live && request === sequence.current) setFailed(true); });
    };
    read();
    window.addEventListener("focus", read);
    return () => { live = false; sequence.current += 1; window.removeEventListener("focus", read); };
  }, [api.agentApps]);
  const change = (app: AgentApp) => {
    if (changing.current) return;
    const request = ++sequence.current;
    changing.current = true;
    setBusy(true); setFailed(false);
    const operation = app.state === "connected" ? api.disconnectAgentApp(app.id, app.revision) : api.connectAgentApp(app.id, app.state === "different", app.revision);
    void operation.then((next) => {
      if (request === sequence.current) setApps(next);
    }, () => { if (request === sequence.current) setFailed(true); }).finally(() => {
      changing.current = false;
      if (request === sequence.current) setBusy(false);
    });
  };
  return <Section id="agent-apps" title={text("settings-ai-apps")}>
    <Group>
      {!apps && <Row label={text("ai-apps-checking")} pending={!failed} />}
      {apps?.map((app) => {
        const description = app.state === "blocked" || app.state === "by-hand" ? app.detail : text(STATES[app.state]);
        const action = app.state in ACTIONS ? ACTIONS[app.state as keyof typeof ACTIONS] : null;
        return <Row key={app.id} id={`agent-app-${app.id}`} label={app.name} description={<>
          {description}
          {app.needs_command && !app.ibis_on_path && app.state !== "blocked" && app.state !== "plugin" && <> {text("ai-apps-command-needed")}</>}
        </>} highlighted={highlighted === "agent-apps"}>
          {action && <Button isDisabled={busy} onPress={() => change(app)}>{text(action)}</Button>}
          {app.state === "different" && app.diff && <Note><code dir="ltr">{app.diff.split("\n").map((line, index) => <Fragment key={`${index}:${line}`}>{index > 0 && <br />}<span>{line}</span></Fragment>)}</code></Note>}
        </Row>;
      })}
    </Group>
    {failed && <Note problem role="alert">{text("ai-apps-failed")}</Note>}
    <Note>{text("ai-apps-kept")}</Note>
  </Section>;
}
