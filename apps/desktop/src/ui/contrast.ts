/** CSS colours are composited before WCAG's sRGB luminance calculation. */
export type Colour = readonly [number, number, number, number];
export const over = (ink: Colour, paper: Colour): Colour => {
  const alpha = ink[3] + paper[3] * (1 - ink[3]);
  return [0, 1, 2].map((i) => alpha ? (ink[i]! * ink[3] + paper[i]! * paper[3] * (1 - ink[3])) / alpha : 0).concat(alpha) as unknown as Colour;
};
export function contrast(ink: Colour, paper: Colour): number {
  const luminance = (rgb: Colour) => rgb.slice(0, 3).reduce((sum, value, i) => {
    const c = value / 255;
    return sum + [.2126, .7152, .0722][i]! * (c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4);
  }, 0);
  const a = luminance(over(ink, paper)), b = luminance(paper);
  return (Math.max(a, b) + .05) / (Math.min(a, b) + .05);
}
/** Split CSS function arguments without splitting a nested function. */
function args(value: string) {
  let depth = 0, start = 0;
  const found: string[] = [];
  [...value].forEach((c, i) => {
    if (c === "(") depth++;
    if (c === ")") depth--;
    if (c === "," && depth === 0) { found.push(value.slice(start, i).trim()); start = i + 1; }
  });
  return [...found, value.slice(start).trim()];
}
/** Resolve the same role graph the browser uses, including alpha fills. Unknown
 * syntax fails closed, so a new colour cannot silently leave the check. */
export function colour(value: string, tokens: ReadonlyMap<string, string>, dark: boolean, stack: string[] = []): Colour {
  const read = (v: string) => colour(v.trim(), tokens, dark, stack);
  if (value.startsWith("var(")) {
    const [name, fallback] = args(value.slice(4, -1));
    if (stack.includes(name!)) throw new Error(`Cyclic colour: ${name}`);
    const next = tokens.get(name!) ?? fallback;
    if (!next) throw new Error(`Unknown colour: ${name}`);
    return colour(next, tokens, dark, [...stack, name!]);
  }
  if (value.startsWith("light-dark(")) return read(args(value.slice(11, -1))[dark ? 1 : 0]!);
  if (value.startsWith("color-mix(")) {
    const [, first, second] = args(value.slice(10, -1));
    const part = (v: string) => { const m = /^(.*)\s+([\d.]+)%$/.exec(v); return { c: read(m?.[1] ?? v), weight: m ? Number(m[2]) / 100 : undefined }; };
    const a = part(first!), b = part(second!), weight = a.weight ?? 1 - (b.weight ?? .5);
    const alpha = a.c[3] * weight + b.c[3] * (1 - weight);
    return [0, 1, 2].map((i) => alpha ? (a.c[i]! * a.c[3] * weight + b.c[i]! * b.c[3] * (1 - weight)) / alpha : 0).concat(alpha) as unknown as Colour;
  }
  if (value === "transparent") return [0, 0, 0, 0];
  const system: Record<string, string> = { Canvas: dark ? "#121212" : "#ffffff", CanvasText: dark ? "#ffffff" : "#000000", Highlight: dark ? "#0059d1" : "#0064e1", HighlightText: "#ffffff", white: "#ffffff", black: "#000000" };
  if (system[value]) return read(system[value]);
  if (/^#(?:[\da-f]{3,4}|[\da-f]{6}|[\da-f]{8})$/i.test(value)) {
    const hex = value.length <= 5 ? [...value.slice(1)].map((c) => c + c).join("") : value.slice(1);
    return [0, 2, 4].map((i) => Number.parseInt(hex.slice(i, i + 2), 16)).concat(hex.length === 8 ? Number.parseInt(hex.slice(6), 16) / 255 : 1) as unknown as Colour;
  }
  const rgb = /^rgba?\(([^)]+)\)$/.exec(value);
  if (rgb) {
    const parts = rgb[1]!.split(/[\s,/]+/).filter(Boolean);
    const channels = parts.slice(0, 3).map((part) => part.endsWith("%") ? Number.parseFloat(part) * 2.55 : Number(part));
    if (channels.length === 3 && channels.every(Number.isFinite)) return [...channels, parts[3] ? Number.parseFloat(parts[3]) / (parts[3].endsWith("%") ? 100 : 1) : 1] as unknown as Colour;
  }
  throw new Error(`Unknown colour: ${value}`);
}
export type ContrastPair = { name: string; ink: Colour; paper: Colour; kind: "text" | "icon" | "focus" };
export const contrastFailures = (pairs: readonly ContrastPair[]) => pairs.flatMap((pair) => {
  const ratio = contrast(pair.ink, pair.paper), floor = pair.kind === "text" ? 4.5 : 3;
  return !Number.isFinite(ratio) || ratio < floor ? [`${pair.name}: ${ratio.toFixed(2)} < ${floor}:1`] : [];
});
