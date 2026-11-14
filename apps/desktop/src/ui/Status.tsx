import type { HTMLAttributes, ReactNode } from "react";
import { useInterfaceMessages } from "../i18n";

/** The only live region. Keep its owner mounted while its words change;
 * atomic updates speak one complete message, never the changing fragments. */
export function Status({ as: Tag = "p", label, role = "status", muted = false, children, ...props }: {
  as?: "p" | "span" | "div" | "section" | "bdi";
  label?: string; role?: "status" | "alert" | "region" | null; muted?: boolean; children?: ReactNode;
} & Omit<HTMLAttributes<HTMLElement>, "role" | "aria-label" | "aria-live" | "aria-atomic">) {
  const { text } = useInterfaceMessages();
  return <Tag {...props} role={role ?? undefined} aria-label={role ? label ?? text("picker-notices") : undefined}
    aria-live={muted || role === null ? "off" : role === "alert" ? "assertive" : "polite"} aria-atomic="true">{children}</Tag>;
}
