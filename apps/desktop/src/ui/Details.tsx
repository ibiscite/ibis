import { useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Button as AriaButton } from "react-aria-components";
import { Icon } from "./Icon";
import { Group, Section } from "./Layout";

/** Rarer rows behind a disclosure row in its own card, its words on the rows'
 * edge and its chevron at the end, as an expander row. It takes rows, which
 * share the toggle's card (the card draws a divider above each). Hidden rows
 * stay mounted, so search can open it before focusing a contained row. */
export function Details({ label, reveal, children }: { label: string; reveal?: { id: string; ticket: number } | null; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const content = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (reveal && content.current?.contains(document.getElementById(reveal.id))) setOpen(true);
  }, [reveal]);
  return <Section><Group>
    <div className="ui-row ui-choice-row">
      <AriaButton className="ui-choice ui-details-toggle" aria-expanded={open} aria-controls={id} onPress={() => setOpen(!open)}>
        <span className="ui-row-text">{label}</span>
        <span className="ui-details-mark" aria-hidden="true"><Icon name="back" /></span>
      </AriaButton>
    </div>
    <div ref={content} id={id} className="ui-details-body" hidden={!open}>{children}</div>
  </Group></Section>;
}
