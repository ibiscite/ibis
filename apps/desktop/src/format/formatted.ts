// Formats the chosen references as an in-text citation for the picker's
// preview and copy, in a background worker (see client.ts).
import { FormatClient } from "./client";
import type { CiteItem, CslItem } from "./engine";
import { copiesEntry, STYLE_NAMES, STYLE_TITLES, type StyleId } from "./styles";
import { pickerText } from "../messages";
import { clipboardHtml, parse, parseSegments, plainText, type CitationPart, type Markup } from "./markup";

export interface FormattedCitation {
  markup: Markup[];
  text: string;
  html: string;
  /** Plain sentences naming references shown with a fallback. */
  warnings: string[];
  segments?: CitationPart[];
  unrepresented?: number[];
}

export const FORMATTED_STYLES = (Object.keys(STYLE_TITLES) as StyleId[]).map((id) => ({ id, title: STYLE_TITLES[id] }));

let shared: FormatClient | null = null;

/**
 * `csl` is the JSON the app returns for the chosen records, in order. Each
 * item is given its position as its id, so records sharing a citation key
 * stay apart and problems can be named. Optional `cites` supplies one citation
 * context per reference, in the same order; generated IDs always take priority.
 * `signal` drops a preview that a newer one replaced before the worker starts it.
 */
export async function formatCitation(
  csl: string,
  style: StyleId,
  { signal, client = (shared ??= new FormatClient()), segments = false, cites }: {
    signal?: AbortSignal; client?: FormatClient; segments?: boolean; cites?: readonly Omit<CiteItem, "id">[];
  } = {},
): Promise<FormattedCitation> {
  const parsed: unknown = JSON.parse(csl);
  if (!Array.isArray(parsed)) throw new Error("the references could not be read");
  if (cites !== undefined && cites.length !== parsed.length) throw new Error("the citation details do not match the references");
  const items: CslItem[] = parsed.map((item: CslItem, index) => ({ ...item, id: `ref-${index}` }));
  if (copiesEntry(style)) return formatEntries(items, parsed, style, client, signal, segments, cites);
  const result = await client.format({
    kind: "citation",
    style,
    format: "html",
    items,
    cites: items.map((item, index) => ({ ...cites?.[index], id: item.id })),
    ...(segments ? { segments: true } : {}),
  }, signal);
  const markup = parse(result.output);
  const warnings = named(result.problems, items);
  const text = plainText(markup);
  const parts = segments && result.preview ? parseSegments(result.preview, items.length) : null;
  const safe = parts && plainText(parts.flatMap((part) => part.markup)) === text ? parts : [{ item: null, markup }];
  const represented = new Set(safe.map((part) => part.item));
  return { markup, text, html: clipboardHtml(markup), warnings, ...(segments ? {
    segments: safe, unrepresented: items.map((_, index) => index).filter((index) => !represented.has(index)),
  } : {}) };
}

async function formatEntries(
  items: CslItem[], original: readonly CslItem[], style: StyleId, client: FormatClient, signal: AbortSignal | undefined, segments: boolean,
  cites: readonly Omit<CiteItem, "id">[] | undefined,
): Promise<FormattedCitation> {
  const result = await client.format({ kind: "bibliography", style, format: "html", items }, signal);
  const formatted = withoutNumbers(parse(result.output)).filter((node) => typeof node !== "string" || node.trim() !== "");
  const warnings = named(result.problems, items);
  if (cites?.some((cite) => cite.locator)) warnings.push(pickerText.entryLocator(STYLE_NAMES[style]));
  const known = result.order && result.order.length === formatted.length ? result.order : undefined;
  // A reference with no field the style prints leaves an empty entry, which
  // would copy as nothing; copy its title or key instead, and say so.
  const entries: Markup[] = [];
  const order: (number | null)[] = [];
  formatted.forEach((entry, index) => {
    const item = known?.[index];
    if (plainText([entry]).trim() !== "") {
      entries.push(entry);
      order.push(item ?? null);
      return;
    }
    const reference = item === undefined ? undefined : original[item];
    const shown = typeof reference?.title === "string" && reference.title.trim() ? reference.title.trim() : typeof reference?.id === "string" ? reference.id.trim() : "";
    if (item === undefined || !shown) {
      warnings.push(pickerText.entryNothing(STYLE_NAMES[style]));
      return;
    }
    warnings.push(pickerText.entryEmpty(STYLE_NAMES[style], shown));
    entries.push({ block: "entry", children: [shown] });
    order.push(item);
  });
  const markup = entries;
  const text = plainText(markup);
  const parts: CitationPart[] = known
    ? entries.map((entry, index) => ({ item: order[index] ?? null, markup: [entry] }))
    : [{ item: null, markup }];
  const represented = new Set(parts.map((part) => part.item));
  return { markup, text, html: clipboardHtml(markup), warnings, ...(segments ? {
    segments: parts, unrepresented: items.map((_, index) => index).filter((index) => !represented.has(index)),
  } : {}) };
}

/** Drop a numeric style's entry numbers; they would not match the document. */
function withoutNumbers(markup: Markup[]): Markup[] {
  return markup.flatMap((node): Markup[] => {
    if (typeof node === "string") return [node];
    if ("block" in node) {
      if (node.block === "margin") return [];
      const children = withoutNumbers(node.children);
      return node.block === "inline" ? children : [{ ...node, children }];
    }
    return [{ ...node, children: withoutNumbers(node.children) }];
  });
}

function named(problems: readonly { id: string; reason: string }[], items: readonly CslItem[]): string[] {
  return problems.map((problem) => {
    const item = items.find((candidate) => candidate.id === problem.id);
    const title = typeof item?.title === "string" && item.title ? `“${item.title}”: ` : "";
    return `${title}${problem.reason}`;
  });
}
