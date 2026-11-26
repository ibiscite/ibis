import { useLayoutEffect, type HTMLAttributes, type ReactNode, type Ref } from "react";
import { FrameDrawer } from "./Frame";
import { LayerScope } from "./Layer";
import { useLayer, type LayerClose } from "./layers";
import "./Drawer.css";

/** Details that open up from the delivery bar, full width, in the frame's own
 * flow: the list gives them its end, so they never cover a row, and the list's
 * highlighted row stays in view above them. Not modal: everything around them
 * stays live, and the picker's layers own Esc, so the caller closes it. What the
 * drawer is about (`lead`) sits beside its fields when the window has room. */
export function Drawer({ ref, kind = "popover", label, role = "dialog", className, lead, children, onClose, ...section }: {
  ref?: Ref<HTMLElement>;
  /** A Peek is a menu on the view stack: it never takes focus, and any other surface closes it. */
  kind?: "popover" | "menu"; label: string; role?: "dialog" | "region"; className?: string;
  lead: ReactNode; children?: ReactNode; onClose: LayerClose;
} & Omit<HTMLAttributes<HTMLElement>, "role" | "children">) {
  const layer = useLayer(kind, true, onClose);
  // The list is shorter now: its highlighted row stays where the person can see it.
  useLayoutEffect(() => { document.querySelector('[role="listbox"] [aria-selected="true"]')?.scrollIntoView?.({ block: "nearest" }); }, []);
  return <FrameDrawer><LayerScope layer={layer}>
    <section {...section} ref={ref} role={role} aria-label={label} className={className ? `ui-drawer ${className}` : "ui-drawer"} data-layer-surface data-layer={kind}>
      <div className="ui-drawer-lead">{lead}</div>
      {children && <div className="ui-drawer-main">{children}</div>}
    </section>
  </LayerScope></FrameDrawer>;
}
