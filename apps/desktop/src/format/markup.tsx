// Formatted citations come from the engine as HTML. That HTML carries
// reference text, so it is untrusted: it is read into a small tree of the
// markup the engine is known to emit, and everything else is dropped. The
// preview and the clipboard are built from that tree, never from the HTML.
import { createElement, type CSSProperties, type ReactNode } from "react";

type Tag = "i" | "b" | "sup" | "sub" | "span";
type Block = "entry" | "margin" | "inline" | "block" | "indent";

export type Markup = string | { tag: Tag; style?: Style; children: Markup[] } | { block: Block; children: Markup[] };
export type CitationPart = { item: number | null; markup: Markup[] };

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

/** Display-only ownership from the engine's byte-neutral annotated pass. */
export function parseSegments(html: string, count: number): CitationPart[] | null {
  if (!Number.isSafeInteger(count) || count < 0 || count > 1000 || html.length > 1024 * 1024) return null;
  const document = new DOMParser().parseFromString(html, "text/html");
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  const parts: CitationPart[] = [];
  let visited = 0;
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (++visited > 10000) return null;
    let item: number | null = null;
    let markup: Markup[] = node.textContent ? [node.textContent] : [];
    let depth = 0;
    for (let parent = node.parentElement; parent && parent !== document.body; parent = parent.parentElement) {
      if (++depth > 256) return null;
      const name = parent.localName;
      if (["script", "style", "template"].includes(name)) { markup = []; break; }
      if (name === "ibis-reference") {
        const value = parent.getAttribute("data-item") ?? "";
        const owner = Number(value);
        if (!/^(0|[1-9]\d*)$/.test(value) || parent.attributes.length !== 1 || owner >= count
          || (item !== null && item !== owner)) return null;
        item = owner;
      } else markup = wrapElement(parent, markup);
    }
    if (markup.length) parts.push({ item, markup });
  }
  // A reference may wrap several variables. Coalesce only its uninterrupted
  // interval; overlapping owners cannot safely become individually removable.
  const ranges = new Map<number, [number, number]>();
  parts.forEach((part, index) => {
    if (part.item !== null) ranges.set(part.item, [ranges.get(part.item)?.[0] ?? index, index]);
  });
  const ambiguous = new Set<number>();
  let cluster: number[] = [];
  let end = -1;
  const finish = () => { if (cluster.length > 1) for (const item of cluster) ambiguous.add(item); };
  for (const [item, [first, last]] of ranges) {
    if (first > end) { finish(); cluster = []; }
    cluster.push(item);
    end = Math.max(first > end ? -1 : end, last);
  }
  finish();
  for (const [item, [first, last]] of ranges) {
    if (!ambiguous.has(item)) for (let index = first; index <= last; index++) parts[index]!.item = item;
  }
  const merged: CitationPart[] = [];
  for (const part of parts) {
    if (part.item !== null && ambiguous.has(part.item)) part.item = null;
    const last = merged.at(-1);
    if (last?.item === part.item) for (const node of part.markup) last.markup.push(node);
    else merged.push(part);
  }
  return merged;
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

/**
 * The markup as HTML for the clipboard: inline styles only, one paragraph per
 * entry, and a numbered style's margin written inline with its entry.
 */
export function clipboardHtml(markup: Markup[]): string {
  return markup.map(htmlOf).join("");
}

function htmlOf(node: Markup): string {
  if (typeof node === "string") return escapeHtml(node);
  const inner = node.children.map(htmlOf).join("");
  if ("block" in node) {
    switch (node.block) {
      case "entry":
        return `<p style="margin:0">${inner.trim()}</p>`;
      case "margin":
        return `${inner.trim()} `;
      default:
        return inner;
    }
  }
  const style = node.style ? ` style="${node.style.join(";")}"` : "";
  return `<${node.tag}${style}>${inner}</${node.tag}>`;
}

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

/** The markup as plain text: one line per entry, a margin followed by a space. */
export function plainText(markup: Markup[]): string {
  return markup.map(textOf).join("").replace(/\n+$/, "");
}

function textOf(node: Markup): string {
  if (typeof node === "string") return node;
  const inner = node.children.map(textOf).join("");
  if ("block" in node && node.block === "entry") return `${inner.trim()}\n`;
  if ("block" in node && node.block === "margin") return `${inner.trim()} `;
  return inner;
}
