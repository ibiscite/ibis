import { createContext, useContext, useMemo, type ReactNode } from "react";

/** A region of a window (decision 0062): where what floats from inside it may
 * go, and what of it a person sees. The frame publishes the window's content
 * box, the drawer its visible box and a view or page its scroll box; a floating
 * surface takes its boundary from the nearest one, never from where focus will
 * return, and a reveal keeps its target inside the visible box. */
export type Region = {
  /** Where a surface anchored here may float, in window coordinates. */
  bounds: () => DOMRect;
  /** What of the region a person sees: its box less a sticky title, inside its parent's. */
  visible: () => DOMRect;
  /** The element whose size and content move the region. */
  element: () => HTMLElement | null;
  /** Whether a menu with no room on either side may open as a view of the window. */
  views: boolean;
};

const windowBox = () => new DOMRect(0, 0, document.documentElement.clientWidth || innerWidth, document.documentElement.clientHeight || innerHeight);
const intersect = (a: DOMRect, b: DOMRect) => {
  const left = Math.max(a.left, b.left), top = Math.max(a.top, b.top);
  return new DOMRect(left, top, Math.max(0, Math.min(a.right, b.right) - left), Math.max(0, Math.min(a.bottom, b.bottom) - top));
};

/** The whole window, for a part drawn without a frame (a test, a board). */
const WINDOW: Region = { bounds: windowBox, visible: windowBox, element: () => null, views: false };
const Regions = createContext<Region>(WINDOW);

export const useRegion = () => useContext(Regions);

/** What of a scroll box a person sees: inside its borders and scroll bars, less
 * a sticky title at its top or a sticky bar at its foot, which content scrolls
 * under. The reveal and the canvas's checks read this one definition. */
export function visibleBox(box: Element): DOMRect {
  const rect = box.getBoundingClientRect();
  const left = rect.left + box.clientLeft, right = left + (box.clientWidth || rect.width);
  let top = rect.top + box.clientTop, bottom = top + (box.clientHeight || rect.height);
  for (const child of box.children) {
    if (getComputedStyle(child).position !== "sticky") continue;
    const stuck = child.getBoundingClientRect();
    if (stuck.top <= top + 1 && stuck.bottom > top) top = stuck.bottom;
    else if (stuck.bottom >= bottom - 1 && stuck.top < bottom) bottom = stuck.top;
  }
  return new DOMRect(left, top, Math.max(0, right - left), Math.max(0, bottom - top));
}

/** Publishes `element` as the region for everything inside. `floats` says where
 * what floats from here may go: its own visible box (a page, a view), its
 * parent's (the drawer: a menu from a field in it opens over the list), or a
 * box of its own (the frame's content, which ends above the delivery bar). */
export function Region({ element, floats = "own", views, children }: {
  element: HTMLElement | null;
  floats?: "own" | "parent" | ((element: HTMLElement) => DOMRect);
  views?: boolean;
  children: ReactNode;
}) {
  const parent = useRegion();
  const region = useMemo<Region>(() => {
    const visible = () => element?.isConnected ? intersect(visibleBox(element), parent.visible()) : parent.visible();
    return {
      element: () => element ?? parent.element(),
      visible,
      bounds: () => !element?.isConnected || floats === "parent" ? parent.bounds() : floats === "own" ? visible() : floats(element),
      views: views ?? parent.views,
    };
  }, [element, floats, views, parent]);
  return <Regions.Provider value={region}>{children}</Regions.Provider>;
}

/** The rows a floating list shows at least (decision 0062): four, or all it has. */
export const LEAST_ROWS = 4;

export type Side = "top" | "bottom";

/** Which side of its anchor a surface opens on (decision 0062): the preferred
 * side when its least fits there, else the other; the first boundary with room
 * wins (the region, then the whole window). Null when no side of any has room.
 * `gap` is the offset from the anchor, `padding` the boundary's own. */
export function fitSide({ anchor, boundaries, least, preferred, gap, padding }: {
  anchor: DOMRect; boundaries: readonly DOMRect[]; least: number; preferred: Side; gap: number; padding: number;
}): { side: Side; boundary: DOMRect } | null {
  const other: Side = preferred === "top" ? "bottom" : "top";
  for (const boundary of boundaries) {
    const room = roomBeside(anchor, boundary, gap, padding);
    if (room[preferred] >= least) return { side: preferred, boundary };
    if (room[other] >= least) return { side: other, boundary };
  }
  return null;
}

/** The room above and below an anchor inside a boundary. */
export function roomBeside(anchor: DOMRect, boundary: DOMRect, gap: number, padding: number): Record<Side, number> {
  return { top: anchor.top - gap - boundary.top - padding, bottom: boundary.bottom - padding - anchor.bottom - gap };
}

/** An anchor where a person can see it: one scrolled out of its region's
 * visible box (a field low in the drawer, a footer below a wrapped window)
 * counts as at that box's edge, so placement still has its room to measure. */
export function anchorIn(rect: DOMRect, visible: DOMRect): DOMRect {
  if (rect.top >= visible.top && rect.bottom <= visible.bottom) return rect;
  const top = Math.max(visible.top, Math.min(rect.top, visible.bottom - rect.height));
  return new DOMRect(rect.x, top, rect.width, rect.height);
}
