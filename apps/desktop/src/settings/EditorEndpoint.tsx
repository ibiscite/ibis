import { useEffect, useRef, useState } from "react";
import type { EndpointProblem, EndpointStatus } from "../bindings";
import { type MessageId, useInterfaceMessages } from "../i18n";
import { Button } from "../ui/Button";
import { SettingsRow as Row } from "../ui/SettingsGroup";
import { Switch } from "../ui/Switch";
import { rowDetails } from "./rows";

export type EditorEndpointApi = {
  editorEndpoint: () => Promise<EndpointStatus>;
  setEditorEndpoint: (on: boolean) => Promise<EndpointStatus>;
  copyEditorEndpoint: () => Promise<null>;
};

const PROBLEMS: Record<EndpointProblem, MessageId> = {
  "port-in-use": "citations-editor-port-in-use", "bad-setting": "citations-editor-bad-setting", "not-saved": "citations-editor-not-saved",
};

/** Settings › Citations › Editor endpoint (#42). Off until switched on; the
 * address holds the token that opens the picker, so it is shown only here. */
export function EditorEndpointRow({ api, highlighted }: { api: EditorEndpointApi; highlighted: boolean }) {
  const { text } = useInterfaceMessages();
  const [status, setStatus] = useState<EndpointStatus | null>(null);
  const [failed, setFailed] = useState(false);
  const [copied, setCopied] = useState(false);
  const asked = useRef(0);
  // Only the newest answer counts: a slow read cannot undo a newer switch.
  const take = (request: Promise<EndpointStatus>) => {
    const ticket = ++asked.current;
    setFailed(false);
    setCopied(false);
    request.then((next) => { if (ticket === asked.current) setStatus(next); }, () => { if (ticket === asked.current) setFailed(true); });
  };
  // biome-ignore lint/correctness/useExhaustiveDependencies: read once when the page opens.
  useEffect(() => { take(api.editorEndpoint()); return () => { asked.current += 1; }; }, [api]);
  const problem = failed ? "citations-editor-not-saved" : status?.problem ? PROBLEMS[status.problem] : undefined;
  return <>
    <Row id="editor-endpoint" {...rowDetails("editor-endpoint", text)} highlighted={highlighted} problem={problem ? text(problem) : undefined}>
      <Switch aria-labelledby="editor-endpoint" aria-describedby="editor-endpoint-description" isDisabled={!status && !failed}
        isSelected={status?.on ?? false} onChange={(on) => take(api.setEditorEndpoint(on))} />
    </Row>
    {status?.on && status.url && <Row id="editor-endpoint-address" label={text("citations-editor-address")} description={text("citations-editor-address-description")}>
      <Button aria-describedby="editor-endpoint-address-description" onPress={() => {
        setCopied(false);
        void api.copyEditorEndpoint().then(() => setCopied(true), () => setFailed(true));
      }}>{text(copied ? "citations-editor-copied" : "citations-editor-copy")}</Button>
    </Row>}
    {status?.on && status.url && <p className="settings-address" dir="ltr"><code>{status.url}</code></p>}
  </>;
}
