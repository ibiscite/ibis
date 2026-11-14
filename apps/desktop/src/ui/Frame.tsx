import { createContext, useCallback, useContext, useLayoutEffect, useState, type HTMLAttributes, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Band } from "./Band";
import { say } from "../i18n";
import "./Frame.css";
import { Status } from "./Status";

type FrameSlots = { heading: HTMLElement | null; host: (on: boolean) => void };
const Slots = createContext<FrameSlots | null>(null);

/** [C24] The picker window's frame: fixed slots in a fixed order, the search
 * band, the list's heading line, the content (with the Inspector beside it), the
 * shelf and the footer (ui/Frame.css). The Frame owns every notice and shows the
 * first one where there is already room, so nothing moves and nothing is
 * reserved empty: over the list's heading line, which it replaces until the
 * notice passes, or, with no list, in the room above an empty state
 * (`FrameNotices`). The notices are drawn once, in the line, which moves to that
 * room, so a notice moving between the two keeps its state (an Undo keeps its
 * time). Callers never place a notice themselves. */
export function Frame({ search, notice, children, ...main }: {
  search: ReactNode;
  /** The notices, most important first. */
  notice: ReactNode;
  children: ReactNode;
} & HTMLAttributes<HTMLElement>) {
  const [heading, setHeading] = useState<HTMLElement | null>(null);
  const [hosts, setHosts] = useState(0);
  const host = useCallback((on: boolean) => setHosts((count) => count + (on ? 1 : -1)), []);
  return <Slots.Provider value={{ heading, host }}>
    <main className="ui-frame" aria-label={say("delight-ibis")} data-notice-room={hosts ? "" : undefined} {...main}>
      {search}
      <Band className="ui-notice-line">{notice}<div className="ui-heading-slot" ref={setHeading} /></Band>
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

/** An empty state has room for the frame's notices above its own words: while
 * this is drawn, the notice line sits there, over the content's top. */
export function FrameNotices() {
  const host = useContext(Slots)?.host;
  useLayoutEffect(() => {
    if (!host) return;
    host(true);
    return () => host(false);
  }, [host]);
  return null;
}

/** A notice: its words in one line, then its actions. Words cut to fit unfold
 * whole, over the list, while pointed at or reached with Tab. */
export function LineNotice({ role = "status", children, actions, className }: {
  role?: "status" | "alert"; children: ReactNode; actions?: ReactNode; className?: string;
}) {
  const words = typeof children === "string" ? children : undefined;
  return <div className={className ? `ui-line-notice ${className}` : "ui-line-notice"} data-role={role}>
    <Status className="ui-line-words" role={role} dir="auto" title={words} tabIndex={0}>{children}</Status>
    {actions}
  </div>;
}
