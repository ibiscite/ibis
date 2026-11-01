import type { ShortcutStatus } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { SHORTCUT_COMMANDS, type ShortcutProblem } from "../i18n/settings";
import { DEFAULT_PICKER_SHORTCUT, shortcutLabel } from "../keys";
import { currentPlatform } from "../platform";
import { Button } from "./Button";
import { SettingsGroup, SettingsRow } from "./SettingsGroup";
import { ShortcutRecorder } from "./ShortcutRecorder";
import { ShortcutEcho, useKeyPulse } from "./Delights";

/** Both Settings pages edit the same owner. This component only presents it. */
export function ShortcutSetting({ id, rowId, status, problem, saving, highlighted, onRecord, onConfigure, echo = false, table = false }: {
  id: string; rowId: string; status: ShortcutStatus | null; problem: ShortcutProblem | null;
  saving: boolean; highlighted: boolean;
  onRecord: (shortcut: string | null) => void; onConfigure: () => void;
  echo?: boolean; table?: boolean;
}) {
  const { text } = useInterfaceMessages();
  const pressed = useKeyPulse(status?.shortcut ?? "", table && echo && Boolean(status?.shortcut));
  const mac = currentPlatform() === "macos";
  const defaultLabel = shortcutLabel(DEFAULT_PICKER_SHORTCUT, mac);
  const isDefault = status && /^[a-z]$/i.test(status.shortcut.split("+").at(-1)?.trim() ?? "")
    && shortcutLabel(status.shortcut, mac).split("+").sort().join("+") === defaultLabel.split("+").sort().join("+");
  const controls = <>
      {status?.desktop ? <>
        <span role="status">{status.shortcut || text("shortcuts-not-assigned")}</span>
        {status.desktop.configurable && <Button isDisabled={saving} onPress={onConfigure}>{text("shortcuts-change")}</Button>}
      </> : status && <ShortcutRecorder shortcut={status.shortcut} label={text("shortcuts-open")}
        prompt={text("shortcuts-record")} isDisabled={saving} onRecord={onRecord} />}
      {status?.error && <span role="status">{status.error}</span>}
</>;
  if (table) return <>
    <tr id={rowId} data-echo={pressed || undefined} data-highlighted={highlighted || undefined}><th scope="row">{text("shortcuts-open")}</th><td><span className="ui-settings-keys">{controls}
      {status && !status.desktop && !isDefault && <Button size="small" isDisabled={saving} onPress={() => onRecord(null)}>{text("shortcuts-use-default")}</Button>}
    </span></td></tr>
    {(problem || status?.desktop?.guidance) && <tr><td colSpan={2}>
      {status?.desktop?.guidance && <p className="ui-settings-description">{status.desktop.guidance}</p>}
      {problem && <p className="ui-settings-row-problem" role="alert">{text(problem.id, problem.command ? { command: text(SHORTCUT_COMMANDS[problem.command]) } : undefined)}</p>}
    </td></tr>}
  </>;
  return <SettingsGroup id={id}>
    <ShortcutEcho keys={echo && status ? [status.shortcut] : []}><SettingsRow id={rowId} label={text("shortcuts-open")} highlighted={highlighted}
      description={status?.desktop?.guidance ?? (status ? text("shortcuts-description") : undefined)}
      problem={problem ? text(problem.id, problem.command ? { command: text(SHORTCUT_COMMANDS[problem.command]) } : undefined) : undefined}>
      {controls}
    </SettingsRow></ShortcutEcho>
    {status && !status.desktop && !isDefault && <SettingsRow id="default-shortcut" label={defaultLabel}>
      <Button isDisabled={saving} onPress={() => onRecord(null)}>{text("shortcuts-use-default")}</Button>
    </SettingsRow>}

  </SettingsGroup>;
}
