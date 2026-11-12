import { useEffect, useRef, useState } from "react";
import type { CommandStatus } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { Button } from "../ui/Button";
import { Buttons, Group, Note, Row, Section } from "../ui/Layout";
import { rowDetails } from "./rows";

type Api = {
  cliStatus: () => Promise<CommandStatus>;
  setCliInstalled: (installed: boolean) => Promise<CommandStatus>;
  copyCliProfile: () => Promise<unknown>;
};

export function CliCommandRow({ api, highlighted }: { api: Api; highlighted: boolean }) {
  const { text } = useInterfaceMessages();
  const [status, setStatus] = useState<CommandStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [copied, setCopied] = useState(false);
  const sequence = useRef(0);
  const changing = useRef(false);
  useEffect(() => {
    let live = true;
    const read = () => {
      if (changing.current) return;
      const request = ++sequence.current;
      void api.cliStatus().then((next) => { if (live && request === sequence.current) { setStatus(next); setFailed(false); } }, () => { if (live && request === sequence.current) setFailed(true); });
    };
    read();
    const focus = () => { if (document.visibilityState !== "hidden") read(); };
    window.addEventListener("focus", focus);
    return () => { live = false; sequence.current += 1; window.removeEventListener("focus", focus); };
  }, [api.cliStatus]);
  const change = (installed: boolean) => {
    const request = ++sequence.current;
    changing.current = true;
    setBusy(true); setFailed(false); setCopied(false);
    void api.setCliInstalled(installed).then((next) => {
      if (request === sequence.current) setStatus(next);
    }, () => { if (request === sequence.current) setFailed(true); }).finally(() => {
      changing.current = false;
      if (request === sequence.current) setBusy(false);
    });
  };
  const description = !status ? text("command-checking") : text({
    "not-installed": "command-missing", matching: "command-matching", different: "command-different", foreign: "command-foreign",
  }[status.state] as "command-missing" | "command-matching" | "command-different" | "command-foreign", { version: status.version ?? "", path: status.used_path ?? "" });
  return <Section id="command"><Group>
    <Row id="cli-command" {...rowDetails("cli-command", text)} description={description} highlighted={highlighted} problem={failed ? text("command-failed") : undefined}>
      <Buttons>
        {status?.installable && <Button isDisabled={busy} onPress={() => change(true)}>{text("command-install")}</Button>}
        {status?.removable && <Button isDisabled={busy} onPress={() => change(false)}>{text("command-remove")}</Button>}
      </Buttons>
    </Row>
    {status?.used_path && status.state !== "foreign" && <Note><code dir="ltr">{status.used_path}</code></Note>}
    {status?.profile_line && <Row label={text("command-profile")} description={<code dir="ltr">{status.profile_line}</code>}>
      <Button isDisabled={busy} onPress={() => {
        const request = ++sequence.current;
        void api.copyCliProfile().then(() => { if (request === sequence.current) setCopied(true); }, () => { if (request === sequence.current) setFailed(true); });
      }}>{text(copied ? "command-copied" : "command-copy")}</Button>
    </Row>}
  </Group></Section>;
}
