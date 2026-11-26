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
// Values, not parts: the tokens.
const values = new Set(["src/tokens.css"]);

/** Comments blanked to spaces, so offsets and line numbers still point into the file. */
const uncomment = (css: string) => css.replace(/\/\*[^]*?\*\//g, (comment) => comment.replace(/[^\n]/g, " "));
const lineAt = (text: string, offset: number) => text.slice(0, offset).split("\n").length;

/** Every style rule (selector list, line, declarations), inside @media, @supports, @container and @layer, never @keyframes. */
function styleRules(css: string): { selector: string; line: number; body: string }[] {
  const text = uncomment(css);
  const found: { selector: string; line: number; body: string }[] = [];
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
        else if (!prelude.startsWith("@")) {
          const at = start + text.slice(start, index).search(/\S/);
          found.push({ selector: prelude.replace(/\s+/g, " "), line: lineAt(text, at), body: text.slice(index + 1, end - 1) });
        }
        index = end - 1;
        start = end;
      }
    }
  };
  walk(0, text.length);
  return found;
}
/** Every style rule's selector list. */
const selectorLists = (css: string): string[] => styleRules(css).map(({ selector }) => selector);

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
  .map((name) => [name, styleRules(read(name)).flatMap(({ selector, line, body }) => splitList(selector)
    .map((one) => [one, parseSelector(one), line, body] as const))] as const);

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

test("screen stylesheets never restyle a part", () => {
  expect(restyles().map((entry) => `${entry} (ui/${owner(entry)})`),
    "These selectors outside ui/ restyle a shared part. Move the look into the part's own stylesheet, "
    + "as a variant if this context needs one, and let the screen only place the part").toEqual([]);
});

/** [Layout] Where things sit is decided once: the layout
 * tokens in tokens.css and the parts in ui/ that read them. Outside them nothing
 * sets a margin, padding, gap, inset or width, in a stylesheet or a style prop,
 * whatever the value; a screen composes parts (ui/Layout, ui/ViewFrame and the
 * others) and gets its spacing from them. There is no list of exceptions. */
const spacing = /^(margin|padding|gap|row-gap|column-gap|inset|top|right|bottom|left|width|min-width|max-width|inline-size|min-inline-size|max-inline-size)$|^(margin|padding|inset)-[a-z-]+$/;
const spacingProp = /^(margin|padding|gap|rowGap|columnGap|inset|top|right|bottom|left|width|minWidth|maxWidth|inlineSize|minInlineSize|maxInlineSize)$|^(margin|padding|inset)[A-Z]\w*$/;
function spacingDeclarations(css: string): string[] {
  const text = css.replace(/\/\*[^]*?\*\//g, "");
  // Declarations only: a property name and value inside a block, never a selector or an at-rule's query.
  return [...text.matchAll(/[{;]\s*([a-z][a-z-]*)\s*:\s*([^;{}]+)(?=[;}])/g)]
    .filter(([, property]) => spacing.test(property!)).map(([, property, value]) => `${property}: ${value!.trim()}`);
}
/** What an element's style prop sets. An element is a lower-case JSX tag; a
 * component's `style` (a citation style, say) is its own prop. The value must be
 * an object literal the guard can read: a style passed by reference, built by an
 * expression, or spread from elsewhere is reported as unreadable, since it could
 * carry spacing. Custom properties ("--name") are values, not spacing. */
function spacingProps(source: string): string[] {
  const text = source.replace(/\/\*[^]*?\*\/|\/\/[^\n]*/g, "");
  const found: string[] = [];
  for (const match of text.matchAll(/<([A-Za-z][\w.]*)\b[^<>]*?\sstyle=\{/g)) {
    if (!/^[a-z]/.test(match[1]!)) continue;
    // The expression inside style={…}, to its balancing brace.
    let depth = 1, end = match.index! + match[0].length;
    const start = end;
    for (; end < text.length && depth; end++) {
      if (text[end] === "{") depth++;
      else if (text[end] === "}") depth--;
    }
    const expression = text.slice(start, end - 1).trim().replace(/\s+as\s+[\w.<>[\]|& ]+$/, "").trim();
    if (!expression.startsWith("{") || !expression.endsWith("}")) { found.push(`unreadable ${expression}`); continue; }
    const body = expression.slice(1, -1);
    if (/\.\.\./.test(body)) found.push(`unreadable ${expression}`);
    for (const [, quote, name] of body.matchAll(/(?:^|,)\s*(["']?)([-\w]+)\1\s*:/g)) {
      if (!quote || !name!.startsWith("--")) { if (spacingProp.test(name!)) found.push(name!); }
    }
  }
  return found;
}
const screenSheets = list("src/", /\.css$/).filter((name) => !isPart(name) && !values.has(name));
const screenSources = list("src/", /\.tsx$/).filter((name) => !isPart(name) && !/\.test\.tsx$/.test(name));

test("only tokens.css and ui/ parts set spacing: no margin, padding, gap, inset or width anywhere else", () => {
  expect(screenSheets.flatMap((name) => spacingDeclarations(read(name)).map((found) => `${name}: ${found}`)),
    "Spacing outside ui/: compose the screen from ui/ parts (Section, Group, Row, ViewFrame…) or add what it needs to a part").toEqual([]);
  expect(screenSources.flatMap((name) => spacingProps(read(name)).map((found) => `${name}: style ${found}`)),
    "A style prop outside ui/ sets spacing: take it from a ui/ part").toEqual([]);
});

test("the layout guard catches spacing in a sheet or a style prop, and passes what only lays parts out", () => {
  expect(spacingDeclarations(".a { margin: 0; padding-inline: var(--inset); gap: 4px } .b { inset-block-start: 0; min-width: 0; width: 100% }")).toHaveLength(6);
  expect(spacingDeclarations("@media (max-width: 400px) { .a { display: grid; grid-area: x; --row-height: var(--row-compact); color: red } }")).toEqual([]);
  expect(spacingProps('<span style={{ marginInlineStart: 4, colorScheme: "dark" }} /><b style={{ width: "1rem" }} />')).toEqual(["marginInlineStart", "width"]);
  expect(spacingProps("<span style={{ backgroundImage: `url(x)` }} />")).toEqual([]);
  // A style by reference, behind a cast, or spread can carry spacing the guard can't see.
  expect(spacingProps("<div style={layout} /><div style={{ padding: 24 } as CSSProperties} /><p style={{ ...base, color: \"red\" }} />"))
    .toEqual(["unreadable layout", "padding", "unreadable { ...base, color: \"red\" }"]);
  // Custom properties are values; a component's own style prop is not an element's.
  expect(spacingProps('<span style={{ "--preview-window-w": w } as CSSProperties} /><PickerOverlays style={styleOf(format)} />')).toEqual([]);
});

/** A screen's own look: spacing, colour or a corner it declares instead of taking it from a part. */
const look = /^(padding|margin|gap|row-gap|column-gap|color|background|outline|box-shadow|fill|stroke|caret-color|accent-color|text-decoration-color|border)(-[a-z-]+)?$/;
const neutral = /^(0|0px|auto|none|inherit|initial|unset|transparent|currentcolor)( (0|0px|auto))?$/i;
function ownLook(css: string): string[] {
  return [...css.replace(/\/\*[^]*?\*\//g, "").matchAll(/([a-z][a-z-]*)\s*:\s*([^;{}]+)(?=[;}])/g)]
    .filter(([, property, value]) => look.test(property!) && !neutral.test(value!.trim())).map(([declaration]) => declaration.replace(/\s+/g, " "));
}

test("screens take their colours and corners from parts, never their own", () => {
  expect(screenSheets.flatMap((name) => ownLook(read(name)).map((found) => `${name}: ${found}`)),
    "These screens bring their own colour or corner. Draw them with a part from ui/, or add the look to the part").toEqual([]);
});

test("the look guard fails a screen that pads, paints or rounds itself, and passes one that only lays parts out", () => {
  expect(ownLook(".my-screen { padding: var(--space-2); color: var(--text-secondary); border-radius: var(--r-control); }")).toHaveLength(3);
  expect(ownLook(".my-screen { display: grid; grid-template-columns: 1fr auto; background: transparent; color: inherit; }")).toEqual([]);
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

// Ratchets of decision 0062 (docs/decisions/0062-closed-parts-owned-space-fitted-layers.md,
// sections 1 and 2, "Ratchets"): a part is closed, so each count below may only go down. Lower
// the ceiling in the change that lowers the count; the failure lists every offender by line.

/** Every class and id a selector names anywhere: subject, context and the arguments of
 * :is(), :where(), :has(), :not() and :nth-child(… of …). Strings and attribute values are skipped. */
function mentions(selector: string): { classes: string[]; ids: string[] } {
  const text = selector.replace(/"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/g, "\"\"").replace(/\[[^\]]*\]/g, "[]");
  const words = (sigil: string) => [...text.matchAll(new RegExp(`\\${sigil}(-?(?:[A-Za-z_]|\\\\.)(?:[\\w-]|\\\\.)*)`, "g"))]
    .map(([, word]) => word!.replace(/\\(.)/g, "$1"));
  return { classes: words("."), ids: words("#") };
}

/** A part is a ui/ file's name. It owns the classes `ui-<name>` and `<name>` in kebab case, and
 * these prefixes where its names follow no one rule. A class belongs to the part whose prefix it
 * equals or continues after a "-", the longest prefix winning (ui-status-mark is StatusMark's). */
const extraPrefixes: Record<string, string[]> = {
  ChoiceList: ["choice"], CitationDetails: ["citation"], CitationShelf: ["shelf", "preview-text"],
  CopiedNotice: ["notice-window"], CopyReceipt: ["copy-check-reveal"], Delights: ["ui-about", "ui-quiet-status"], Display: ["display", "csl"],
  EmptyState: ["empty", "opening", "opened-mark"], Footer: ["footer", "library-bar", "bar", "enter-hints", "more", "delivery-bar"],
  Foundation: ["ui-list-heading"], Frame: ["ui-line", "ui-notice-line", "ui-heading-slot", "ui-drawer-slot"],
  HeldKeys: ["ui-key-mark"], IbisMark: ["eyeshade"], Inspector: ["inspector"], KeyCaps: ["key-cap", "key-hint"], KeysSheet: ["keys"],
  Layout: ["ui-row", "ui-note", "ui-choice", "ui-section", "ui-group", "ui-details", "ui-buttons"], LibraryStatus: ["library-status", "library-name", "library-action-error", "library-inline-action", "library-announcement",
    "library-glyph", "library-words", "library-source", "library-state", "library-separator", "library-age", "status-dot"],
  Menu: ["menu"], NoticeSlot: ["notice-band"], Option: ["result", "tile-button", "type-icon"], PictureChoice: ["ui-picture", "ui-picker-picture"],
  PopupButton: ["popup-indicator"], Progress: ["update"], Reader: ["reader", "invented"], SearchBar: ["search", "pin-button", "query-input"],
  Segmented: ["ui-segment"], SettingsLayout: ["settings", "ui-settings-page"], ShortcutTable: ["ui-shortcut-filter", "ui-settings-keys", "keys-binding"],
  SideNavigation: ["ui-side"], SourceAction: ["source"], ViewFrame: ["ui-view"],
};
/** Components named for no part, and the part they draw; any other component is the part it is named for (features/reader/Reader.tsx is Reader). */
const drawsPart: Record<string, string> = { "src/ui/Details.tsx": "Layout", "src/ui/ShortcutSetting.tsx": "ShortcutTable", "src/ui/UndoNotice.tsx": "NoticeSlot",
  "src/features/picker/PickerFooter.tsx": "Footer", "src/LibraryStatusLine.tsx": "LibraryStatus" };
/** Classes any element may carry: states of the element itself, and the one utility (Foundation's visually-hidden). */
const anyPart = new Set(["showing", "leaving", "visually-hidden"]);
const kebab = (name: string) => name.replace(/(?<=[a-z0-9])(?=[A-Z])/g, "-").toLowerCase();
const partNames = [...new Set(list("src/ui/", /^[A-Z]\w*\.(css|tsx)$/).map((name) => name.slice("src/ui/".length).replace(/\.\w+$/, "")))]
  .filter((name) => !(`src/ui/${name}.tsx` in drawsPart));
const prefixes = new Map(partNames.map((part) => [part, [`ui-${kebab(part)}`, kebab(part), ...extraPrefixes[part] ?? []]]));
/** The parts a class belongs to: one, or none for a screen's or a library's class. */
function ownerOf(word: string): string[] {
  const stem = word.replace(/-+$/, "");
  let best = 0, owners: string[] = [];
  for (const [part, list] of prefixes) for (const prefix of list) {
    if (stem !== prefix && !stem.startsWith(`${prefix}-`)) continue;
    if (prefix.length > best) [best, owners] = [prefix.length, [part]];
    else if (prefix.length === best && !owners.includes(part)) owners.push(part);
  }
  return owners;
}
const partSheets = styled.filter(([name]) => /^src\/ui\/[^/]+\.css$/.test(name));
const appStyled = styled.filter(([name]) => name.startsWith("src/"));
const at = (name: string, line: number) => `${name.slice("src/".length)}:${line}`;
const ratchet = (what: string, offenders: string[]) => `${what} may only go down (decision 0062). Lower the ceiling when it falls. `
  + `${offenders.length} found:\n${offenders.join("\n")}`;

/** A ui/ selector that names a class not its sheet's own, or reaches the part from a :root, html or body context to set more than knobs. */
function crossPart(): string[] {
  return partSheets.flatMap(([name, selectors]) => {
    const part = name.slice("src/ui/".length, -".css".length);
    return selectors.flatMap(([selector, , line, body]) => {
      const foreign = [...new Set(mentions(selector).classes)].filter((word) => !anyPart.has(word) && !ownerOf(word).includes(part))
        .map((word) => `.${word} (${ownerOf(word).join(", ") || "no part"})`);
      if (/^(:root|html|body)\b[^\s>+~]*\s*[\s>+~]\s*\S/.test(selector) && /(^|;)\s*(?!--)[a-z-]+\s*:/.test(body)) foreign.push(`${selector.split(/[\s>+~]/)[0]} context sets more than custom properties`);
      return foreign.length ? [`${at(name, line)} › ${selector} › ${foreign.join(", ")}`] : [];
    });
  });
}

const CROSS_PART_CEILING = 177;
test(`ui/ sheets select only their own part: ${CROSS_PART_CEILING} selectors at most reach another`, () => {
  const found = crossPart();
  expect(found.length, ratchet("Selectors in a ui/ sheet that name another part's class (style it through that part's knobs or props)", found))
    .toBeLessThanOrEqual(CROSS_PART_CEILING);
});

const ID_SELECTOR_CEILING = 23;
test(`ID selectors in the app's stylesheets stay at or under ${ID_SELECTOR_CEILING}`, () => {
  const found = appStyled.flatMap(([name, selectors]) => selectors.filter(([selector]) => mentions(selector).ids.length).map(([selector, , line]) => `${at(name, line)} › ${selector}`));
  expect(found.length, ratchet("ID selectors (a part is reached through its knobs, never by id)", found)).toBeLessThanOrEqual(ID_SELECTOR_CEILING);
});

const BUTTON_ELSEWHERE_CEILING = 40;
test(`.ui-button is selected outside ui/Button.css at most ${BUTTON_ELSEWHERE_CEILING} times`, () => {
  const found = appStyled.filter(([name]) => name !== "src/ui/Button.css").flatMap(([name, selectors]) => selectors
    .filter(([selector]) => mentions(selector).classes.includes("ui-button")).map(([selector, , line]) => `${at(name, line)} › ${selector}`));
  expect(found.length, ratchet("Selectors naming .ui-button outside ui/Button.css (Button owns every state fill; use its variants or knobs)", found))
    .toBeLessThanOrEqual(BUTTON_ELSEWHERE_CEILING);
});

/** Each className-style attribute or property and the class words in its string and template literals, with its line. */
function renderedClasses(source: string): { line: number; words: string[] }[] {
  const text = source.replace(/\/\*[^]*?\*\/|(^|\s)\/\/[^\n]*/g, (comment) => comment.replace(/[^\n]/g, " "));
  const literals = (code: string): string[] => [...code.matchAll(/"((?:\\.|[^"\\])*)"|'((?:\\.|[^'\\])*)'|`((?:\\.|[^`\\])*)`/g)]
    .map(([, double, single, template]) => template === undefined ? double ?? single ?? ""
      : template.replace(/\$\{([^{}]*)\}/g, (_, inner: string) => ` ${literals(inner).join(" ")} `));
  const found: { line: number; words: string[] }[] = [];
  for (const match of text.matchAll(/\b\w*(?:className|ClassName)\s*\??([=:])(?!=)\s*/g)) {
    let end = match.index! + match[0].length;
    const first = text[end];
    if (first === "{") {
      for (let depth = 1, index = end + 1; index < text.length; index++) {
        if (text[index] === "{") depth++;
        else if (text[index] === "}" && !--depth) { end = index + 1; break; }
      }
    } else if (first === "\"" || first === "'" || first === "`") end = text.indexOf(first, end + 1) + 1;
    else if (match[1] === ":") end = text.slice(end).search(/[,;}\n]/) + end;
    else continue;
    const words = literals(text.slice(match.index! + match[0].length, end)).flatMap((value) => value.split(/\s+/))
      .filter((word) => /^-?[A-Za-z_][\w-]*$/.test(word));
    if (words.length) found.push({ line: text.slice(0, match.index!).split("\n").length, words });
  }
  return found;
}
const componentFiles = [...new Set([...list("src/ui/", /^[A-Z]\w*\.tsx$/), ...list("src/features/", /\.tsx$/).filter((name) => !/\.(test|fixture)\.tsx$/.test(name)),
  ...Object.keys(drawsPart)])];
const partOf = (name: string) => drawsPart[name] ?? name.slice(name.lastIndexOf("/") + 1, -".tsx".length);

/** "file:line › class (part)" for each className a ui/ component or a feature renders from another part. */
function borrowed(): string[] {
  return componentFiles.flatMap((name) => {
    const part = partOf(name);
    return renderedClasses(read(name)).flatMap(({ line, words }) => {
      const foreign = [...new Set(words)].filter((word) => !anyPart.has(word) && ownerOf(word).length && !ownerOf(word).includes(part))
        .map((word) => `${word} (${ownerOf(word).join(", ")})`);
      return foreign.length ? [`${at(name, line)} › ${foreign.join(", ")}`] : [];
    });
  });
}

const BORROWED_CLASS_CEILING = 72;
test(`components render another part's class names at most ${BORROWED_CLASS_CEILING} times`, () => {
  const found = borrowed();
  expect(found.length, ratchet("className attributes in ui/ and features/ that render another part's classes (compose that part instead)", found))
    .toBeLessThanOrEqual(BORROWED_CLASS_CEILING);
});

// Chromium leaves ::before and ::after unpainted on rows that mount after the sheet when a
// selector filters with :nth-child(… of …); count with a class on the row instead.
const nthOf = (selector: string) => /:nth-(?:last-)?child\([^()]*\bof\b/i.test(selector);
test("no stylesheet filters :nth-child() or :nth-last-child() with `of`", () => {
  expect(styled.flatMap(([name, selectors]) => selectors.filter(([selector]) => nthOf(selector)).map(([selector, , line]) => `${name}:${line} › ${selector}`))).toEqual([]);
  expect([".a:nth-child(2 of .x)", "li:nth-last-child(n + 2 of :not([hidden]))", "tr:nth-child(n + 3) > *", ".a:nth-of-type(2)"].map(nthOf)).toEqual([true, true, false, false]);
});

test("the ratchets read every place a class or id is named, and each part's map", () => {
  expect(mentions(".a:is(.b, #c .d):where(.e):not(.f):has(> .g) [data-x=\".h #i\"] li:nth-child(2 of .j)"))
    .toEqual({ classes: ["a", "b", "d", "e", "f", "g", "j"], ids: ["c"] });
  expect([ownerOf("ui-status-mark"), ownerOf("ui-drawer-slot"), ownerOf("citation-token-handle"), ownerOf("citation-locator"),
    ownerOf("toolbar-menu-control"), ownerOf("menu-item"), ownerOf("ui-palette-row"), ownerOf("ui-buttons"), ownerOf("result-"), ownerOf("picker-content")])
    .toEqual([["StatusMark"], ["Frame"], ["CitationToken"], ["CitationDetails"], ["ToolbarMenu"], ["Menu"], ["Palette"], ["Layout"], ["Option"], []]);
  expect(renderedClasses("<a className=\"ui-row result-tile\" /><b className={`x ${on ? \"tile-button\" : \"\"} result-${kind}`} />\n<i className={cx('menu-item')} />\n// className=\"shelf\"\nconst p = { className: \"ui-band\" }; type T = { className?: string };"))
    .toEqual([{ line: 1, words: ["ui-row", "result-tile"] }, { line: 1, words: ["x", "tile-button", "result-"] }, { line: 2, words: ["menu-item"] }, { line: 4, words: ["ui-band"] }]);
  // Every extra prefix names something in its part's own sheet or component, and no class has two owners.
  const named = new Map(partNames.map((part) => [part, new Set([...partSheets.filter(([name]) => name === `src/ui/${part}.css`)
    .flatMap(([, selectors]) => selectors.flatMap(([selector]) => mentions(selector).classes)),
    ...componentFiles.filter((name) => partOf(name) === part).flatMap((name) => renderedClasses(read(name)).flatMap(({ words }) => words))])]));
  expect(Object.entries(extraPrefixes).flatMap(([part, list]) => list.filter((prefix) => ![...named.get(part) ?? []].some((word) => ownerOf(word).includes(part) && (word === prefix || word.startsWith(`${prefix}-`))))
    .map((prefix) => `${part}: ${prefix}`))).toEqual([]);
  expect([...new Set([...named.values()].flatMap((words) => [...words]))].filter((word) => ownerOf(word).length > 1)).toEqual([]);
});
