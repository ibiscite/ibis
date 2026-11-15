import type { ComponentPropsWithRef, CSSProperties } from "react";
import "./Stack.css";

export type Space = `--space-${1 | 2 | 3 | 4 | 5 | 6 | 8 | 10}`;

/** One column; the spacing scale is its only source of gaps. */
export function Stack({ as: Tag = "div", gap, shortGap, className, ...props }: {
  as?: "div" | "section" | "span" | "ul" | "p";
  gap?: Space;
  shortGap?: Space;
} & Omit<ComponentPropsWithRef<"div">, "style">) {
  return <Tag {...props as object} className={className ? `ui-stack ${className}` : "ui-stack"}
    style={{ "--stack-gap": gap ? `var(${gap})` : 0, "--stack-short-gap": shortGap && `var(${shortGap})` } as CSSProperties} />;
}
