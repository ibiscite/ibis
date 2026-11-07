import { Children, useEffect, useRef, type ReactNode } from "react";
import { say } from "../i18n";
import "./NoticeSlot.css";

/** A window's one place for what passes; nothing is drawn while it is empty. */
export function NoticeSlot({ children }: { children: ReactNode }) {
  return Children.toArray(children).length ? <div className="notice-slot" role="region" aria-label={say("picker-notices")}>{children}</div> : null;
}

/** A remark that needs nothing from the person. Refusals and sizes are announced; a
 * passive one is not. Its 8 s run only while it is shown, so none passes unseen. */
export function QuietNotice({ children, passive = false, role = "status", onExpire, paused = false }: {
  children: ReactNode; passive?: boolean; role?: "status" | "alert"; onExpire?: () => void; paused?: boolean;
}) {
  const expire = useRef(onExpire);
  expire.current = onExpire;
  const expires = Boolean(onExpire);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new remark starts its own time.
  useEffect(() => {
    if (!expires || paused) return;
    const timer = window.setTimeout(() => expire.current?.(), 8000);
    return () => window.clearTimeout(timer);
  }, [children, expires, paused]);
  return <div className="notice-band"><span role={passive ? undefined : role}>{children}</span></div>;
}
