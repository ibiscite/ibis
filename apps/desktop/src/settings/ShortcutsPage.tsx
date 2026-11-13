import { Note } from "../ui/Layout";
import { useLayoutEffect, useState, type ReactNode } from "react";
import { useInterfaceMessages } from "../i18n";
import { SHORTCUT_ACTIONS, SHORTCUT_REGIONS } from "../i18n/settings";
import { shortcutBindings } from "../keys";
import { currentPlatform } from "../platform";
import { filterPalette } from "../ui/Palette";
import { SettingsSearch } from "../ui/SettingsSearch";
import { SHORTCUT_GROUPS, ShortcutGroup } from "../ui/ShortcutTable";

/** `enterPastes` names Enter "Paste" only when Ibis is known to paste. */
export function ShortcutsPage({ opening, enterPastes, reveal, highlighted }: { opening: ReactNode; enterPastes: boolean; reveal?: { id: string; ticket: number } | null; highlighted?: string | null }) {
  const { text } = useInterfaceMessages();
  const [query, setQuery] = useState("");
  useLayoutEffect(() => { if (reveal) setQuery(""); }, [reveal]);
  const matches = (label: string) => filterPalette([{ id: "shortcuts", items: [{ id: "row", label }] }], query).length > 0;
  const bindings = shortcutBindings(enterPastes)
    .filter((binding) => matches(text(binding.label ?? SHORTCUT_ACTIONS[binding.action])));
  const showOpen = ([SHORTCUT_REGIONS.system, SHORTCUT_ACTIONS.open, "shortcuts-open"] as const).some((id) => matches(text(id)));
  return <>
    <SettingsSearch className="ui-shortcut-filter" label={text("shortcuts-filter")} value={query} onChange={setQuery} />
    <table className="ui-shortcut-table" aria-label={text("keys-title")}>
      <thead className="visually-hidden"><tr><th scope="col">{text("keys-action")}</th><th scope="col">{text("keys-shortcut")}</th></tr></thead>
      {SHORTCUT_GROUPS.map((region) => region === "system"
        ? showOpen && <ShortcutGroup key={region} ids region="system" bindings={[]} isMac={currentPlatform() === "macos"}>{opening}</ShortcutGroup>
        : bindings.some((binding) => binding.region === region) && <ShortcutGroup highlighted={highlighted} key={region} region={region} bindings={bindings} isMac={currentPlatform() === "macos"} plain ids />)}
    </table>
    {!showOpen && !bindings.length && <Note role="status">{text("shortcuts-empty")}</Note>}
  </>;
}
