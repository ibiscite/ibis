import { useEffect, useId, useLayoutEffect, useRef, useState, useSyncExternalStore, type RefObject } from "react";
import { say } from "../i18n";
import { keysFor } from "../keys";
import { Button } from "./Button";
import { useInterfaceMessages } from "../i18n";
import { registerUndo, subscribeUndo } from "./editUndo";
import "./NoticeSlot.css";

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
    const current = registerUndo(() => invoke.current());
    current.pause(paused); lease.current = current;
    return () => { current(); if (lease.current === current) lease.current = null; };
  }, [label]);
  useEffect(() => lease.current?.pause(paused), [paused]);
  // The slot holds one notice: an older one waits, keeping the rest of its time.
  const newest = useSyncExternalStore(subscribeUndo, () => lease.current?.isNewest() ?? true);
  useLayoutEffect(() => {
    if (newest || !notice.current?.contains(document.activeElement)) return;
    setFocused(false);
    fallbackFocusRef?.current?.focus({ preventScroll: true });
  }, [newest, fallbackFocusRef]);
  useEffect(() => {
    if (paused || hovered || focused || pending || !newest) return;
    const start = performance.now();
    const timer = window.setTimeout(onExpire, remaining.current);
    return () => { window.clearTimeout(timer); remaining.current = Math.max(0, remaining.current - (performance.now() - start)); };
  }, [paused, hovered, focused, pending, newest, onExpire]);
  useLayoutEffect(() => {
    mounted.current = true;
    const node = notice.current;
    return () => {
      mounted.current = false;
      if (node?.contains(document.activeElement)) fallbackFocusRef?.current?.focus({ preventScroll: true });
    };
  }, [fallbackFocusRef]);
  return <div ref={notice} className="notice-band" data-offer hidden={!newest} onPointerEnter={() => setHovered(true)} onPointerLeave={() => setHovered(false)}
    onFocusCapture={() => setFocused(true)} onBlurCapture={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocused(false); }}>
    <span id={description} role={error ? "alert" : "status"} title={noticeText(message)}>{error ? say("picker-undo-failed") : noticeText(message)}</span>
    {/* Retain native focus while waiting, so dismissal can return it to Search. */}
    {/* The message names the action; the key cap says ⌘Z does the same. */}
    <Button variant="quiet" size="small" label={say("picker-undo")} keys={keysFor("undo")} aria-label={label} aria-describedby={description} isPending={pending} onPress={() => invoke.current()} />
  </div>;
}
