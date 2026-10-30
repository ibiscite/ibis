import type { ComponentPropsWithRef } from "react";
import { mergeProps, useHover } from "react-aria";
import "./Option.css";

/** Virtual-focus options keep their caller's real-motion hover policy. */
export function Option({ hoverAllowed = false, className, ...props }: ComponentPropsWithRef<"li"> & { hoverAllowed?: boolean }) {
  const { hoverProps, isHovered } = useHover({});
  return <li {...mergeProps(hoverProps, props)} className={["ui-option", className].filter(Boolean).join(" ")} data-hovered={hoverAllowed && isHovered || undefined} />;
}
