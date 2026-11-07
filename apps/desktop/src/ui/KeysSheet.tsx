import { useInterfaceMessages } from "../i18n";
import { type KeyBinding } from "../keys";
import { SHORTCUT_GROUPS, ShortcutGroup } from "./ShortcutTable";
import { Sheet } from "./Sheet";
import "./KeysSheet.css";

/** Settings' groups in the same order, less standard menu navigation, which
 * every Mac menu shares; Anywhere in Ibis starts the second column. */
const GROUPS = SHORTCUT_GROUPS.filter((region) => region !== "menu");
const SPLIT = GROUPS.indexOf("picker");

/** The picker's keys in two compact columns of groups, as a sheet. */
export function KeysDialog({ bindings, isMac, onClose }: { bindings: readonly KeyBinding[]; isMac: boolean; onClose: () => void }) {
  const { text } = useInterfaceMessages();
  return <Sheet title={text("keys-title")} closeLabel={text("keys-close")} onClose={onClose}>
    <div className="keys-groups">
      {/* The groups in their one order, read down the first column, then the second. */}
      {[GROUPS.slice(0, SPLIT), GROUPS.slice(SPLIT)].map((regions, column) => <table key={column}>
        <thead className="visually-hidden"><tr><th scope="col">{text("keys-action")}</th><th scope="col">{text("keys-shortcut")}</th></tr></thead>
        {regions.filter((region) => bindings.some((binding) => binding.region === region)).map((region) => <ShortcutGroup key={region} region={region} bindings={bindings} isMac={isMac} />)}
      </table>)}
    </div>
  </Sheet>;
}
