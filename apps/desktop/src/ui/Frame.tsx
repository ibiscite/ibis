import { createContext, useCallback, useContext, useLayoutEffect, useState, type HTMLAttributes, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Band } from "./Band";
import "./Frame.css";

type FrameSlots = { notices: ReactNode; heading: HTMLElement | null; host: (on: boolean) => void };
const Slots = createContext<FrameSlots | null>(null);

/** [C24] The picker window's frame: fixed slots in a fixed order, the search
 * band, the list's heading line, the content (with the Inspector beside it), the
 * shelf and the footer (ui/Frame.css). The Frame owns every notice and shows the
 * first one where there is already room, so nothing moves and nothing is
 * reserved empty: over the list's heading line, which it replaces until the
 * notice passes, or, with no list, in the room above an empty state
 * (`FrameNotices`). Callers never place a notice themselves. */
export function Frame({ search, notice, children, ...main }: {
  search: ReactNode;
  /** The notices, most important first. */
  notice: ReactNode;
  children: ReactNode;
} & HTMLAttributes<HTMLElement>) {
  const [heading, setHeading] = useState<HTMLElement | null>(null);
  const [hosts, setHosts] = useState(0);
  const host = useCallback((on: boolean) => setHosts((count) => count + (on ? 1 : -1)), []);
  return <Slots.Provider value={{ notices: notice, heading, host }}>
    <main className="ui-frame" {...main}>
      {search}
      <Band className="ui-notice-line">{hosts ? null : notice}<div className="ui-heading-slot" ref={setHeading} /></Band>
      {children}
    </main>
  </Slots.Provider>;
}

/** The list's heading ("Best matches … 14 results"), drawn in the frame's line,
 * where a notice can stand in for it without moving a row. */
export function FrameHeading({ children }: { children: ReactNode }) {
  const slots = useContext(Slots);
  return slots?.heading ? createPortal(children, slots.heading) : null;
}

/** Where an empty state has room for the frame's notices, above its own words.
 * While one is drawn, the heading line holds none. */
export function FrameNotices() {
  const slots = useContext(Slots);
  const host = slots?.host;
  useLayoutEffect(() => {
    if (!host) return;
    host(true);
    return () => host(false);
  }, [host]);
  return slots ? <Band className="ui-notice-host">{slots.notices}</Band> : null;
}

/** A notice: its words in one line, then its actions. Words cut to fit unfold
 * whole, over the list, while pointed at or reached with Tab. */
export function LineNotice({ role = "status", children, actions, className }: {
  role?: "status" | "alert"; children: ReactNode; actions?: ReactNode; className?: string;
}) {
  const words = typeof children === "string" ? children : undefined;
  return <div className={className ? `ui-line-notice ${className}` : "ui-line-notice"} data-role={role}>
    <p className="ui-line-words" role={role} dir="auto" title={words} tabIndex={0}>{children}</p>
    {actions}
  </div>;
}
