import { useCallback, useLayoutEffect, useState, type HTMLAttributes, type ReactNode, type Ref } from "react";
import { FrameDrawer } from "./Frame";
import { LayerScope } from "./Layer";
import { useLayer, type LayerClose } from "./layers";
import { Region } from "./region";
import "./Drawer.css";

/** Details that open up from the delivery bar, full width, in the frame's own
 * flow: the list gives them its end, so they never cover a row, and the list's
 * highlighted row stays in view above them. Not modal: everything around them
 * stays live, and the picker's layers own Esc, so the caller closes it. What the
 * drawer is about (`lead`) sits beside its fields when the window has room.
 * Its visible box is a region (ui/region): a menu from a field scrolled out of
 * it is placed from the edge a person sees, and opens over the list. */
export function Drawer({ ref, kind = "popover", label, role = "dialog", className, lead, children, onClose, ...section }: {
  ref?: Ref<HTMLElement>;
  /** A Peek is a menu on the view stack: it never takes focus, and any other surface closes it. */
  kind?: "popover" | "menu"; label: string; role?: "dialog" | "region"; className?: string;
  lead: ReactNode; children?: ReactNode; onClose: LayerClose;
} & Omit<HTMLAttributes<HTMLElement>, "role" | "children">) {
  const layer = useLayer(kind, true, onClose);
  const [box, setBox] = useState<HTMLElement | null>(null);
  // The caller's ref keeps its own cleanup, which React 19 calls on detach.
  const own = useCallback((node: HTMLElement | null) => {
    setBox(node);
    if (typeof ref === "function") return ref(node);
    if (ref) ref.current = node;
  }, [ref]);
  // The list is shorter now: its highlighted row stays where the person can see it.
  useLayoutEffect(() => { document.querySelector('[role="listbox"] [aria-selected="true"]')?.scrollIntoView?.({ block: "nearest" }); }, []);
  return <FrameDrawer><LayerScope layer={layer}><Region element={box} floats="parent">
    <section {...section} ref={own} role={role} aria-label={label} className={className ? `ui-drawer ${className}` : "ui-drawer"} data-layer-surface data-layer={kind}>
      <div className="ui-drawer-lead">{lead}</div>
      {children && <div className="ui-drawer-main">{children}</div>}
    </section>
  </Region></LayerScope></FrameDrawer>;
}
