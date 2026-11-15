import type { HTMLAttributes, ReactNode } from "react";
import { StatusMark, type StatusKind } from "./StatusMark";
import "./Status.css";

/** A live region: words a screen reader speaks when they change. Each is spoken
 * whole (atomic), so a changing count or preview is heard as one message, never
 * as fragments. `role` says how urgently: "status" waits its turn, "alert"
 * interrupts, "region" is a named area that also speaks its updates, and null
 * draws the words without speaking them (a notice the person already heard).
 * `muted` keeps the role but stops speaking, for words that tick (an age).
 * Keep a Status mounted while its words change: a region that appears with its
 * words already in it is not always spoken. */
export function Status({ as: Tag = "p", label, role = "status", muted = false, tone, className, children, ...props }: {
  as?: "p" | "span" | "div" | "section";
  /** The region's spoken name, when its words alone do not say what they are. */
  label?: string;
  role?: "status" | "alert" | "region" | null;
  muted?: boolean;
  /** A redundant symbol carries the tone; the words keep their readable ink. */
  tone?: StatusKind;
  children?: ReactNode;
} & Omit<HTMLAttributes<HTMLElement>, "role" | "aria-label" | "aria-live" | "aria-atomic">) {
  return <Tag {...props} className={[tone && "ui-status", className].filter(Boolean).join(" ") || undefined} role={role ?? undefined} aria-label={label}
    aria-live={muted || role === null ? "off" : role === "alert" ? "assertive" : "polite"} aria-atomic="true">
    {tone ? <><StatusMark kind={tone} /><span>{children}</span></> : children}
  </Tag>;
}
