import type { ComponentPropsWithRef } from "react";
import { mergeProps, useHover } from "react-aria";

/** Virtual-focus options keep their caller's real-motion hover policy. */
export function Option({ hoverAllowed = false, ...props }: ComponentPropsWithRef<"li"> & { hoverAllowed?: boolean }) {
  const { hoverProps, isHovered } = useHover({});
  return <li {...mergeProps(hoverProps, props)} data-hovered={hoverAllowed && isHovered || undefined} />;
}
