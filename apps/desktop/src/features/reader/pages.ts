// One model of "which pages are cited" for the thumbnails, the Page field and
// the shelf (docs/design.md, Reader; decision 0038). Pages are PDF positions;
// what a person reads and cites is the printed label. Pure, so every view
// that shows pages agrees and each rule is tested once.

/** A label is citable only if, after NFC, it is at most 16 characters of
 * letters, digits, "-" and "." (decision 0036). Anything else counts as unknown. */
export function citableLabel(raw: string | null | undefined): string | null {
  if (!raw) return null;
  const label = raw.normalize("NFC").trim();
  return label.length > 0 && label.length <= 16 && /^[\p{L}\p{N}.-]+$/u.test(label) ? label : null;
}

const romanValues: Record<string, number> = { i: 1, v: 5, x: 10, l: 50, c: 100, d: 500, m: 1000 };
/** The value of a Roman numeral such as "xii", or null. */
export function roman(label: string): number | null {
  const letters = label.toLowerCase();
  if (!/^[ivxlcdm]+$/.test(letters)) return null;
  let total = 0;
  for (let i = 0; i < letters.length; i++) {
    const value = romanValues[letters[i]!]!, next = romanValues[letters[i + 1] ?? ""] ?? 0;
    total += value < next ? -value : value;
  }
  return total > 0 ? total : null;
}

/** The number a label counts as, when it is one: "417" → 417, "xii" → 12. */
function ordinal(label: string): { kind: "arabic" | "roman"; value: number } | null {
  if (/^\d+$/.test(label)) return { kind: "arabic", value: Number(label) };
  const value = roman(label);
  return value === null ? null : { kind: "roman", value };
}

/** Whether `b` is the page printed after `a`. */
function follows(a: string, b: string) {
  const x = ordinal(a), y = ordinal(b);
  return Boolean(x && y && x.kind === y.kind && y.value === x.value + 1);
}

const dash = "–";

/** The cited pages as a person writes them: "417–419, 423". Neighbouring PDF
 * pages whose printed numbers follow each other join into a range; a page
 * without a citable label is left out, since it cannot be cited by number. */
export function formatPages(labels: readonly (string | null)[], selected: ReadonlySet<number>): string {
  const pages = [...selected].filter((index) => labels[index]).sort((a, b) => a - b);
  const runs: number[][] = [];
  for (const index of pages) {
    const run = runs.at(-1), last = run?.at(-1);
    if (run && last === index - 1 && follows(labels[last]!, labels[index]!)) run.push(index);
    else runs.push([index]);
  }
  return runs.map((run) => run.length === 1 ? labels[run[0]!]! : `${labels[run[0]!]}${dash}${labels[run.at(-1)!]}`).join(", ");
}

/** The PDF pages a typed Page value names, and the parts no page is printed as.
 * "417–419, 423", "417-419; 423" and "XII" all read; nothing is guessed. A label
 * printed on several pages names the first. */
export function parsePages(text: string, labels: readonly (string | null)[]): { selected: Set<number>; unknown: string[] } {
  const selected = new Set<number>(), unknown: string[] = [];
  const find = (label: string) => labels.findIndex((candidate) => candidate !== null && candidate.toLowerCase() === label.toLowerCase());
  for (const part of text.split(/[,;]/).map((part) => part.trim()).filter(Boolean)) {
    const [from, to, ...rest] = part.split(/\s*[-–—]\s*/);
    const start = from ? find(from) : -1;
    const end = to === undefined ? start : find(to);
    if (start < 0 || end < start || rest.length) { unknown.push(part); continue; }
    for (let index = start; index <= end; index++) if (labels[index]) selected.add(index);
  }
  return { selected, unknown };
}

/** Labels for the pages without one, from a printed number the person
 * confirmed on one of them: only the unlabelled run around it is numbered,
 * since front matter, gaps and supplements break any single offset. */
export function confirmOffset(labels: readonly (string | null)[], index: number, printed: string): (string | null)[] {
  const value = citableLabel(printed);
  if (value === null || labels[index] !== null || !/^\d+$/.test(value)) return [...labels];
  const next = [...labels];
  let start = index, end = index;
  while (start > 0 && labels[start - 1] === null) start--;
  while (end < labels.length - 1 && labels[end + 1] === null) end++;
  for (let i = start; i <= end; i++) {
    const number = Number(value) + i - index;
    if (number > 0) next[i] = String(number);
  }
  return next;
}
