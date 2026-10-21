// Formats the chosen references as an in-text citation for the picker's
// preview and copy, in a background worker (see client.ts).
import { FormatClient } from "./client";
import type { CslItem } from "./engine";
import { STYLE_TITLES, type StyleId } from "./styles";
import { clipboardHtml, parse, plainText, type Markup } from "./markup";

export interface FormattedCitation {
  markup: Markup[];
  text: string;
  html: string;
  /** Plain sentences naming references shown with a fallback. */
  warnings: string[];
}

/** Numbers depend on every reference cited in the document, which Ibis cannot see. */
export function provisional(style: StyleId): boolean {
  return style === "ieee";
}

export const FORMATTED_STYLES = (Object.keys(STYLE_TITLES) as StyleId[]).map((id) => ({ id, title: STYLE_TITLES[id] }));

let shared: FormatClient | null = null;

/**
 * `csl` is the JSON the app returns for the chosen records, in order. Each
 * item is given its position as its id, so records sharing a citation key
 * stay apart and problems can be named. `signal` drops a preview that a newer
 * one replaced before the worker starts it.
 */
export async function formatCitation(
  csl: string,
  style: StyleId,
  { signal, client = (shared ??= new FormatClient()) }: { signal?: AbortSignal; client?: FormatClient } = {},
): Promise<FormattedCitation> {
  const parsed: unknown = JSON.parse(csl);
  if (!Array.isArray(parsed)) throw new Error("the references could not be read");
  const items: CslItem[] = parsed.map((item: CslItem, index) => ({ ...item, id: `ref-${index}` }));
  const result = await client.format({
    kind: "citation",
    style,
    format: "html",
    items,
    cites: items.map((item) => ({ id: item.id })),
  }, signal);
  const markup = parse(result.output);
  const warnings = result.problems.map((problem) => {
    const item = items.find((candidate) => candidate.id === problem.id);
    const title = typeof item?.title === "string" && item.title ? `“${item.title}”: ` : "";
    return `${title}${problem.reason}`;
  });
  return { markup, text: plainText(markup), html: clipboardHtml(markup), warnings };
}
