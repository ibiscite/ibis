import { useCallback, useContext, useLayoutEffect, useRef, useState, type ComponentProps, type ReactNode, type Ref, type RefObject } from "react";
import { flushSync } from "react-dom";
import { Popover } from "react-aria-components";
import { LayerParent, useLayer, type LayerClose, type LayerHandle } from "./layers";
import { anchorIn, fitSide, roomBeside, useRegion, type Side } from "./region";
import "./Overlay.css";

/** What renders inside a layer belongs to it: a menu opened in a popover or a
 * sheet leaves that one open (ui/layers). */
export function LayerScope({ layer, children }: { layer: LayerHandle; children: ReactNode }) {
  return <LayerParent value={layer.scope}>{children}</LayerParent>;
}

export type FloatingPlacement = "top left" | "top right" | "bottom left" | "bottom right";

/** The one way to float something over the window: a React Aria popover whose
 * being open is a layer on the stack, so opening it closes any other menu or
 * popover, and Esc and Back reach it first. `open` is what is shown; React Aria's
 * own trigger may still hold the state, as a menu's does. Without a kind it is
 * part of the layer it opens in, as a submenu is of its menu.
 *
 * Anchored to `anchor`, it fits its region (ui/region, decision 0062): it opens
 * on the side of the anchor with room for `least` (what it must show, measured
 * on the drawn surface), its height is what that side allows, and where focus
 * returns plays no part. With room on neither side it calls `onCramped`, whose
 * owner opens it as a view of the window, or, without one, takes the larger side. */
export function Floating({ kind, open = false, onClose = () => false, anchor, placement = "bottom right", least, onCramped, children, ref, offset = 0, containerPadding = 12, ...popover }: Omit<ComponentProps<typeof Popover>, "children" | "placement" | "ref"> & {
  kind?: "menu" | "popover"; open?: boolean; onClose?: LayerClose; children: ReactNode;
  anchor?: RefObject<HTMLElement | null>; placement?: FloatingPlacement; least?: (surface: HTMLElement) => number; onCramped?: () => void;
  ref?: Ref<HTMLElement>;
}) {
  const layer = useLayer(kind ?? "menu", open && kind !== undefined, onClose);
  const surface = useRef<HTMLElement | null>(null);
  const setRef = useCallback((node: HTMLElement | null) => {
    surface.current = node;
    if (typeof ref === "function") return ref(node);
    if (ref) ref.current = node;
  }, [ref]);
  const fit = useFit({ open: open && anchor !== undefined, anchor, surface, placement, least, gap: offset, padding: containerPadding, onCramped });
  // Marked when it opened inside another layer, which stays open under it.
  return <Popover {...popover} ref={setRef} offset={offset} containerPadding={containerPadding} {...anchor ? fit : { placement }}
    data-layer-surface data-layer={kind} data-layer-within={useContext(LayerParent) !== null || undefined}>
    {kind ? <LayerScope layer={layer}>{children}</LayerScope> : children}
  </Popover>;
}

/** The one fallback of decision 0062: a menu with room for its least on
 * neither side opens as a view of the window, the way the palettes do, until it
 * closes. Pass `cramped` as a `Floating`'s `onCramped` and draw the view while
 * `asView`. The popover leaving takes the keyboard from inside it, which React
 * Aria's trigger reports as a dismissal: `heeds(next)` says whether a change
 * its trigger reports is the person's (a close never is while the view stands in). */
export function useViewFallback(open: boolean) {
  const [asView, setAsView] = useState(false);
  const standing = useRef(false);
  useLayoutEffect(() => { if (!open) { standing.current = false; setAsView(false); } }, [open]);
  const cramped = useCallback(() => { standing.current = true; setAsView(true); }, []);
  const heeds = useCallback((next: boolean) => next || !standing.current, []);
  return { asView: open && asView, cramped, heeds };
}

const windowBox = () => new DOMRect(0, 0, document.documentElement.clientWidth || innerWidth, document.documentElement.clientHeight || innerHeight);
const boxOf = (rect: DOMRect) => [rect.left, rect.top, rect.width, rect.height].map(Math.round);

/** React Aria's boundary (the region's, or the window's when only it has room),
 * the side, and the anchor as the region sees it. */
function useFit({ open, anchor, surface, placement, least, gap, padding, onCramped }: {
  open: boolean; anchor?: RefObject<HTMLElement | null>; surface: RefObject<HTMLElement | null>; placement: FloatingPlacement;
  least?: (surface: HTMLElement) => number; gap: number; padding: number; onCramped?: () => void;
}) {
  const region = useRegion();
  const [fit, setFit] = useState<{ boundary: HTMLElement; side: Side; visible: DOMRect }>();
  const latest = useRef({ least, onCramped });
  useLayoutEffect(() => { latest.current = { least, onCramped }; });
  const [preferred, horizontal] = placement.split(" ") as [Side, "left" | "right"];
  useLayoutEffect(() => {
    if (!open) { setFit(undefined); return; }
    let current = true, frame = 0, signature = "", side: Side | undefined, placedIn: DOMRect | undefined, node: HTMLElement | null = null;
    const update = () => {
      const trigger = anchor?.current;
      if (!current || !trigger) return;
      const visible = region.visible(), bounds = region.bounds(), whole = windowBox();
      const rect = anchorIn(trigger.getBoundingClientRect(), visible);
      const need = surface.current && latest.current.least ? latest.current.least(surface.current) : 0;
      const next = [...boxOf(whole), ...boxOf(bounds), ...boxOf(visible), ...boxOf(rect)].join(",");
      // Filtering its rows never flips it: with the same region and anchor, the
      // side it opened on stays while it has room for what it now must show.
      if (next === signature && (!side || roomBeside(rect, placedIn ?? whole, gap, padding)[side] >= need)) return;
      signature = next;
      const found = fitSide({ anchor: rect, boundaries: [bounds, whole], least: need, preferred: side ?? preferred, gap, padding });
      if (!found && latest.current.onCramped && region.views) { latest.current.onCramped(); return; }
      const room = roomBeside(rect, whole, gap, padding);
      const chosen = found ?? { side: room.top > room.bottom ? "top" as const : "bottom" as const, boundary: whole };
      side = chosen.side;
      placedIn = chosen.boundary;
      const previous = node;
      node = document.createElement("div");
      node.dataset.floatingBoundary = "";
      node.setAttribute("aria-hidden", "true");
      const { left, top, width, height } = chosen.boundary;
      Object.assign(node.style, { position: "fixed", left: `${left}px`, top: `${top}px`, width: `${width}px`, height: `${height}px`, pointerEvents: "none", visibility: "hidden" });
      document.body.append(node);
      // React Aria observes the surface's size, not position-only layout
      // changes: a fresh boundary asks it to place and cap the surface again,
      // without remounting what is focused inside.
      setFit({ boundary: node, side: chosen.side, visible });
      previous?.remove();
    };
    const schedule = () => {
      if (!current || frame) return;
      frame = requestAnimationFrame(() => { frame = 0; update(); });
    };
    update();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(schedule);
    const owner = region.element() ?? anchor?.current?.closest("main") ?? anchor?.current?.parentElement;
    if (anchor?.current) observer?.observe(anchor.current);
    // A neighbour can move a fixed-size anchor: a header wrapping, a shelf
    // folding, a footer growing all belong to the region that holds it.
    if (owner) {
      observer?.observe(owner);
      for (const child of owner.children) observer?.observe(child);
    }
    const changed = new MutationObserver(schedule);
    // Its rows arrive after it opens (a collection renders twice), and change as
    // it filters: it fits again at once, before the frame is drawn, so it never
    // shows on one side and then moves to the other.
    const rows = new MutationObserver(() => { if (current) flushSync(update); });
    if (surface.current) rows.observe(surface.current, { childList: true, subtree: true });
    if (owner) changed.observe(owner, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ["class", "style", "hidden", "data-folded"] });
    window.addEventListener("resize", schedule);
    window.visualViewport?.addEventListener("resize", schedule);
    document.addEventListener("scroll", schedule, true);
    return () => {
      current = false;
      cancelAnimationFrame(frame);
      observer?.disconnect();
      changed.disconnect();
      rows.disconnect();
      window.removeEventListener("resize", schedule);
      window.visualViewport?.removeEventListener("resize", schedule);
      document.removeEventListener("scroll", schedule, true);
      node?.remove();
    };
  }, [open, anchor, surface, region, preferred, gap, padding]);
  const getTargetRect = useCallback((target: Element) => anchorIn(target.getBoundingClientRect(), fit?.visible ?? windowBox()), [fit]);
  return {
    placement: `${fit?.side ?? preferred} ${horizontal}` as FloatingPlacement, shouldFlip: false,
    boundaryElement: fit?.boundary.isConnected ? fit.boundary : undefined, getTargetRect,
  };
}
