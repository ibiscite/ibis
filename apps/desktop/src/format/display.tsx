import { Fragment, createElement, isValidElement, type ReactNode } from "react";
import type { DisplayNode, DisplayRun, DisplayTree } from "../bindings";
import "../ui/Display.css";

/** Engine display leaves are inert style data, never HTML to evaluate. */
function renderRun(run: DisplayRun, key: number, text: ReactNode): ReactNode {
  let node: ReactNode = text;
  if (run.italic) node = createElement("i", null, node);
  if (run.bold) node = createElement("b", null, node);
  if (run.script === "superscript" || run.script === "subscript") node = createElement(run.script === "superscript" ? "sup" : "sub", null, node);
  return createElement("span", { key, className: "display-run", style: {
    ...(run.small_caps ? { fontVariant: "small-caps" } : {}),
    ...(run.underline ? { textDecoration: "underline" } : {}),
  } }, node);
}

/** Block ranges use run indices, so Unicode and styled resets need no parsing. */
export function renderNode(node: DisplayNode, text: (value: string) => ReactNode = (value) => value): ReactNode[] {
  let blockIndex = 0;
  const content = (start: number, end: number): ReactNode[] => {
    const children: ReactNode[] = [];
    for (let run = start; run < end;) {
      const block = node.blocks[blockIndex];
      if (block && block.start === run && block.end <= end) {
        const key = blockIndex++;
        const nested = content(block.start, block.end);
        const margin = block.kind === "entry" && nested.some((child) => isValidElement<{ className: string }>(child) && child.props.className === "csl-margin");
        children.push(createElement("div", { key: `block-${key}`, className: `csl-${block.kind}`, "data-margin": margin || undefined }, ...nested));
        run = block.end;
      } else {
        children.push(renderRun(node.content[run]!, run, text(node.content[run]!.text)));
        run++;
      }
    }
    return children;
  };
  return content(0, node.content.length);
}

export function renderDisplay(tree: DisplayTree, text?: (value: string) => ReactNode): ReactNode[] {
  return tree.nodes.map((node, key) => createElement(Fragment, { key }, ...renderNode(node, text)));
}
