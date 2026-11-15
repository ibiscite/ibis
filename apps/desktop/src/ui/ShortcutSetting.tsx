import type { ShortcutStatus } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { SHORTCUT_ACTIONS, type ShortcutProblem } from "../i18n/settings";
import { DEFAULT_PICKER_SHORTCUT, shortcutLabel } from "../keys";
import { currentPlatform } from "../platform";
import { Button } from "./Button";
import { Note, ResetSlot, Row } from "./Layout";
import { ShortcutRecorder } from "./ShortcutRecorder";
import { ShortcutEcho, useKeyPulse } from "./Delights";
import { Status } from "./Status";

/** Both Settings pages edit the same owner. This component only presents it. */
export function ShortcutSetting({ rowId, details, status, problem, saving, highlighted, onRecord, onConfigure, echo = false, table = false }: {
  rowId: string; status: ShortcutStatus | null; problem: ShortcutProblem | null;
  saving: boolean; highlighted: boolean;
  details: { label: string; description?: string };
  onRecord: (shortcut: string | null) => void; onConfigure: () => void;
  echo?: boolean; table?: boolean;
}) {
  const { text } = useInterfaceMessages();
  const pressed = useKeyPulse(status?.shortcut ?? "", table && echo && Boolean(status?.shortcut));
  const mac = currentPlatform() === "macos";
  const defaultLabel = shortcutLabel(DEFAULT_PICKER_SHORTCUT, mac);
  const isDefault = status && /^[a-z]$/i.test(status.shortcut.split("+").at(-1)?.trim() ?? "")
    && shortcutLabel(status.shortcut, mac).split("+").sort().join("+") === defaultLabel.split("+").sort().join("+");
  // A desktop's own shortcut is the system's to reset; ours keeps its room at the default.
  const reset = status?.desktop ? undefined : status && !isDefault ? { label: details.label, disabled: saving, onReset: () => onRecord(null) } : null;
  const controls = <>
      {status?.desktop ? <>
        <Status as="span">{status.shortcut || text("shortcuts-not-assigned")}</Status>
        {status.desktop.configurable && <Button isDisabled={saving} onPress={onConfigure}>{text("shortcuts-change")}</Button>}
      </> : status && <ShortcutRecorder shortcut={status.shortcut} label={text("shortcuts-open")}
        prompt={text("shortcuts-record")} isDisabled={saving} onRecord={onRecord} />}
      {status?.error && <Status as="span">{status.error}</Status>}
</>;
  if (table) return <>
    <tr id={rowId} data-echo={pressed || undefined} data-highlighted={highlighted || undefined}><th scope="row">{text(SHORTCUT_ACTIONS.open)}</th><td><span className="ui-settings-keys"><ResetSlot reset={reset} />{controls}</span></td></tr>
    {(problem || status?.desktop?.guidance) && <tr><td colSpan={2}>
      {status?.desktop?.guidance && <Note>{status.desktop.guidance}</Note>}
      {problem && <Status className="ui-row-problem" role="alert">{text(problem.id, problem.args)}</Status>}
    </td></tr>}
  </>;
  return <ShortcutEcho keys={echo && status ? [status.shortcut] : []}><Row id={rowId} label={details.label} highlighted={highlighted} reset={reset}
      description={status?.desktop?.guidance ?? (status ? details.description : undefined)}
      problem={problem ? text(problem.id, problem.args) : undefined}>
      {controls}
    </Row></ShortcutEcho>;
}
