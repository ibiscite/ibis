import type { ReactNode } from "react";
import type { KeyBinding } from "../keys";
import { ariaKeys, keyLabel } from "../keys";
import { useInterfaceMessages } from "../i18n";
import { SHORTCUT_ACTIONS, SHORTCUT_REGIONS } from "../i18n/settings";
import { useKeyPulse } from "./Delights";
import "./ShortcutTable.css";

/** Read-only combinations are one cap each; alternatives keep separate caps. */
export function ShortcutKeys({ binding, isMac, system = false }: { binding: KeyBinding; isMac: boolean; system?: boolean }) {
  const { text } = useInterfaceMessages();
  return <span className="ui-settings-keys">{system && <span className="ui-system-note">{text("shortcuts-system")}</span>}{binding.keys.map((keys, i) => <span className="keys-binding key-caps" key={keys} aria-keyshortcuts={binding.displayLabel ? undefined : ariaKeys(keys, isMac)}>
    {i > 0 && <>{" "}<span>{text("shortcuts-or")}</span>{" "}</>}
    <kbd data-system={system || undefined} dir="ltr">{binding.displayLabel ?? keyLabel(keys, isMac)}</kbd>
  </span>)}</span>;
}

function ShortcutRow({ binding, isMac, system, ids, echo }: { binding: KeyBinding; isMac: boolean; system: boolean; ids: boolean; echo: boolean }) {
  const { text } = useInterfaceMessages();
  const pressed = useKeyPulse(binding.keys, echo);
  return <tr id={ids ? `key-${binding.action}` : undefined} data-echo={pressed || undefined}>
    <th scope="row">{text(binding.label ?? SHORTCUT_ACTIONS[binding.action])}</th>
    <td><ShortcutKeys binding={binding} isMac={isMac} system={system} /></td>
  </tr>;
}

export function ShortcutGroup({ region, bindings, isMac, title, children, systemNotes = false, ids = false }: {
  region: KeyBinding["region"]; bindings: readonly KeyBinding[]; isMac: boolean; title?: string; children?: ReactNode; systemNotes?: boolean; ids?: boolean;
}) {
  const { text } = useInterfaceMessages();
  const fixed = new Set(["copy", "move", "escape", "settings", "quit", "menuMove", "menuEnds", "menuChoose", "menuLeave"]);
  return <tbody id={ids ? `shortcuts-${region}` : undefined}>
    <tr><th scope="rowgroup" colSpan={2}>{title ?? text(SHORTCUT_REGIONS[region])}</th></tr>
    {children}
    {bindings.filter((binding) => binding.region === region).map((binding) => <ShortcutRow key={binding.action} binding={binding} isMac={isMac} system={systemNotes && fixed.has(binding.action)} ids={ids} echo={systemNotes} />)}
  </tbody>;
}
