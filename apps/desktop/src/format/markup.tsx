// Formatted citations come from the engine as HTML. That HTML carries
// reference text, so it is untrusted: it is read into a small tree of the
// markup the engine is known to emit, and everything else is dropped. The
// preview is built from that tree; clipboard content stays in the engine.
import { createElement, type CSSProperties, type ReactNode } from "react";
import type { DisplayRun } from "../bindings";

type Tag = "i" | "b" | "sup" | "sub" | "span";
type Block = "entry" | "margin" | "inline" | "block" | "indent";

/** Engine display leaves are inert style data, never HTML to evaluate. */
export function renderRuns(runs: readonly DisplayRun[]): ReactNode[] {
  return runs.map((run, index) => {
    let node: ReactNode = run.text;
    if (run.italic) node = createElement("i", null, node);
    if (run.bold) node = createElement("b", null, node);
    if (run.script === "superscript" || run.script === "subscript") node = createElement(run.script === "superscript" ? "sup" : "sub", null, node);
    return createElement("span", { key: index, style: {
      ...(run.small_caps ? { fontVariant: "small-caps" } : {}),
      ...(run.underline ? { textDecoration: "underline" } : {}),
    } }, node);
  });
}

export type Markup = string | { tag: Tag; style?: Style; children: Markup[] } | { block: Block; children: Markup[] };

/** The only CSS declarations kept, each written as the engine writes it. */
const STYLES = {
  "font-style:italic": { fontStyle: "italic" },
  "font-style:normal": { fontStyle: "normal" },
  "font-weight:bold": { fontWeight: "bold" },
  "font-weight:normal": { fontWeight: "normal" },
  "font-variant:small-caps": { fontVariant: "small-caps" },
  "font-variant:normal": { fontVariant: "normal" },
  "text-decoration:underline": { textDecoration: "underline" },
  "text-decoration:none": { textDecoration: "none" },
  "vertical-align:sup": { verticalAlign: "super" },
  "vertical-align:sub": { verticalAlign: "sub" },
  "vertical-align:baseline": { verticalAlign: "baseline" },
} as const satisfies Record<string, CSSProperties>;

type Declaration = keyof typeof STYLES;
type Style = Declaration[];

const BLOCKS: Record<string, Block> = {
  "csl-entry": "entry",
  "csl-left-margin": "margin",
  "csl-right-inline": "inline",
  "csl-block": "block",
  "csl-indent": "indent",
};

const TAGS = new Set<string>(["i", "b", "sup", "sub", "span"]);

/** Read engine HTML into markup, keeping only known tags and styles. */
export function parse(html: string): Markup[] {
  const document = new DOMParser().parseFromString(html, "text/html");
  return children(document.body);
}

function children(node: Node): Markup[] {
  const out: Markup[] = [];
  for (const child of node.childNodes) {
    if (child.nodeType === Node.TEXT_NODE) {
      if (child.textContent) out.push(child.textContent);
    } else if (child instanceof Element) {
      out.push(...element(child));
    }
  }
  return out;
}

function element(node: Element): Markup[] {
  const name = node.tagName.toLowerCase();
  // Never show the text of code or style sheets.
  if (name === "script" || name === "style" || name === "template") return [];
  const inner = children(node);
  return wrapElement(node, inner);
}

function wrapElement(node: Element, inner: Markup[]): Markup[] {
  const name = node.tagName.toLowerCase();
  if (name === "div") {
    const block = [...node.classList].filter((name) => Object.hasOwn(BLOCKS, name)).map((name) => BLOCKS[name])[0];
    return block ? [{ block, children: inner }] : inner;
  }
  if (!TAGS.has(name)) return inner;
  const style = declarations(node.getAttribute("style") ?? "");
  if (name === "span" && style.length === 0) return inner;
  return [{ tag: name as Tag, ...(style.length ? { style } : {}), children: inner }];
}

function declarations(style: string): Style {
  return style
    .split(";")
    .map((part) => part.replace(/\s+/g, "").toLowerCase())
    .filter((part): part is Declaration => Object.hasOwn(STYLES, part));
}

function css(style: Style | undefined): CSSProperties | undefined {
  return style && Object.assign({}, ...style.map((part) => STYLES[part]));
}

/** The markup as React elements for the preview. */
export function render(markup: Markup[]): ReactNode[] {
  return markup.map((node, index) => {
    if (typeof node === "string") return node;
    if ("block" in node) {
      return createElement("div", { key: index, className: `csl-${node.block}` }, ...render(node.children));
    }
    return createElement(node.tag, { key: index, style: css(node.style) }, ...render(node.children));
  });
}
