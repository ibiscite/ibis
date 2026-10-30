import { useMemo, useRef, type ReactNode, type RefObject } from "react";
import { Dialog, Heading, Popover } from "react-aria-components";
import { flushSync } from "react-dom";
import { composing } from "../composition";
import { keyText } from "../messages";
import { Button } from "./Button";
import { useScrollEdges } from "./ScrollArea";
import { useMenuBoundary, useMenuPlacement } from "./Menu";
import "./Menu.css";
import "./DetailsMenu.css";

/** Editable details use the shared menu surface, anchored to their cause.
 * The form retains dialog semantics for its field and radio group; React Aria
 * owns placement, viewport bounds, Tab containment and return focus. */
export function DetailsMenu({ id, anchor, returnFocusRef, title, heading = title, children, onClose }: {
  id?: string; anchor: HTMLElement; returnFocusRef?: RefObject<HTMLElement | null>;
  title: string; heading?: string; children: ReactNode; onClose: () => void;
}) {
  const trigger = useMemo(() => ({ current: anchor }), [anchor]);
  const placement = useMenuPlacement(true, trigger, "top end");
  const frame = useRef<HTMLDivElement>(null);
  const returnTo = useRef(document.activeElement instanceof HTMLElement ? document.activeElement : null);
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
  // Short windows permit a menu to use the whole viewport, over the search
  // band. If its cause leaves view or even one control cannot fit, dismiss it
  // with edits kept and Search focused, rather than detaching or clipping it.
  const boundary = useMenuBoundary(true, undefined, trigger, { overlayRef: frame, scrollRef: scroll, onAnchorUnavailable: () => close(false) });
  return <Popover ref={frame} className="menu-popover details-menu" trigger="DetailsMenu" triggerRef={trigger} isOpen
    placement={placement} offset={6} containerPadding={8} scrollRef={scroll} shouldSkipAnimation {...boundary}
    style={{ zIndex: "var(--z-overlay)" }} onOpenChange={(open) => { if (!open) close(); }}>
      <div className="details-menu-events" onKeyDown={(event) => {
        // React Aria contains Tab at document level. Other keys stay inside
        // this overlay instead of reaching the picker's shortcut handler.
        if (event.key !== "Tab" || composing(event)) event.stopPropagation();
        if (event.key === "Escape" && !composing(event)) { event.preventDefault(); close(); }
      }}>
      <Dialog id={id} className="details-menu-body" aria-label={title}>
        <header className="details-menu-heading"><Heading slot="title" title={heading}>{heading}</Heading>
          <Button variant="icon" size="small" glyph="close" aria-label={keyText.close} onPress={() => close()} />
        </header>
        <div ref={scroll} className="details-menu-scroll scroll-area">{children}</div>
      </Dialog>
      </div>
  </Popover>;
}
