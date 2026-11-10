import { Note } from "../ui/Layout";
import { useState, type ReactNode } from "react";
import { useInterfaceMessages } from "../i18n";
import { SHORTCUT_ACTIONS, SHORTCUT_REGIONS } from "../i18n/settings";
import { KEY_TABLE } from "../keys";
import { currentPlatform } from "../platform";
import { SettingsSearch } from "../ui/SettingsSearch";
import { SHORTCUT_GROUPS, ShortcutGroup } from "../ui/ShortcutTable";

/** `enterPastes` names Enter "Paste" only when Ibis is known to paste. */
export function ShortcutsPage({ opening, enterPastes }: { opening: ReactNode; enterPastes: boolean }) {
  const { text } = useInterfaceMessages();
  const [query, setQuery] = useState("");
  const matches = (label: string) => label.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase());
  const bindings = KEY_TABLE.map((binding) => binding.action === "copy" && !enterPastes ? { ...binding, label: "shortcuts-action-copy-only" as const } : binding)
    .filter((binding) => matches(text(binding.label ?? SHORTCUT_ACTIONS[binding.action])));
  const showOpen = ([SHORTCUT_REGIONS.system, SHORTCUT_ACTIONS.open, "shortcuts-open"] as const).some((id) => matches(text(id)));
  return <>
    <SettingsSearch className="ui-shortcut-filter" label={text("shortcuts-filter")} value={query} onChange={setQuery} />
    <table className="ui-shortcut-table" aria-label={text("keys-title")}>
      <thead className="visually-hidden"><tr><th scope="col">{text("keys-action")}</th><th scope="col">{text("keys-shortcut")}</th></tr></thead>
      {SHORTCUT_GROUPS.map((region) => region === "system"
        ? showOpen && <ShortcutGroup key={region} ids region="system" bindings={[]} isMac={currentPlatform() === "macos"}>{opening}</ShortcutGroup>
        : bindings.some((binding) => binding.region === region) && <ShortcutGroup key={region} region={region} bindings={bindings} isMac={currentPlatform() === "macos"} plain ids />)}
    </table>
    {!showOpen && !bindings.length && <Note role="status">{text("shortcuts-empty")}</Note>}
  </>;
}
