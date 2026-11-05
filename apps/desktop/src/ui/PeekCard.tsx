import type { RefObject } from "react";
import { Popover } from "react-aria-components";
import { useLayer } from "./layers";
import "./PeekCard.css";

/** Quick Look for one item: its full details in a card under it, without taking
 * focus, so typing carries on in the search. ⌘Y again or Esc closes it. */
export function PeekCard({ triggerRef, open, label, title, byline, facts, onClose }: {
  triggerRef: RefObject<Element | null>; open: boolean; label: string; title: string; byline?: string | null;
  /** Label and value rows; `mono` for identifiers such as a citation key. */
  facts: readonly { label: string; value: string; mono?: boolean }[];
  onClose: () => void;
}) {
  useLayer("menu", open, () => { onClose(); });
  return <Popover triggerRef={triggerRef} isOpen={open} isNonModal placement="bottom start" offset={4} containerPadding={8}
    className="peek-card ui-overlay" onOpenChange={(next) => { if (!next) onClose(); }}>
    <section aria-label={label} aria-live="polite">
      <h3 className="peek-title"><bdi dir="auto">{title}</bdi></h3>
      {byline && <p className="peek-byline"><bdi dir="auto">{byline}</bdi></p>}
      <dl className="peek-facts">
        {facts.map((fact) => <div key={fact.label}><dt>{fact.label}</dt><dd data-mono={fact.mono || undefined}><bdi dir={fact.mono ? "ltr" : "auto"}>{fact.value}</bdi></dd></div>)}
      </dl>
    </section>
  </Popover>;
}
