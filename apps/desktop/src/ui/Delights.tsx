import { type ReactNode, useEffect, useRef, useState } from "react";
import { composing } from "../composition";
import { useInterfaceMessages } from "../i18n";
import { matchesAccelerator } from "../keys";
import { currentPlatform } from "../platform";
import { Button } from "./Button";
import { IbisMark } from "./IbisMark";
import "./Delights.css";

function matchesEcho(event: KeyboardEvent, accelerator: string) {
  const parts = accelerator === "+" ? ["+"] : accelerator.split(/\+(?!$)/);
  const key = parts.pop() ?? "";
  const modifiers = new Set(parts.map((part) => part.toLowerCase()));
  const mac = currentPlatform() === "macos";
  const command = ["cmdorctrl", "commandorcontrol", "commandorctrl", "cmdorcontrol"].some((name) => modifiers.has(name));
  const ctrl = command ? !mac : modifiers.has("ctrl") || modifiers.has("control");
  const meta = command ? mac : ["cmd", "command", "meta", "super"].some((name) => modifiers.has(name));
  if (event.ctrlKey !== ctrl || event.metaKey !== meta) return false;
  const help = event.key === "?" && (key === "?" || key === "/");
  // Retain the shared layout-aware key matcher; only explicit modifier aliases differ.
  return matchesAccelerator({ key: help ? key : event.key === " " ? "Space" : event.key, code: event.code, altKey: event.altKey, shiftKey: help ? modifiers.has("shift") : event.shiftKey, ctrlKey: false, metaKey: false },
    [...(modifiers.has("alt") || modifiers.has("option") ? ["Alt"] : []), ...(modifiers.has("shift") ? ["Shift"] : []), key].join("+"), mac);
}

/** Visual echoes never handle a command or move focus. */
export function useKeyPulse(accelerator: string | readonly string[], enabled = true) {
  const [pressed, setPressed] = useState(false);
  const keys = typeof accelerator === "string" ? accelerator : accelerator.join("|");
  useEffect(() => {
    let timer: number | undefined;
    setPressed(false);
    if (!enabled) return;
    const down = (event: KeyboardEvent) => {
      if (!enabled || event.repeat || composing(event) || document.querySelector('[data-recording="true"]')) return;
      if (event.target instanceof Element && event.target.closest('[role="menu"], [role="dialog"], .toolbar-menu-popover')) return;
      if (!keys.split("|").some((key) => matchesEcho(event, key))) return;
      setPressed(true);
      window.clearTimeout(timer);
      timer = window.setTimeout(() => setPressed(false), 100);
    };
    const clear = () => { window.clearTimeout(timer); setPressed(false); };
    window.addEventListener("keydown", down);
    window.addEventListener("blur", clear);
    return () => { window.clearTimeout(timer); window.removeEventListener("keydown", down); window.removeEventListener("blur", clear); };
  }, [keys, enabled]);
  return pressed;
}

export function ShortcutEcho({ keys, children }: { keys: readonly string[]; children: ReactNode }) {
  const pressed = useKeyPulse(keys, keys.length > 0);
  return <div className="ui-shortcut-echo" data-echo={pressed || undefined}>{children}</div>;
}

export function AboutDelight({ blink, enabled, onClick }: { blink: number; enabled: boolean; onClick: () => void }) {
  const { text } = useInterfaceMessages();
  return <div className="ui-about-delight">
    <Button variant="quiet" aria-label={text("delight-ibis")} preventFocusOnPress onPress={onClick}>
      <IbisMark key={blink} size="welcome" motion={enabled && blink > 0 ? "blink" : undefined} />
    </Button>
  </div>;
}

export function QuietStatus({ children, passive = false }: { children: ReactNode; passive?: boolean }) {
  return children ? <p className="ui-quiet-status" role={passive ? undefined : "status"}>{children}</p> : null;
}

/** Browser zoom stays local to this webview; the receipt slot reports its result. */
export function useZoomFeedback(enabled: boolean) {
  const { text } = useInterfaceMessages();
  const [percent, setPercent] = useState<number | null>(null);
  const zoom = useRef(Number(document.documentElement.style.zoom) || 1);
  useEffect(() => {
    let timer: number | undefined;
    const down = (event: KeyboardEvent) => {
      if (!enabled || composing(event) || event.altKey || event.ctrlKey || !event.metaKey || !["+", "=", "-", "−"].includes(event.key)) return;
      event.preventDefault();
      if (event.repeat) return;
      zoom.current = Math.max(0.5, Math.min(2, Math.round((zoom.current + (["-", "−"].includes(event.key) ? -0.1 : 0.1)) * 10) / 10));
      document.documentElement.style.zoom = String(zoom.current);
      setPercent(Math.round(zoom.current * 100));
      window.clearTimeout(timer);
      timer = window.setTimeout(() => setPercent(null), 1000);
    };
    setPercent(null);
    window.addEventListener("keydown", down);
    return () => { window.clearTimeout(timer); window.removeEventListener("keydown", down); };
  }, [enabled]);
  return percent === null ? null : text("delight-zoom", { percent });
}

export function useEmptyFeedback(identity: string) {
  const [feedback, setFeedback] = useState<string | null>(null);
  const [shaking, setShaking] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new presentation retires feedback.
  useEffect(() => { setFeedback(null); setShaking(false); return () => window.clearTimeout(timer.current); }, [identity]);
  return { feedback, shaking, refuse: (message: string) => {
    setFeedback(message); setShaking(true); window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setShaking(false), 240);
  } };
}
