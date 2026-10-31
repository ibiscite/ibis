import { useEffect, useId, useLayoutEffect, useRef, useState, type RefObject } from "react";
import { pickerText } from "../messages";
import { Button } from "./Button";
import "./UndoNotice.css";

/** Eight visible seconds, paused on hover, focus and while Undo is in flight.
 * One timeout owns expiry; no polling or layout changes. */
export function UndoNotice({ message, onUndo, onExpire, paused = false, fallbackFocusRef }: {
  message: string; onUndo: () => Promise<void>; onExpire: () => void; paused?: boolean; fallbackFocusRef?: RefObject<HTMLElement | null>;
}) {
  const remaining = useRef(8000);
  const description = useId();
  const notice = useRef<HTMLDivElement>(null);
  const mounted = useRef(true);
  const [hovered, setHovered] = useState(false), [focused, setFocused] = useState(false), [pending, setPending] = useState(false);
  const [error, setError] = useState(false);
  useEffect(() => {
    if (paused || hovered || focused || pending) return;
    const start = performance.now();
    const timer = window.setTimeout(onExpire, remaining.current);
    return () => { window.clearTimeout(timer); remaining.current = Math.max(0, remaining.current - (performance.now() - start)); };
  }, [paused, hovered, focused, pending, onExpire]);
  useLayoutEffect(() => {
    mounted.current = true;
    const node = notice.current;
    return () => {
      mounted.current = false;
      if (node?.contains(document.activeElement)) fallbackFocusRef?.current?.focus({ preventScroll: true });
    };
  }, [fallbackFocusRef]);
  return <div ref={notice} className="undo-notice" onPointerEnter={() => setHovered(true)} onPointerLeave={() => setHovered(false)}
    onFocusCapture={() => setFocused(true)} onBlurCapture={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocused(false); }}>
    <span id={description} role={error ? "alert" : "status"}>{error ? pickerText.undoFailed : message}</span>
    {/* Retain native focus while waiting, so dismissal can return it to Search. */}
    <Button variant="quiet" size="small" aria-describedby={description} isPending={pending} onPress={() => {
      setPending(true); setError(false);
      void onUndo().catch(() => { if (mounted.current) setError(true); }).finally(() => { if (mounted.current) setPending(false); });
    }}>{pickerText.undo}</Button>
  </div>;
}
