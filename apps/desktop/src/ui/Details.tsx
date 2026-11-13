import { useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Button } from "./Button";

/** A labelled focus group. Search can open it before focusing a contained row. */
export function Details({ label, reveal, children }: { label: string; reveal?: { id: string; ticket: number } | null; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const content = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (reveal && content.current?.contains(document.getElementById(reveal.id))) setOpen(true);
  }, [reveal]);
  return <div className="ui-details">
    <Button variant="quiet" size="small" aria-expanded={open} aria-controls={id} onPress={() => setOpen(!open)}>{label}</Button>
    <div ref={content} id={id} hidden={!open}>{children}</div>
  </div>;
}
