import type { HTMLAttributes, ReactNode } from "react";
import { Band } from "./Band";
import "./Frame.css";

/** [C24] The picker window's frame: fixed slots in a fixed order, the search
 * band, one reserved notice line, the content (with the Inspector beside it),
 * the shelf and the footer (ui/Frame.css). Each slot keeps its place whatever
 * fills it, so feedback goes into the notice line or the footer's notice slot,
 * never into a row of its own that moves the list. */
export function Frame({ search, notice, children, ...main }: {
  search: ReactNode;
  /** The notice line's notices, most important first. */
  notice: ReactNode;
  children: ReactNode;
} & HTMLAttributes<HTMLElement>) {
  return <main className="ui-frame" {...main}>
    {search}
    <Band className="ui-notice-line">{notice}</Band>
    {children}
  </main>;
}

/** A notice for the notice line: its words in one line, the whole sentence kept
 * for the pointer's tooltip and for screen readers, then its actions. */
export function LineNotice({ role = "status", children, actions, className }: {
  role?: "status" | "alert"; children: ReactNode; actions?: ReactNode; className?: string;
}) {
  const words = typeof children === "string" ? children : undefined;
  return <div className={className ? `ui-line-notice ${className}` : "ui-line-notice"} data-role={role}>
    <p className="ui-line-words" role={role} dir="auto" title={words}>{children}</p>
    {actions}
  </div>;
}
