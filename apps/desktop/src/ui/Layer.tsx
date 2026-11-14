import { useContext, type ComponentProps, type ReactNode } from "react";
import { Popover } from "react-aria-components";
import { LayerParent, useLayer, type LayerClose, type LayerHandle } from "./layers";
import "./Overlay.css";

/** What renders inside a layer belongs to it: a menu opened in a popover or a
 * sheet leaves that one open (ui/layers). */
export function LayerScope({ layer, children }: { layer: LayerHandle; children: ReactNode }) {
  return <LayerParent value={layer.scope}>{children}</LayerParent>;
}

/** The one way to float something over the window: a React Aria popover whose
 * being open is a layer on the stack, so opening it closes any other menu or
 * popover, and Esc and Back reach it first. `open` is what is shown; React Aria's
 * own trigger may still hold the state, as a menu's does. */
export function Floating({ kind, open, onClose, children, ...popover }: Omit<ComponentProps<typeof Popover>, "children"> & {
  kind: "menu" | "popover"; open: boolean; onClose: LayerClose; children: ReactNode;
}) {
  const layer = useLayer(kind, open, onClose);
  // Marked when it opened inside another layer, which stays open under it.
  return <Popover {...popover} data-layer-surface data-layer={kind} data-layer-within={useContext(LayerParent) !== null || undefined}>
    <LayerScope layer={layer}>{children}</LayerScope>
  </Popover>;
}
