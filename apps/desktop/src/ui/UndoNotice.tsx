import { useEffect, useId, useLayoutEffect, useRef, useState, type RefObject } from "react";
import { pickerText } from "../messages";
import { Button } from "./Button";
import { useInterfaceMessages } from "../i18n";
import { currentPlatform } from "../platform";
import { registerUndo } from "./editUndo";
import "./UndoNotice.css";

function noticeText(value: string) {
  const letters = [...value.replace(/\s+/gu, " ")].filter((letter) => {
    const code = letter.codePointAt(0)!;
    return code >= 32 && !(code >= 127 && code <= 159) && !(code >= 0x202a && code <= 0x202e) && !(code >= 0x2066 && code <= 0x2069);
  });
  return letters.length <= 160 ? letters.join("") : `${letters.slice(0, 120).join("")}…${letters.slice(-39).join("")}`;
}

/** Eight visible seconds, paused on hover, focus and while Undo is in flight.
 * One timeout owns expiry; no polling or layout changes. */
export function UndoNotice({ message, action, onUndo, onExpire, paused = false, fallbackFocusRef }: {
  message: string; action: string; onUndo: () => Promise<void>; onExpire: () => void; paused?: boolean; fallbackFocusRef?: RefObject<HTMLElement | null>;
}) {
  const { text } = useInterfaceMessages();
  const label = text("undo-action", { action: noticeText(action) });
  const remaining = useRef(8000);
  const description = useId();
  const notice = useRef<HTMLDivElement>(null);
  const mounted = useRef(true);
  const [hovered, setHovered] = useState(false), [focused, setFocused] = useState(false), [pending, setPending] = useState(false);
  const [error, setError] = useState(false);
  const busy = useRef(false);
  const invoke = useRef(() => {});
  invoke.current = () => {
    if (busy.current || paused) return;
    busy.current = true; setPending(true); setError(false);
    void onUndo().catch(() => { if (mounted.current) setError(true); }).finally(() => {
      busy.current = false; if (mounted.current) setPending(false);
    });
  };
  const lease = useRef<ReturnType<typeof registerUndo> | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: pausing retains this action’s place in undo order.
  useEffect(() => {
    const current = registerUndo(label, () => invoke.current());
    current.pause(paused); lease.current = current;
    return () => { current(); if (lease.current === current) lease.current = null; };
  }, [label]);
  useEffect(() => lease.current?.pause(paused), [paused]);
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
    <span id={description} role={error ? "alert" : "status"}>{error ? pickerText.undoFailed : noticeText(message)}</span>
    {/* Retain native focus while waiting, so dismissal can return it to Search. */}
    {/* The message names the action; the short word keeps two notices on a small window. */}
    <Button variant="quiet" size="small" aria-label={label} aria-describedby={description} aria-keyshortcuts={currentPlatform() === "macos" ? "Meta+Z" : "Control+Z"} isPending={pending} onPress={() => invoke.current()}>{pickerText.undo}</Button>
  </div>;
}
