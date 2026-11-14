import type { ComponentPropsWithRef, ReactNode } from "react";
import { mergeProps, useHover } from "react-aria";
import { Band } from "./Band";
import "./Option.css";

/** A list row: a band with a fill (ui/Band), its glyph in the icon column.
 * Virtual-focus options keep their caller's real-motion hover policy. */
export function Option({ hoverAllowed = false, className, icon, ...props }: ComponentPropsWithRef<"li"> & { hoverAllowed?: boolean; icon: ReactNode }) {
  const { hoverProps, isHovered } = useHover({});
  return <Band as="li" fill icon={icon} {...mergeProps(hoverProps, props) as ComponentPropsWithRef<"div">}
    className={className ? `ui-option ${className}` : "ui-option"} data-hovered={hoverAllowed && isHovered || undefined} />;
}
