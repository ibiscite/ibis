import type { SearchResult } from "../../api";
import { say } from "../../i18n";
import { Drawer } from "../../ui/Drawer";
import { Status } from "../../ui/Status";
import { rowWords } from "./ResultList";

/** Peek (⌘Y), as Quick Look: the highlighted reference's whole record, in the
 * drawer under the list, so it covers no row and never takes focus; typing
 * carries on. What the row shortens (every author, the full container, the
 * type and key) is here whole. ⌘Y again, Esc or a new highlight closes it. */
export function PeekDrawer({ record, onClose }: { record: SearchResult; onClose: () => void }) {
  const title = record.title || record.key;
  const type = rowWords.itemType(record.item_type);
  const facts = [
    ...record.container ? [{ label: say("peek-container"), value: record.container }] : [],
    { label: say("peek-year"), value: record.year ? String(record.year) : say("picker-no-date") },
    ...type ? [{ label: say("peek-type"), value: type }] : [],
    { label: say("peek-key"), value: record.key, mono: true },
  ];
  return <Drawer kind="menu" role="region" className="peek-drawer" label={say("peek-label", { title })} onClose={() => { onClose(); }}
    lead={<Status as="div" className="ui-drawer-heading">
      <h3><bdi dir="auto">{title}</bdi></h3>
      {record.authors && <p><bdi dir="auto">{record.authors}</bdi></p>}
    </Status>}>
    <dl className="ui-drawer-facts">
      {facts.map((fact) => <div key={fact.label}><dt>{fact.label}</dt><dd data-mono={fact.mono || undefined}><bdi dir={fact.mono ? "ltr" : "auto"}>{fact.value}</bdi></dd></div>)}
    </dl>
  </Drawer>;
}
