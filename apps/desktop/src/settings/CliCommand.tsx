import { useEffect, useState } from "react";
import type { CommandStatus } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { Button } from "../ui/Button";
import { Buttons, Row } from "../ui/Layout";
import { rowDetails } from "./rows";
import { useSettingsRequests } from "./useSettingsRequests";

type Api = {
  cliStatus: () => Promise<CommandStatus>;
  setCliInstalled: (installed: boolean) => Promise<CommandStatus>;
  copyCliProfile: () => Promise<unknown>;
};

export function CliCommandRow({ api, highlighted, onChanged }: { api: Api; highlighted: boolean; onChanged?: () => void }) {
  const { text } = useInterfaceMessages();
  const [status, setStatus] = useState<CommandStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [copied, setCopied] = useState(false);
  const { read, begin } = useSettingsRequests(api.cliStatus, (next) => { setStatus(next); setFailed(false); }, () => setFailed(true));
  useEffect(() => {
    read();
    const focus = () => { if (document.visibilityState !== "hidden") read(); };
    window.addEventListener("focus", focus);
    return () => window.removeEventListener("focus", focus);
  }, [read]);
  const change = (installed: boolean) => {
    const request = begin("command");
    setBusy(true); setFailed(false); setCopied(false);
    void api.setCliInstalled(installed).then((next) => {
      if (request.current()) { setStatus(next); onChanged?.(); }
    }, () => { if (request.current()) setFailed(true); }).finally(() => {
      if (request.current()) setBusy(false);
      request.finish();
    });
  };
  const description = !status ? text("command-checking") : text({
    "not-installed": "command-missing", matching: "command-matching", different: "command-different", foreign: "command-foreign",
  }[status.state] as "command-missing" | "command-matching" | "command-different" | "command-foreign", { version: status.version ?? "", path: status.used_path ?? "" });
  return <>
    <Row id="cli-command" {...rowDetails("cli-command", text)} description={description} highlighted={highlighted} pending={!status && !failed} problem={failed ? text("command-failed") : undefined}
      note={status?.used_path && status.state !== "foreign" && <code dir="ltr">{status.used_path}</code>}>
      <Buttons>
        {status?.installable && <Button isDisabled={busy} aria-label={text("command-install")} onPress={() => change(true)}>{text("command-install-button")}</Button>}
        {status?.removable && <Button isDisabled={busy} onPress={() => change(false)}>{text("command-remove")}</Button>}
      </Buttons>
    </Row>
    {status?.profile_line && <Row label={text("command-profile")} description={<code dir="ltr">{status.profile_line}</code>}>
      <Button isDisabled={busy} onPress={() => {
        const request = begin();
        void api.copyCliProfile().then(() => { if (request.current()) setCopied(true); }, () => { if (request.current()) setFailed(true); });
      }}>{text(copied ? "command-copied" : "command-copy")}</Button>
    </Row>}
  </>;
}
