import { Children, createContext, useContext, useState, type HTMLAttributes, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Band } from "./Band";
import { usePresence, usePresenceSlot } from "./presence";
import "./Frame.css";
import { Status } from "./Status";

type FrameSlot = "heading" | "notice" | "host" | "inspector" | "reading";
type FrameSlots = {
  heading: HTMLElement | null; drawer: HTMLElement | null; citation: HTMLElement | null;
  setCitation: (node: HTMLElement | null) => void; register: (slot: FrameSlot, on: boolean) => void;
};
const Slots = createContext<FrameSlots | null>(null);

export function useFrameSlot(slot: FrameSlot, present = true) {
  usePresenceSlot(useContext(Slots)?.register, slot, present);
}

/** [C24] The picker window's frame: fixed slots in a fixed order, the search
 * band, the list's heading line, the content (with the Inspector beside it), the
 * drawer and the delivery bar (ui/Frame.css). The drawer opens up from the bar,
 * in the frame's flow, so details never cover a row: the list gives it room. The Frame owns every notice and shows the
 * first one where there is already room, so nothing moves and nothing is
 * reserved empty: over the list's heading line, which it replaces until the
 * notice passes, or, with no list, in the room above an empty state
 * (`FrameNotices`). The notices are drawn once, in the line, which moves to that
 * room, so a notice moving between the two keeps its state (an Undo keeps its
 * time). Callers never place a notice themselves. */
export function Frame({ search, notice, bar, children, ...main }: {
  search: ReactNode;
  /** The delivery bar, drawn last, after the drawer: what is read and tabbed to
   * follows what is seen, top to bottom. */
  bar?: ReactNode;
  /** The notices, most important first. */
  notice: ReactNode;
  children: ReactNode;
} & HTMLAttributes<HTMLElement>) {
  const [heading, setHeading] = useState<HTMLElement | null>(null);
  const [drawer, setDrawer] = useState<HTMLElement | null>(null);
  const [citation, setCitation] = useState<HTMLElement | null>(null);
  const { counts, register } = usePresence({ heading: 0, notice: 0, host: 0, inspector: 0, reading: 0 });
  return <Slots.Provider value={{ heading, drawer, citation, setCitation, register }}>
    <main className="ui-frame" {...main} data-heading={counts.heading ? "" : undefined} data-notice={counts.notice ? "" : undefined}
      data-notice-room={counts.host ? "" : undefined} data-inspector={counts.inspector ? "" : undefined} data-reading={counts.reading ? "" : undefined}>
      {search}
      <Band className="ui-notice-line">{notice}<div className="ui-heading-slot" ref={setHeading} /></Band>
      {children}
      <div className="ui-drawer-slot" ref={setDrawer} />
      {bar}
    </main>
  </Slots.Provider>;
}

/** The list's heading ("Best matches … 14 results"), drawn in the frame's line,
 * where a notice can stand in for it without moving a row. */
export function FrameHeading({ children }: { children: ReactNode }) {
  const slots = useContext(Slots);
  useFrameSlot("heading", Children.toArray(children).length > 0);
  return slots?.heading ? createPortal(children, slots.heading) : null;
}

/** What the drawer shows (ui/Drawer), drawn in the frame's drawer slot above
 * the bar; a part shown without a frame (a test, a board) draws it in place. */
export function FrameDrawer({ children }: { children: ReactNode }) {
  const slots = useContext(Slots);
  if (!slots) return children;
  return slots.drawer ? createPortal(children, slots.drawer) : null;
}

/** The delivery bar's room for the citation, which the citation line fills. */
export function FrameCitationSlot() {
  return <div className="bar-citation" ref={useContext(Slots)?.setCitation} />;
}

/** The citation line, drawn in the delivery bar's room for it, wherever its
 * owner renders; without a frame it is drawn in place. */
export function FrameCitation({ children }: { children: ReactNode }) {
  const slots = useContext(Slots);
  if (!slots) return children;
  return slots.citation ? createPortal(children, slots.citation) : null;
}

/** An empty state has room for the frame's notices above its own words: while
 * this is drawn, the notice line sits there, over the content's top. */
export function FrameNotices() {
  useFrameSlot("host");
  return null;
}

/** The line's notice container, including notices with richer words or lists. */
export function FrameNotice({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  useFrameSlot("notice");
  return <div {...props} className={className ? `ui-line-notice ${className}` : "ui-line-notice"} />;
}

/** A notice: its words in one line, then its actions. Words cut to fit unfold
 * whole, over the list, while pointed at or reached with Tab. */
export function LineNotice({ role = "status", children, actions, className }: {
  role?: "status" | "alert"; children: ReactNode; actions?: ReactNode; className?: string;
}) {
  const words = typeof children === "string" ? children : undefined;
  return <FrameNotice className={className} data-role={role}>
    <Status className="ui-line-words" role={role} dir="auto" title={words} tabIndex={0}>{children}</Status>
    {actions}
  </FrameNotice>;
}
