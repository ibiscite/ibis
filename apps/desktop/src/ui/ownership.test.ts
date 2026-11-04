// Screens never restyle a part (docs/design.md, "How the design changes"): a
// part's look lives once, in its own ui/ stylesheet. This guard parses every
// stylesheet's selectors and fails when a stylesheet outside ui/ styles a part.
//
// A class is a part's when a ui/ stylesheet styles it (it is in the subject, the
// last compound, of one of its selectors, directly or through :is()/:where())
// and a ui/ component renders it. So a screen region that a ui/ sheet only
// reaches into (.picker-content) and a class used only as context (an ancestor,
// :has(), :not()) stay the screen's.
//
// A screen selector restyles a part when its subject carries a part class, or
// when its subject has no class or id of its own and sits inside a part (that is
// the part's own insides, as in `.part svg`). A screen may still place its own,
// named elements inside a part, and pass a part its own class to lay it out.
import { expect, test, vi } from "vitest";

const { readFileSync, readdirSync } = await vi.importActual<{
  readFileSync: (path: URL, encoding: string) => string;
  readdirSync: (path: URL, options: { recursive: boolean }) => string[];
}>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");

const app = new FileURL("../../", import.meta.url);
const list = (dir: string, pattern: RegExp) => readdirSync(new FileURL(dir, app), { recursive: true })
  .map((name) => name.replaceAll("\\", "/")).filter((name) => pattern.test(name)).map((name) => `${dir}${name}`).sort();
const read = (name: string) => readFileSync(new FileURL(name, app), "utf8");
const isPart = (name: string) => name.startsWith("src/ui/");
// Values, not parts: the tokens and the two files that state each system's materials.
const values = new Set(["src/tokens.css", "src/nativeAppearance.css", "src/nativeAccent.css"]);

/** Every style rule's selector list, inside @media, @supports, @container and @layer, never @keyframes. */
function selectorLists(css: string): string[] {
  const text = css.replace(/\/\*[^]*?\*\//g, "");
  const found: string[] = [];
  const walk = (from: number, to: number) => {
    let start = from;
    for (let index = from; index < to; index++) {
      const char = text[index];
      if (char === ";" || char === "}") start = index + 1;
      else if (char === "\"" || char === "'") index = text.indexOf(char, index + 1);
      else if (char === "{") {
        const prelude = text.slice(start, index).trim();
        let depth = 1, end = index + 1;
        for (; end < to && depth; end++) {
          if (text[end] === "{") depth++;
          else if (text[end] === "}") depth--;
        }
        if (/^@(media|supports|container|layer|scope|document)\b/.test(prelude)) walk(index + 1, end - 1);
        else if (!prelude.startsWith("@")) found.push(prelude.replace(/\s+/g, " "));
        index = end - 1;
        start = end;
      }
    }
  };
  walk(0, text.length);
  return found;
}

/** Splits at top-level commas, outside brackets, parentheses and strings. */
function splitList(text: string): string[] {
  const parts: string[] = [];
  let depth = 0, start = 0;
  for (let index = 0; index < text.length; index++) {
    const char = text[index];
    if (char === "\\") index++;
    else if (char === "\"" || char === "'") index = text.indexOf(char, index + 1);
    else if (char === "(" || char === "[") depth++;
    else if (char === ")" || char === "]") depth--;
    else if (char === "," && depth === 0) { parts.push(text.slice(start, index)); start = index + 1; }
  }
  parts.push(text.slice(start));
  return parts.map((part) => part.trim()).filter(Boolean);
}

/** One compound selector: its classes, whether it has an id, and the selectors of :is()/:where(). */
type Compound = { classes: string[]; id: boolean; matches: Complex[] };
type Complex = Compound[];

/** Pseudo-classes whose argument names the element itself; any other argument is about something else. */
const matchesAny = new Set(["is", "where", "matches", "-webkit-any", "-moz-any"]);

function parseSelector(text: string): Complex {
  const compounds: Complex = [];
  let current: Compound = { classes: [], id: false, matches: [] };
  let dirty = false;
  const close = () => {
    if (dirty) compounds.push(current);
    current = { classes: [], id: false, matches: [] };
    dirty = false;
  };
  const ident = (from: number) => {
    let end = from;
    while (end < text.length && /[\w\-\\ -￿]/.test(text[end]!)) end += text[end] === "\\" ? 2 : 1;
    return end;
  };
  /** The index after the bracket or parenthesis that closes the one before `from`. */
  const closing = (from: number, open: string, shut: string) => {
    let depth = 1, end = from;
    for (; end < text.length && depth; end++) {
      const char = text[end];
      if (char === "\\") end++;
      else if (char === "\"" || char === "'") end = text.indexOf(char, end + 1);
      else if (char === open) depth++;
      else if (char === shut) depth--;
    }
    return end;
  };
  for (let index = 0; index < text.length;) {
    const char = text[index]!;
    if (/[\s>+~]/.test(char)) { close(); index++; continue; }
    dirty = true;
    if (char === "." || char === "#") {
      const end = ident(index + 1);
      if (char === ".") current.classes.push(text.slice(index + 1, end).replace(/\\(.)/g, "$1"));
      else current.id = true;
      index = end;
    } else if (char === "[") {
      index = closing(index + 1, "[", "]");
    } else if (char === ":") {
      const element = text[index + 1] === ":";
      const nameEnd = ident(index + (element ? 2 : 1));
      const name = text.slice(index + (element ? 2 : 1), nameEnd).toLowerCase();
      index = nameEnd;
      if (text[index] === "(") {
        const end = closing(index + 1, "(", ")");
        if (!element && matchesAny.has(name)) current.matches.push(...splitList(text.slice(index + 1, end - 1)).map(parseSelector));
        index = end;
      }
    } else {
      index = Math.max(ident(index), index + 1);
    }
  }
  close();
  return compounds;
}

/** Classes the element a selector styles must carry. */
function subjectClasses(complex: Complex): string[] {
  const subject = complex.at(-1);
  return subject ? [...subject.classes, ...subject.matches.flatMap(subjectClasses)] : [];
}
/** Whether the element a selector styles is named by a class or id of its own. */
function named(complex: Complex): boolean {
  const subject = complex.at(-1);
  return !!subject && (subject.id || subject.classes.length > 0 || (subject.matches.length > 0 && subject.matches.every(named)));
}
/** Classes of the elements the subject sits inside, through :is()/:where() too. */
function contextClasses(complex: Complex): string[] {
  const all = (compound: Compound): string[] => [...compound.classes, ...compound.matches.flatMap((inner) => inner.flatMap(all))];
  const subject = complex.at(-1);
  return [...complex.slice(0, -1).flatMap(all), ...(subject?.matches ?? []).flatMap(contextClasses)];
}

/** Every word in the ui/ components' code, comments left out: the classes they can render. */
const rendered = new Set(list("src/ui/", /\.tsx?$/).filter((name) => !/\.(test|fixture)\.tsx?$/.test(name))
  .flatMap((name) => read(name).replace(/\/\*[^]*?\*\/|\/\/[^\n]*/g, "").match(/[A-Za-z_][\w-]*/g) ?? []));

const styled = [...list("src/", /\.css$/), ...list("canvas/", /\.css$/)]
  .map((name) => [name, selectorLists(read(name)).flatMap(splitList).map((selector) => [selector, parseSelector(selector)] as const)] as const);

/** Each part class and the ui/ stylesheets that style it. */
const partClasses = new Map<string, Set<string>>();
for (const [name, selectors] of styled) {
  if (!isPart(name)) continue;
  for (const [, complex] of selectors) {
    for (const part of subjectClasses(complex).filter((word) => rendered.has(word))) {
      partClasses.set(part, (partClasses.get(part) ?? new Set()).add(name.slice("src/ui/".length)));
    }
  }
}

/** "file › selector › .part" for each screen selector that restyles a part. */
function restyles(): string[] {
  const found = styled.filter(([name]) => !isPart(name) && !values.has(name)).flatMap(([name, selectors]) => selectors.flatMap(([selector, complex]) => {
    const own = subjectClasses(complex).filter((word) => partClasses.has(word));
    const inside = named(complex) ? [] : contextClasses(complex).filter((word) => partClasses.has(word)).slice(-1);
    return [...own, ...inside].map((part) => `${name} › ${selector} › .${part}`);
  }));
  return [...new Set(found)];
}
const owner = (entry: string) => [...partClasses.get(entry.slice(entry.lastIndexOf(" › .") + 4))!].join(", ");

// RATCHET: restyles in files that open pull requests own, so they are not edited
// here. It may only shrink: the test fails when an entry no longer restyles a
// part, so whoever fixes one deletes its line. Never add to it; move the look into
// the part's ui/ stylesheet, as a variant if a context needs one.
const owned = ["src/styles.css", "src/features/picker/"];
const ratchet = [
  // styles.css: header controls take the field's height and the window's nested radius.
  "src/styles.css › .search-bar .ui-popup-button.ui-button[data-variant] › .ui-popup-button",
  "src/styles.css › .search-bar .ui-popup-button.ui-button[data-variant] › .ui-button",
  "src/styles.css › .search-bar .pin-button.ui-button[data-variant] › .ui-button",
  "src/styles.css › .search-clear.ui-button › .ui-button",
  "src/styles.css › .search-page-token > .ui-button › .ui-button",
  "src/styles.css › .search-bar .toolbar-menu-control › .toolbar-menu-control",
  // styles.css: result-row text the choice cards and options share.
  "src/styles.css › .result-meta › .result-meta",
  "src/styles.css › .result-title › .result-title",
  "src/styles.css › [data-active] .result-meta › .result-meta",
  "src/styles.css › :root[data-density=\"compact\"] .result-meta › .result-meta",
  "src/styles.css › :root[data-density=\"compact\"] .result-title › .result-title",
  // styles.css: the keys sheet, which ui/KeysSheet.css should own whole.
  "src/styles.css › .keys-backdrop › .keys-backdrop",
  "src/styles.css › .keys-dialog › .keys-dialog",
  "src/styles.css › .keys-heading › .keys-heading",
  "src/styles.css › .keys-heading h2 › .keys-heading",
  "src/styles.css › .keys-heading button › .keys-heading",
  "src/styles.css › .keys-scroll › .keys-scroll",
  "src/styles.css › .keys-scroll table › .keys-scroll",
  "src/styles.css › .keys-scroll th › .keys-scroll",
  "src/styles.css › .keys-scroll td › .keys-scroll",
  "src/styles.css › .keys-scroll th[scope=\"rowgroup\"] › .keys-scroll",
  "src/styles.css › .keys-scroll kbd › .keys-scroll",
  // features/picker/EmptyState.css: key hints that wrap under an empty state.
  "src/features/picker/EmptyState.css › .empty-keys > .key-hint › .key-hint",
  // features/picker/PickerFooter.css: the footer grid places and truncates its parts by their own classes.
  "src/features/picker/PickerFooter.css › .library-bar > :is(.ui-quiet-status, .library-action-error) › .ui-quiet-status",
  "src/features/picker/PickerFooter.css › .library-bar .ui-button.footer-action › .ui-button",
  "src/features/picker/PickerFooter.css › .library-bar .ui-button.footer-action > .key-hint › .key-hint",
];

test("screen stylesheets never restyle a part", () => {
  const listed = new Set(ratchet);
  const added = restyles().filter((entry) => !listed.has(entry));
  expect(added.map((entry) => `${entry} (ui/${owner(entry)})`),
    "These selectors outside ui/ restyle a shared part. Move the look into the part's own stylesheet, "
    + "as a variant if this context needs one, and let the screen only place the part").toEqual([]);
});

test("the restyle ratchet only shrinks", () => {
  const found = new Set(restyles());
  expect(ratchet.filter((entry) => !found.has(entry)),
    "These no longer restyle a part: delete them from the ratchet in ui/ownership.test.ts").toEqual([]);
  expect(ratchet.filter((entry) => !owned.some((prefix) => entry.startsWith(prefix))),
    "The ratchet holds only files other pull requests own; fix these instead").toEqual([]);
  expect(new Set(ratchet).size).toBe(ratchet.length);
});

test("the guard reads the element a selector styles", () => {
  const parse = (selector: string) => parseSelector(selector);
  expect(subjectClasses(parse(".shelf .ui-button"))).toEqual(["ui-button"]);
  expect(subjectClasses(parse(".ui-button .shelf-label"))).toEqual(["shelf-label"]);
  expect(subjectClasses(parse(".a > .b.c[data-x=\".d > .e\"]::after"))).toEqual(["b", "c"]);
  expect(subjectClasses(parse(".a:is(.b, .c .d):not(.e):has(> .f)"))).toEqual(["a", "b", "d"]);
  expect(subjectClasses(parse(":where(.a) + .b ~ .c"))).toEqual(["c"]);
  expect(subjectClasses(parse("main:has(> .picker-content)"))).toEqual([]);
  expect(subjectClasses(parse(".a:nth-child(2n + 1 of .b)"))).toEqual(["a"]);
  expect(named(parse(".a svg"))).toBe(false);
  expect(named(parse(".a #search"))).toBe(true);
  expect(contextClasses(parse(".a > .b svg"))).toEqual(["a", "b"]);
  expect(partClasses.has("ui-button")).toBe(true);
  expect(partClasses.has("picker-content")).toBe(false);
  expect(selectorLists("/* .x { } */ @keyframes k { from { opacity: 0 } } @media (a) { @container (b) { .y, .z { color: red } } } .w { }"))
    .toEqual([".y, .z", ".w"]);
});
