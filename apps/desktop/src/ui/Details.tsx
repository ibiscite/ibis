import { useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Button as AriaButton } from "react-aria-components";
import { Icon } from "./Icon";
import "./Layout.css";

/** Rarer rows behind a disclosure row in its own card, its words on the rows'
 * edge and its chevron at the end, as an expander row. Search can open it
 * before focusing a contained row. */
export function Details({ label, reveal, children }: { label: string; reveal?: { id: string; ticket: number } | null; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const content = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (reveal && content.current?.contains(document.getElementById(reveal.id))) setOpen(true);
  }, [reveal]);
  return <div className="ui-details">
    <div className="ui-group"><div className="ui-row ui-choice-row">
      <AriaButton className="ui-choice ui-details-toggle" aria-expanded={open} aria-controls={id} onPress={() => setOpen(!open)}>
        <span className="ui-row-text">{label}</span>
        <span className="ui-details-mark" aria-hidden="true"><Icon name="back" /></span>
      </AriaButton>
    </div></div>
    <div ref={content} id={id} hidden={!open}>{children}</div>
  </div>;
}
