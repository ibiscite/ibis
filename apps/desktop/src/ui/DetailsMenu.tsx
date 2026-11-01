import { useMemo, useRef, type ReactNode, type RefObject } from "react";
import { Dialog, Heading, Popover } from "react-aria-components";
import { flushSync } from "react-dom";
import { composing } from "../composition";
import { useScrollEdges } from "./ScrollArea";
import { useMenuBoundary, useMenuPlacement } from "./Menu";
import "./Menu.css";
import "./DetailsMenu.css";

/** Editable details use the shared menu surface, anchored to their cause.
 * The form retains dialog semantics for its field and radio group; React Aria
 * owns placement and viewport bounds. Tab leaves the menu, as in every Ibis menu. */
export function DetailsMenu({ id, anchor, returnFocusRef, title, heading = title, children, onClose, onRemove }: {
  id?: string; anchor: HTMLElement; returnFocusRef?: RefObject<HTMLElement | null>;
  title: string; heading?: string; children: ReactNode; onClose: () => void;
  onRemove?: () => void;
}) {
  const trigger = useMemo(() => ({ current: anchor }), [anchor]);
  const placement = useMenuPlacement(true, trigger, "top end");
  const frame = useRef<HTMLDivElement>(null);
  const returnTo = useRef(document.activeElement instanceof HTMLElement && document.activeElement !== document.body ? document.activeElement : null);
  const scroll = useRef<HTMLDivElement>(null);
  useScrollEdges(scroll);
  const close = (returnToSegment = true) => {
    const previous = document.activeElement;
    const inside = frame.current?.contains(previous);
    // Release the menu before returning focus. An anchor may have folded or
    // disappeared; a stale focus return must not take somebody else's focus.
    flushSync(onClose);
    if (!inside || document.activeElement !== document.body && document.activeElement !== previous) return;
    const original = returnToSegment ? returnTo.current : null;
    const target = original?.isConnected && !original.closest("[hidden]") ? original
      : returnToSegment && anchor.isConnected && !anchor.closest("[hidden]") ? anchor.matches("button, input, [tabindex]") ? anchor : anchor.querySelector<HTMLElement>("button, input, [tabindex]")
      : returnFocusRef?.current;
    (target ?? returnFocusRef?.current)?.focus({ preventScroll: true });
  };
  const tab = (backward: boolean) => {
    const origin = returnTo.current?.isConnected && !returnTo.current.closest("[hidden]") ? returnTo.current
      : anchor.matches("button, input, [tabindex]") ? anchor : anchor.querySelector<HTMLElement>("button, input, [tabindex]");
    const controls = [...(anchor.closest("main") ?? document.body).querySelectorAll<HTMLElement>("button, input, select, textarea, a[href], [tabindex]")]
      .filter((node) => node.tabIndex >= 0 && !node.closest("[hidden], [inert], .details-menu") && !node.matches(":disabled") && getComputedStyle(node).display !== "none" && getComputedStyle(node).visibility !== "hidden");
    const index = origin ? controls.indexOf(origin) : -1;
    const target = index >= 0 ? controls[(index + (backward ? -1 : 1) + controls.length) % controls.length] : returnFocusRef?.current;
    close(false);
    target?.focus({ preventScroll: true });
  };
  // Short windows permit a menu to use the whole viewport, over the search
  // band. If its cause leaves view or even one control cannot fit, dismiss it
  // with edits kept and Search focused, rather than detaching or clipping it.
  const boundary = useMenuBoundary(true, undefined, trigger, { overlayRef: frame, scrollRef: scroll, onAnchorUnavailable: () => close(false) });
  return <Popover ref={frame} className="menu-popover details-menu ui-overlay" trigger="DetailsMenu" triggerRef={trigger} isOpen
    placement={placement} offset={6} containerPadding={8} scrollRef={scroll} shouldSkipAnimation {...boundary}
    style={{ zIndex: "var(--z-overlay)" }} onOpenChange={(open) => { if (!open) close(); }}>
      <div className="details-menu-events" onKeyDownCapture={(event) => {
        if (composing(event)) { event.stopPropagation(); return; }
        if (event.key === "Tab") { event.preventDefault(); event.stopPropagation(); tab(event.shiftKey); }
      }} onKeyDown={(event) => {
        event.stopPropagation();
        if (composing(event)) return;
        if (event.key === "Escape" && !composing(event)) { event.preventDefault(); close(); }
        const target = event.target as HTMLElement;
        const inField = target.matches("input:not([type=radio]), textarea") || target.isContentEditable;
        if (onRemove && !inField && (event.key === "Backspace" || event.key === "Delete")) { event.preventDefault(); close(false); onRemove(); }
        // Radio groups retain their native arrow selection. Elsewhere arrows
        // reach the next field/action without making Tab a focus trap.
        if ((event.key === "ArrowDown" || event.key === "ArrowUp" || !inField && (event.key === "Home" || event.key === "End")) && !target.closest('[role="radiogroup"]')) {
          const controls = [...(frame.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), [tabindex="0"]') ?? [])].filter((node) => node.tabIndex >= 0);
          const index = controls.indexOf(target);
          if (index >= 0 && controls.length > 1) {
            event.preventDefault();
            const next = event.key === "Home" ? 0 : event.key === "End" ? controls.length - 1 : (index + (event.key === "ArrowDown" ? 1 : controls.length - 1)) % controls.length;
            controls[next]?.focus();
          }
        }
      }}>
      <Dialog id={id} className="details-menu-body" aria-label={title}>
        <header className="details-menu-heading"><Heading slot="title">{heading}</Heading></header>
        <div ref={scroll} className="details-menu-scroll scroll-area">{children}</div>
      </Dialog>
      </div>
  </Popover>;
}
