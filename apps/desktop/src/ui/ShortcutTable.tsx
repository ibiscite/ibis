import type { ReactNode } from "react";
import type { KeyBinding } from "../keys";
import { ariaKeys, keyLabel } from "../keys";
import { useInterfaceMessages } from "../i18n";
import { SHORTCUT_ACTIONS, SHORTCUT_REGIONS } from "../i18n/settings";
import { useKeyPulse } from "./Delights";
import "./ShortcutTable.css";

/** One cap per combination in a key list; plain secondary text where keys are
 * read, not edited (Settings). Alternatives stay apart either way. */
export function ShortcutKeys({ binding, isMac, plain = false }: { binding: KeyBinding; isMac: boolean; plain?: boolean }) {
  const { text } = useInterfaceMessages();
  return <span className="ui-settings-keys">{binding.keys.map((keys, i) => <span className="keys-binding key-caps" key={keys} aria-keyshortcuts={binding.displayLabel ? undefined : ariaKeys(keys, isMac)}>
    {i > 0 && <>{" "}<span>{text("shortcuts-or")}</span>{" "}</>}
    <kbd data-plain={plain || undefined} dir="ltr">{binding.displayLabel ?? keyLabel(keys, isMac)}</kbd>
  </span>)}</span>;
}

function ShortcutRow({ binding, isMac, plain, ids }: { binding: KeyBinding; isMac: boolean; plain: boolean; ids: boolean }) {
  const { text } = useInterfaceMessages();
  const pressed = useKeyPulse(binding.keys, plain);
  return <tr id={ids ? `key-${binding.action}` : undefined} data-echo={pressed || undefined}>
    <th scope="row">{text(binding.label ?? SHORTCUT_ACTIONS[binding.action])}</th>
    <td><ShortcutKeys binding={binding} isMac={isMac} plain={plain} /></td>
  </tr>;
}

/** `plain` is Settings › Shortcuts: only the opening-shortcut recorder edits a
 * key, so every key the table lists is plain secondary text, never a cap. */
export function ShortcutGroup({ region, bindings, isMac, title, children, plain = false, ids = false }: {
  region: KeyBinding["region"]; bindings: readonly KeyBinding[]; isMac: boolean; title?: string; children?: ReactNode; plain?: boolean; ids?: boolean;
}) {
  const { text } = useInterfaceMessages();
  return <tbody id={ids ? `shortcuts-${region}` : undefined}>
    <tr><th scope="rowgroup" colSpan={2}>{title ?? text(SHORTCUT_REGIONS[region])}</th></tr>
    {children}
    {bindings.filter((binding) => binding.region === region).map((binding) => <ShortcutRow key={binding.action} binding={binding} isMac={isMac} plain={plain} ids={ids} />)}
  </tbody>;
}
