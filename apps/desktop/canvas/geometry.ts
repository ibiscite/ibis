// The layout rules of docs/design.md as data, checked on whatever screen is
// mounted. One table, read by the Mac's WebKit gallery (gallery/webkit.swift)
// and the Chromium canvas renderer (gallery/canvas.mjs), so a rule written
// once is held on every screen, in light, dark, larger text and right to left.
// Each rule names its tag; a violation says the tag, the part and the distance.

/** [C3] Parts whose mark centres on the tile column, and parts whose words
 * start on the title column, measured from the window's leading edge. Choice
 * screens centre their own group and the inspector is its own column, so
 * neither is held to the window's columns. */
export const columns = {
  marks: [".result-tile", ".ui-palette-row > .ui-band-icon", ".search-bar .search-mark", ".library-bar .status-dot", ".ui-notice-line .update-ring", ".ui-view-back .ui-icon"],
  words: [".result-title", ".search-bar input", ".ui-view-field", ".library-bar .library-words", ".ui-notice-line .notice-band > span:first-child", ".ui-notice-line .update-text", ".ui-notice-line .ui-line-words", ".shelf-citation", ".ui-view-title", ".ui-palette-label", ".ui-view-status", ".ui-list-heading > :is(span:not(.ui-band-icon), h2)"],
  outside: ".choice-screen, .inspector, .menu-popover, [inert], .ui-notice-line > :is(.notice-slot, .ui-line-notice) ~ .ui-line-notice",
  /** Hover plates on the window's leading edge: each starts on the window inset, as a row does. */
  plates: ["#results [role=\"option\"]", ".library-bar .library-status-trigger"],
} as const;

/** [C21] Pop-ups that fill a form row: the menu sits under the button's leading
 * edge and is never narrower than it. */
export const anchored = { popover: '.toolbar-menu-popover[data-anchor="field"]', trigger: '.citation-field .ui-popup-button[aria-expanded="true"]' } as const;

/** [C8] The leading footer selector is one of the footer's small controls;
 * its menu opens above it with the same leading edge and a clear gap. */
export const footerSelector = {
  trigger: ".library-bar .library-status-trigger",
  controls: ".library-bar .enter-hints .ui-button, .library-bar .more .more-button",
  popover: ".menu-popover:has(#library-menu)",
} as const;

/** [C9] Action menus fit their labels until the window limits their width,
 * scrolling or not: a scroll bar never takes its width from a label. Pop-up
 * menus of choices deliberately keep one narrow column. */
export const menuLabels = { label: ".menu-popover .menu-label", outside: ".toolbar-menu-popover" } as const;

/** [C19] Rows of one form share both edges: every control's drawn fill. */
export const forms = {
  form: ".citation-details",
  // Page's type and text share the locator's fill. Measure that whole control,
  // just as Show as measures its whole pop-up.
  controls: ".citation-field > .toolbar-menu-control .ui-popup-button, .citation-field > .citation-locator, .citation-field > input",
} as const;

/** [C19] A pane divider runs the whole region it splits, as a Mac split view's:
 * from the search bar's divider to the shelf. */
export const panes = { pane: ".inspector:not([data-reading])", top: ".search-bar", bottom: ".shelf" } as const;

/** Stacked parts never cover each other: each starts below the one before. */
export const stacks = [".inspector-titles", ".inspector-column", ".citation-details", ".peek-card section"] as const;

/** [C12] A grid of choice cards: one height, and each glyph level with its title's first line. */
export const cards = { grid: '.choice-screen[data-grid] .choice-list', card: ".choice-row", glyph: ".choice-tile", title: ".result-title" } as const;

/** Every interface glyph has one of two fixed pixel sizes and the same ink weight. */
export const glyphs = { selector: "svg.ui-icon", sizes: [16, 20], stroke: 1.5 } as const;

/** [D8] A ring hugs the control it names: a sheet, a menu or anything else only
 * the app focuses (tabindex -1) never draws one around itself. */
export const rings = { containers: '[role="dialog"], [role="menu"], [tabindex="-1"]' } as const;

/** [C8] A status is never cut to a stub ("Z…"): a line whose words end in "…"
 * keeps at least --status-short of them, and the footer folds its hint labels
 * before that. A part that drops whole (an update's phase) is not cut. */
export const stubs = { line: ".library-bar .library-words, .update-text", words: ".library-source, .library-state, .library-age, .update-text > *" } as const;

const tolerance = 1;

/** Every rule broken on the mounted screen, as "[tag] part: detail". Empty when all hold. */
/** Record identities are the engine's, never people's words. Every fixture's
 * record ID carries this sentinel and differs from its citation key, so an ID
 * shown in place of a key, a title or a name is seen wherever it appears. */
export const identities = { sentinel: "zz-record-id-", said: "[title], [aria-label], [aria-description], [placeholder]" } as const;

export function geometry(root: ParentNode = document): string[] {
  const failures: string[] = [];
  const rtl = getComputedStyle(document.documentElement).direction === "rtl";
  const width = document.documentElement.clientWidth;
  // Distance from the window's leading edge, in either direction.
  const lead = (rect: DOMRect, end = false) => rtl ? width - (end ? rect.left : rect.right) : end ? rect.right : rect.left;
  const visible = (node: Element) => { const rect = node.getBoundingClientRect(); return rect.width > 0 && rect.height > 0 && getComputedStyle(node).visibility !== "hidden"; };
  const name = (node: Element) => `${node.tagName.toLowerCase()}.${[...node.classList].join(".")}`;

  for (const glyph of root.querySelectorAll<SVGSVGElement>(glyphs.selector)) {
    // A pressed control gives a little (--press-scale): a transform of its drawn size, not its size.
    if (!visible(glyph) || glyph.closest("[data-pressed]")) continue;
    const rect = glyph.getBoundingClientRect();
    if (!glyphs.sizes.some((size) => Math.abs(rect.width - size) < .02 && Math.abs(rect.height - size) < .02))
      failures.push(`[icons] ${name(glyph)}: ${rect.width.toFixed(2)} × ${rect.height.toFixed(2)}px, expected 16 or 20px square`);
    for (const path of glyph.querySelectorAll("path")) {
      const matrix = path.getScreenCTM();
      if (!matrix) continue;
      const style = getComputedStyle(path), stroke = parseFloat(style.strokeWidth);
      const scales = style.vectorEffect === "non-scaling-stroke" ? [1] : [Math.hypot(matrix.a, matrix.b), Math.hypot(matrix.c, matrix.d)];
      if (scales.some((scale) => !Number.isFinite(stroke) || Math.abs(stroke * scale - glyphs.stroke) >= .02))
        failures.push(`[icons] ${name(glyph)}: stroke ${(stroke * scales[0]!).toFixed(2)}px, expected 1.5px`);
    }
  }

  // The existing More, Format and shortcuts scenarios cover the shared Back
  // anatomy in both palettes and sheets, including coarse-pointer canvases.
  for (const back of root.querySelectorAll(".ui-view-back")) {
    if (!visible(back)) continue;
    const rect = back.getBoundingClientRect();
    const scale = (parseFloat(getComputedStyle(document.documentElement).fontSize) || 16) / 16;
    const minimum = (matchMedia("(pointer: coarse)").matches ? 44 : 32) * scale;
    if (Math.abs(rect.width - minimum) > tolerance || Math.abs(rect.height - minimum) > tolerance)
      failures.push(`[Back] target ${rect.width.toFixed(1)} × ${rect.height.toFixed(1)}px, expected ${minimum}px square`);
    const field = back.closest(".ui-view-head")?.querySelector(".ui-view-field");
    if (field && visible(field)) {
      const search = field.getBoundingClientRect(), delta = rect.top + rect.height / 2 - (search.top + search.height / 2);
      if (Math.abs(delta) > tolerance) failures.push(`[Back] centre ${delta.toFixed(1)}px off the search field vertically`);
    }
  }

  // The columns come from the tokens, never from another part that could be wrong too.
  const main = root.querySelector("main");
  if (main) {
    const probe = document.createElement("span");
    probe.style.cssText = "position:absolute;inset-block-start:0;inset-inline-start:calc(var(--inset) + var(--row-pad));inline-size:var(--tile);block-size:1px;visibility:hidden";
    main.append(probe);
    const tile = probe.getBoundingClientRect();
    const title = probe.cloneNode() as HTMLElement;
    title.style.insetInlineStart = "calc(var(--inset) + var(--row-pad) + var(--title-start))";
    main.append(title);
    const plate = probe.cloneNode() as HTMLElement;
    plate.style.insetInlineStart = "var(--inset)";
    main.append(plate);
    const tileCentre = lead(tile) + tile.width / 2, titleStart = lead(title.getBoundingClientRect()), plateStart = lead(plate.getBoundingClientRect());
    probe.remove(); title.remove(); plate.remove();
    const held = (selector: string) => [...root.querySelectorAll(selector)].filter((node) => visible(node) && !node.closest(columns.outside));
    for (const node of held(columns.marks.join(", "))) {
      const rect = node.getBoundingClientRect(), centre = lead(rect) + rect.width / 2;
      if (Math.abs(centre - tileCentre) > tolerance) failures.push(`[C3] ${name(node)}: centre ${centre.toFixed(1)}, tile column ${tileCentre.toFixed(1)}`);
    }
    for (const node of held(columns.plates.join(", "))) {
      const start = lead(node.getBoundingClientRect());
      if (Math.abs(start - plateStart) > tolerance) failures.push(`[C3] ${name(node)}: plate starts ${start.toFixed(1)}, window inset ${plateStart.toFixed(1)}`);
    }
    for (const node of held(columns.words.join(", "))) {
      const rect = node.getBoundingClientRect(), style = getComputedStyle(node);
      // Field and title words start inside their padding and border.
      const inset = node.matches("input, .ui-view-title") ? parseFloat(rtl ? style.paddingRight : style.paddingLeft) + parseFloat(rtl ? style.borderRightWidth : style.borderLeftWidth) : 0;
      const start = lead(rect) + inset;
      if (Math.abs(start - titleStart) > tolerance) failures.push(`[C3] ${name(node)}: starts ${start.toFixed(1)}, title column ${titleStart.toFixed(1)}`);
    }
  }

  const selector = root.querySelector(footerSelector.trigger);
  if (selector && visible(selector)) {
    const button = selector.getBoundingClientRect();
    for (const control of root.querySelectorAll(footerSelector.controls)) {
      if (!visible(control)) continue;
      const rect = control.getBoundingClientRect();
      if (Math.abs(button.height - rect.height) > tolerance)
        failures.push(`[C8] library selector ${button.height.toFixed(1)}px tall, footer control ${name(control)} ${rect.height.toFixed(1)}px`);
      const delta = button.top + button.height / 2 - (rect.top + rect.height / 2);
      if (Math.abs(delta) > tolerance) failures.push(`[C8] library selector centre ${delta.toFixed(1)}px off footer control ${name(control)}`);
    }
    const popover = root.querySelector(footerSelector.popover);
    if (popover && visible(popover)) {
      const menu = popover.getBoundingClientRect();
      if (Math.abs(lead(menu) - lead(button)) > tolerance)
        failures.push(`[C8] library menu starts ${lead(menu).toFixed(1)}, selector ${lead(button).toFixed(1)}`);
      if (menu.bottom > button.top - tolerance)
        failures.push(`[C8] library menu ends ${menu.bottom.toFixed(1)}, must leave a gap above selector ${button.top.toFixed(1)}`);
    }
  }

  const popover = root.querySelector(anchored.popover), trigger = root.querySelector(anchored.trigger);
  if (popover && trigger) {
    const menu = popover.getBoundingClientRect(), button = trigger.getBoundingClientRect();
    // A menu wider than the room left moves back only as far as the window's inset needs [D15].
    const pushed = width - lead(menu, true) <= 8 + tolerance && lead(menu) < lead(button);
    if (Math.abs(lead(menu) - lead(button)) > tolerance && !pushed) failures.push(`[C21] ${name(popover)}: starts ${lead(menu).toFixed(1)}, its button ${lead(button).toFixed(1)}`);
    if (menu.width + tolerance < button.width) failures.push(`[C21] ${name(popover)}: ${menu.width.toFixed(1)} wide, narrower than its button (${button.width.toFixed(1)})`);
  }

  for (const label of root.querySelectorAll<HTMLElement>(menuLabels.label)) {
    if (!visible(label) || label.closest(menuLabels.outside)) continue;
    const menu = label.closest(".menu-popover")!;
    const menuWidth = menu.getBoundingClientRect().width, maximum = parseFloat(getComputedStyle(menu).maxWidth);
    if (menuWidth + tolerance < maximum && label.scrollWidth > label.clientWidth)
      failures.push(`[C9] ${name(label)}: cut by ${label.scrollWidth - label.clientWidth}px while its menu is ${menuWidth.toFixed(1)} wide, below its maximum (${maximum.toFixed(1)})`);
  }

  for (const form of root.querySelectorAll(forms.form)) {
    const fills = [...form.querySelectorAll(forms.controls)].filter(visible).map((node) => {
      const rect = node.getBoundingClientRect(), style = getComputedStyle(node);
      // A field's clear border is target, not fill (CitationDetails.css).
      const border = node.matches("input") ? parseFloat(style.borderLeftWidth) : 0;
      return { node, start: lead(rect) + border, end: lead(rect, true) - border };
    });
    for (const fill of fills.slice(1)) for (const edge of ["start", "end"] as const)
      if (Math.abs(fill[edge] - fills[0]![edge]) > tolerance) failures.push(`[C19] ${name(fill.node)}: ${edge} ${fill[edge].toFixed(1)}, the form's ${fills[0]![edge].toFixed(1)}`);
  }

  const pane = root.querySelector(panes.pane), top = root.querySelector(panes.top), bottom = root.querySelector(panes.bottom);
  if (pane && top && visible(pane)) {
    const rect = pane.getBoundingClientRect(), header = top.getBoundingClientRect().bottom;
    if (Math.abs(rect.top - header) > tolerance) failures.push(`[C19] ${name(pane)}: divider starts ${rect.top.toFixed(1)}, the search bar ends ${header.toFixed(1)}`);
    const floor = bottom && visible(bottom) ? bottom.getBoundingClientRect().top : null;
    if (floor !== null && Math.abs(rect.bottom - floor) > tolerance) failures.push(`[C19] ${name(pane)}: divider ends ${rect.bottom.toFixed(1)}, the shelf starts ${floor.toFixed(1)}`);
  }

  // Nothing scrolls sideways: a sub-pixel overhang draws a whole scroll bar in Chromium.
  for (const node of root.querySelectorAll<HTMLElement>("*")) {
    const overflowX = getComputedStyle(node).overflowX;
    if ((overflowX === "auto" || overflowX === "scroll") && visible(node) && node.scrollWidth > node.clientWidth && !node.closest(".page-strip, .reader-pages"))
      failures.push(`[scroll] ${name(node)} scrolls sideways by ${node.scrollWidth - node.clientWidth}px`);
  }

  for (const stack of root.querySelectorAll(stacks.join(", "))) {
    const parts = [...stack.children].filter((node) => visible(node) && getComputedStyle(node).position !== "absolute");
    for (let i = 1; i < parts.length; i++) {
      const above = parts[i - 1]!.getBoundingClientRect(), below = parts[i]!.getBoundingClientRect();
      if (below.top < above.bottom - tolerance && below.left < above.right && below.right > above.left)
        failures.push(`[stack] ${name(parts[i]!)} covers ${name(parts[i - 1]!)} by ${(above.bottom - below.top).toFixed(1)}`);
    }
  }

  for (const grid of root.querySelectorAll(cards.grid)) {
    const shapes = [...grid.querySelectorAll(cards.card)].map((card) => {
      const box = card.getBoundingClientRect(), glyph = card.querySelector(cards.glyph)?.getBoundingClientRect(), heading = card.querySelector(cards.title);
      // Level with the title's first line: a title that wraps keeps its glyph beside its start.
      const title = heading?.getBoundingClientRect(), line = heading ? Math.min(title!.height, parseFloat(getComputedStyle(heading).lineHeight) || title!.height) : 0;
      return { card, height: box.height, glyph: glyph && title ? (glyph.top + glyph.height / 2) - (title.top + line / 2) : 0, top: glyph ? glyph.top - box.top : 0 };
    });
    for (const shape of shapes.slice(1)) {
      if (Math.abs(shape.height - shapes[0]!.height) > tolerance) failures.push(`[C12] ${name(shape.card)}: ${shape.height.toFixed(1)} tall, the first card ${shapes[0]!.height.toFixed(1)}`);
      if (Math.abs(shape.top - shapes[0]!.top) > tolerance) failures.push(`[C12] ${name(shape.card)}: glyph ${shape.top.toFixed(1)} from the top, the first card ${shapes[0]!.top.toFixed(1)}`);
    }
    for (const shape of shapes) if (Math.abs(shape.glyph) > tolerance) failures.push(`[C12] ${name(shape.card)}: glyph ${shape.glyph.toFixed(1)} off its title's line`);
  }
  // [C24] No empty band: the frame's heading line is drawn only with something in
  // it, and nothing sits between it and the list's first row. A list with no heading
  // line (the commands) keeps its own group headings.
  const line = root.querySelector(".ui-notice-line"), bar = root.querySelector(".search-bar"), first = root.querySelector('#results [role="option"]');
  if (line && visible(line) && !line.textContent?.trim()) failures.push("[C24 frame] the heading line is drawn empty");
  const area = first?.closest(".results-area");
  if (first && area && bar && line && visible(bar) && visible(line) && visible(first)) {
    const above = line.getBoundingClientRect().bottom;
    const gap = first.getBoundingClientRect().top + area.scrollTop + (area.parentElement?.scrollTop ?? 0) - above - parseFloat(getComputedStyle(area).marginTop);
    if (Math.abs(gap) > tolerance) failures.push(`[C24 frame] ${gap.toFixed(1)}px between the heading line and the first row`);
  }
  const focused = document.activeElement;
  if (focused instanceof HTMLElement && focused.matches(rings.containers)) {
    const style = getComputedStyle(focused);
    if (style.outlineStyle !== "none" && parseFloat(style.outlineWidth) > 0) failures.push(`[D8] ${name(focused)}: draws a focus ring around itself`);
  }
  const short = (() => { const probe = document.createElement("span"); probe.style.cssText = "position:absolute;visibility:hidden;inline-size:var(--status-short)"; document.body.append(probe); const px = probe.getBoundingClientRect().width; probe.remove(); return px; })();
  for (const line of root.querySelectorAll<HTMLElement>(stubs.line)) {
    if (!visible(line) || line.closest(".visually-hidden")) continue;
    // The first line only: a part that wrapped onto the hidden second line dropped whole.
    const box = line.getBoundingClientRect(), lineHeight = parseFloat(getComputedStyle(line).lineHeight);
    const bottom = Number.isFinite(lineHeight) ? box.top + lineHeight : box.bottom;
    const cut = [...line.querySelectorAll<HTMLElement>(stubs.words)].some((words) => visible(words) && words.getBoundingClientRect().top < bottom - tolerance && words.scrollWidth > words.clientWidth + tolerance);
    if (cut && line.clientWidth + tolerance < short) failures.push(`[C8] ${name(line)}: cut to ${line.clientWidth}px, under --status-short (${short.toFixed(0)}px)`);
  }
  const body = root instanceof Document ? root.body : root instanceof HTMLElement ? root : null;
  const said = [body?.innerText ?? "", ...[...root.querySelectorAll(identities.said)].flatMap((node) => ["title", "aria-label", "aria-description", "placeholder"].map((attribute) => node.getAttribute(attribute) ?? ""))];
  const leak = said.find((text) => text.includes(identities.sentinel));
  if (leak !== undefined) failures.push(`[ids] a record ID reached people's words: ${leak.slice(Math.max(0, leak.indexOf(identities.sentinel) - 40), leak.indexOf(identities.sentinel) + 30)}`);
  return failures;
}

// ---------------------------------------------------------------------------
// [C24] Guards across states. The rules above hold on one picture; these hold
// between pictures and over every control, so a part that moves, covers, cuts
// its words or takes the wrong layer fails wherever it appears. canvas/audit.ts
// drives them through the scenarios.

type Box = { x: number; y: number; width: number; height: number };
const boxOf = (node: Element): Box => { const rect = node.getBoundingClientRect(); return { x: rect.left, y: rect.top, width: rect.width, height: rect.height }; };
const label = (node: Element) => node.id ? `#${node.id}` : `${node.tagName.toLowerCase()}${[...node.classList].slice(0, 2).map((part) => `.${part}`).join("")}${node.getAttribute("aria-label") ? `[${node.getAttribute("aria-label")}]` : ""}`;

/** Wait until the page has stopped changing, then end every running animation
 * and transition, so a measurement never catches a part mid-motion: an
 * offscreen web view may never paint the frames a transition waits for. */
export async function settle(quiet = 250, longest = 4000): Promise<void> {
  const start = performance.now();
  let last = performance.now();
  const observer = new MutationObserver(() => { last = performance.now(); });
  observer.observe(document.documentElement, { childList: true, subtree: true, attributes: true, characterData: true });
  try {
    while (performance.now() - last < quiet && performance.now() - start < longest) await new Promise((done) => setTimeout(done, 50));
  } finally { observer.disconnect(); }
  for (const animation of document.getAnimations()) {
    // A looping animation (a spinner) has no end; it keeps running.
    try { if (animation.effect?.getComputedTiming().endTime !== Infinity) animation.finish(); } catch { /* already gone */ }
  }
}

/** [C24] The frame's anchors, where a person's eye and pointer rest, and the
 * edges of each that hold in every state of the picker. A field's end follows
 * the controls after it and an action's start follows its words, so neither is
 * held; their other edges are. */
export const anchors = {
  search: { selector: "#search", edges: ["start", "top", "height"] },
  "first row": { selector: '#results [role="option"]', edges: ["start", "end", "top", "height"] },
  "footer action": { selector: ".library-bar .footer-action", edges: ["end", "top", "height"] },
  shelf: { selector: "main > .shelf", edges: ["start", "end", "top", "height"] },
} as const;
type Anchor = keyof typeof anchors;
type Edge = "start" | "end" | "top" | "height";

/** Picker states compared with the idle picture ("results"), each a canvas
 * scenario: loading, failures, a refusal, notices, the shelf, details and the
 * floating parts. Hover, press, focus and selection are checked by paintOnly. */
export const frameStates = ["unknown-app", "loading", "error", "project-check-conflict", "undo-notice", "update", "stale", "peek", "shelf", "reference-details", "more", "format-menu", "library-menu"] as const;

/** Moves an anchor may make, each with its reason. Keep this short: a line here
 * is a design decision, not a fix. */
export const declaredMoves: { anchor: Anchor; scenario: string; edge: Edge; reason: string }[] = [];

/** Where each anchor's edges are in this picture, from the window's leading edge. */
export function anchorBoxes(root: ParentNode = document): Partial<Record<Anchor, Record<Edge, number>>> {
  const found: Partial<Record<Anchor, Record<Edge, number>>> = {};
  const rtl = getComputedStyle(document.documentElement).direction === "rtl", width = document.documentElement.clientWidth;
  for (const [part, { selector }] of Object.entries(anchors) as [Anchor, typeof anchors[Anchor]][]) {
    const node = root.querySelector(selector);
    if (!node || node.closest("[hidden]") || getComputedStyle(node).visibility === "hidden") continue;
    const rect = node.getBoundingClientRect();
    if (rect.height > 0) found[part] = { start: rtl ? width - rect.right : rect.left, end: rtl ? width - rect.left : rect.right, top: rect.top, height: rect.height };
  }
  return found;
}

/** Anchors that moved between the idle picture and another state. An anchor
 * absent from either is no move: that state draws no such part. */
export function anchorMoves(idle: ReturnType<typeof anchorBoxes>, other: ReturnType<typeof anchorBoxes>, scenario: string): string[] {
  const failures: string[] = [];
  for (const [part, { edges }] of Object.entries(anchors) as [Anchor, typeof anchors[Anchor]][]) {
    const before = idle[part], after = other[part];
    if (!before || !after) continue;
    for (const edge of edges) {
      const delta = after[edge] - before[edge];
      if (Math.abs(delta) <= tolerance || declaredMoves.some((move) => move.anchor === part && move.scenario === scenario && move.edge === edge)) continue;
      failures.push(`[C24 frame] ${part} ${edge} ${delta > 0 ? "+" : ""}${delta.toFixed(1)}px in ${scenario}`);
    }
  }
  return failures;
}

/** Everything a person can point at or tab to. */
const controls = 'button, input:not([type="hidden"]), select, textarea, a[href], [role="option"], [role="menuitem"], [role="menuitemradio"], [role="menuitemcheckbox"], [role="switch"], [role="tab"], [role="checkbox"], [role="radio"], [role="slider"], [tabindex]:not([tabindex="-1"])';

/** Whether a control is drawn and live: not hidden, inert, disabled or a 1 px label for screen readers. */
function live(node: Element): boolean {
  if (node.closest('[inert], [aria-hidden="true"], [hidden], .visually-hidden')) return false;
  if (node.matches(':disabled, [aria-disabled="true"]')) return false;
  const rect = node.getBoundingClientRect();
  if (rect.width < 2 || rect.height < 2) return false;
  for (let part: Element | null = node; part; part = part.parentElement) {
    const style = getComputedStyle(part);
    if (style.visibility === "hidden" || style.display === "none" || Number(style.opacity) === 0) return false;
  }
  return getComputedStyle(node).pointerEvents !== "none";
}

/** The part of a node a person can see: clipped by the window and by every clipping ancestor. */
function seen(node: Element): DOMRect | null {
  let { left, top, right, bottom } = node.getBoundingClientRect();
  left = Math.max(left, 0); top = Math.max(top, 0);
  right = Math.min(right, document.documentElement.clientWidth || innerWidth); bottom = Math.min(bottom, document.documentElement.clientHeight || innerHeight);
  for (let part = node.parentElement; part && part !== document.body; part = part.parentElement) {
    const style = getComputedStyle(part);
    if (style.overflowX === "visible" && style.overflowY === "visible") continue;
    const clip = part.getBoundingClientRect();
    left = Math.max(left, clip.left); top = Math.max(top, clip.top); right = Math.min(right, clip.right); bottom = Math.min(bottom, clip.bottom);
  }
  return right - left >= 4 && bottom - top >= 4 ? new DOMRect(left, top, right - left, bottom - top) : null;
}

/** Layers that cover live controls on purpose, each with its reason. */
export const coverings: { selector: string; reason: string }[] = [
  { selector: ".peek-card, .ui-popover", reason: "An overlay anchored to its row or token, opened on purpose, covers the rows beside it until any key, a click outside or Esc closes it." },
];

/** [C24 cover] The centre of every visible, enabled control hits that control:
 * nothing drawn over it takes the click it shows. */
export function occlusion(root: ParentNode = document): string[] {
  const failures: string[] = [];
  for (const node of root.querySelectorAll(controls)) {
    if (!live(node)) continue;
    const rect = seen(node);
    if (!rect) continue;
    const hit = document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
    if (!hit || hit === node || node.contains(hit)) continue;
    if (coverings.some((covering) => hit.closest(covering.selector))) continue;
    failures.push(`[C24 cover] ${label(node)} is covered by ${label(hit)}`);
  }
  return failures;
}

/** Words on screen, each with the part of it a person can see. */
function wordBoxes(root: ParentNode): { node: Node; box: DOMRect }[] {
  const found: { node: Node; box: DOMRect }[] = [];
  const walker = document.createTreeWalker(root instanceof Document ? root.body : root as Node, NodeFilter.SHOW_TEXT);
  for (let text = walker.nextNode(); text; text = walker.nextNode()) {
    const parent = text.parentElement;
    if (!text.textContent?.trim() || !parent || parent.closest(".visually-hidden")) continue;
    const style = getComputedStyle(parent);
    if (style.visibility === "hidden" || Number(style.opacity) === 0) continue;
    const range = document.createRange();
    range.selectNodeContents(text);
    const clip = seen(parent);
    if (!clip) continue;
    for (const rect of range.getClientRects()) {
      const left = Math.max(rect.left, clip.left), top = Math.max(rect.top, clip.top), right = Math.min(rect.right, clip.right), bottom = Math.min(rect.bottom, clip.bottom);
      if (right - left >= 1 && bottom - top >= 1) found.push({ node: text, box: new DOMRect(left, top, right - left, bottom - top) });
    }
  }
  return found;
}

const overlaps = (a: DOMRect, b: DOMRect) => Math.min(a.right, b.right) - Math.max(a.left, b.left) > 1 && Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 1;

/** [C24 held] A key shown while ⌘ (Ctrl) is held (ui/HeldKeys' KeyMark) stays in
 * the window and covers no words and no control but the slot it stands in for.
 * Marks take no pointer, so occlusion() cannot see them; this is their cover check. */
export function heldKeys(root: ParentNode = document): string[] {
  const failures: string[] = [];
  const words = wordBoxes(root);
  const shown = [...root.querySelectorAll(controls)].filter(live).map((node) => ({ node, box: seen(node) })).filter((part): part is { node: Element; box: DOMRect } => !!part.box);
  const marks = [...root.querySelectorAll<HTMLElement>(".ui-key-mark")].map((mark) => ({ mark, box: (mark.firstElementChild ?? mark).getBoundingClientRect() })).filter(({ box }) => box.width > 0);
  for (const { mark, box } of marks) {
    const slot = mark.parentElement!, name = `"${mark.textContent}"`;
    if (box.left < 0 || box.top < 0 || box.right > innerWidth || box.bottom > innerHeight) failures.push(`[C24 held] ${name} leaves the window`);
    for (const { node, box: control } of shown)
      if (!slot.contains(node) && !node.contains(slot) && overlaps(box, control)) failures.push(`[C24 held] ${name} covers ${label(node)}`);
    for (const { node, box: text } of words)
      if (!slot.contains(node) && overlaps(box, text)) failures.push(`[C24 held] ${name} covers "${node.textContent!.trim().slice(0, 30)}"`);
    for (const other of marks)
      if (other.mark !== mark && overlaps(box, other.box)) failures.push(`[C24 held] ${name} covers the key "${other.mark.textContent}"`);
  }
  return [...new Set(failures)];
}

/** The states a control draws: React Aria's attributes, set as the canvas States board sets them. */
export const paintStates = ["data-hovered", "data-pressed", "data-focus-visible", "data-active", "aria-expanded"] as const;

/** [C24 states] Hover, press, focus, selection and an open menu change paint,
 * never size: no control or anchor moves or resizes while any control takes any
 * of these states. A press's small give (--press-scale) is a transform, not
 * layout, so scale is held still while measuring. */
export function paintOnly(root: ParentNode = document): string[] {
  const failures: string[] = [];
  const still = document.createElement("style");
  still.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; scale: none !important; }";
  document.head.append(still);
  try {
    const parts = [...root.querySelectorAll(controls)].filter(live);
    const watched = [...parts, ...Object.values(anchors).flatMap(({ selector }) => [...root.querySelectorAll(selector)])];
    const before = watched.map(boxOf);
    for (const part of parts) for (const state of paintStates) {
      if (state === "aria-expanded" ? part.getAttribute(state) !== "false" : part.hasAttribute(state)) continue;
      const previous = part.getAttribute(state);
      part.setAttribute(state, state === "aria-expanded" ? "true" : "");
      const after = watched.map(boxOf);
      if (previous === null) part.removeAttribute(state); else part.setAttribute(state, previous);
      let worst: { node: Element; delta: number; edge: string } | undefined;
      after.forEach((box, index) => {
        for (const edge of ["x", "y", "width", "height"] as const) {
          const delta = box[edge] - before[index]![edge];
          if (Math.abs(delta) > tolerance && (!worst || Math.abs(delta) > Math.abs(worst.delta))) worst = { node: watched[index]!, delta, edge };
        }
      });
      if (worst) failures.push(`[C24 states] ${label(part)} [${state}] ${worst.node === part ? "changes its own" : `moves ${label(worst.node)}`} ${worst.edge} by ${worst.delta.toFixed(1)}px`);
    }
  } finally { still.remove(); }
  return failures;
}

/** Surfaces that float over the window or stand in for it, and the layer role tokens they may take. */
export const surfaces = {
  selector: ".ui-overlay, .menu-popover, .ui-popover, .peek-card, .ui-view",
  roles: ["--surface", "--surface-overlay", "--layer-base", "--layer-inset", "--layer-overlay", "--layer-view"],
} as const;

/** [C24 layers] One surface floats at a time: a menu opened inside another surface
 * (ui/Layer marks it) is the only second one. */
export function oneSurface(root: ParentNode = document): string[] {
  const shown = [...root.querySelectorAll("[data-layer-surface]")].filter((node) => node.getBoundingClientRect().width
    && !node.matches("[data-layer-within]") && !node.parentElement?.closest("[data-layer-surface]"));
  return shown.length > 1 ? [`[C24 layers] ${shown.length} surfaces at once: ${shown.map(label).join(", ")}`] : [];
}

/** A colour as the page paints it, in sRGB bytes and alpha: a canvas resolves any syntax the engine accepts. */
function paint(colour: string): [number, number, number, number] {
  const plain = /^rgba?\(([\d.]+),? ([\d.]+),? ([\d.]+)(?:,? \/? ?([\d.]+%?))?\)$/.exec(colour.trim());
  if (plain) return [+plain[1]!, +plain[2]!, +plain[3]!, plain[4] === undefined ? 1 : plain[4].endsWith("%") ? parseFloat(plain[4]) / 100 : +plain[4]];
  // A DOM without a cascade (jsdom) leaves a variable unresolved: no colour.
  if (!colour || colour.includes("var(")) return [0, 0, 0, 0];
  const context = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
  if (!context) return [0, 0, 0, 0];
  context.fillStyle = "#000"; context.fillStyle = colour;
  context.fillRect(0, 0, 1, 1);
  const [r, g, b, a] = context.getImageData(0, 0, 1, 1).data;
  return [r!, g!, b!, a! / 255];
}
const luminance = (rgb: readonly number[]) => { const [r, g, b] = rgb.slice(0, 3).map((v) => { const c = v / 255; return c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4; }); return .2126 * r! + .7152 * g! + .0722 * b!; };

/** The colour a node shows: its own and its ancestors' translucent fills over the window's. */
function backdrop(node: Element): number[] {
  const fills: [number, number, number, number][] = [];
  for (let part: Element | null = node; part; part = part.parentElement) {
    const colour = paint(getComputedStyle(part).backgroundColor);
    if (colour[3] > 0) fills.push(colour);
    if (colour[3] >= 1) break;
  }
  let base = matchMedia("(prefers-color-scheme: dark)").matches ? [0, 0, 0] : [255, 255, 255];
  for (const [r, g, b, a] of fills.reverse()) base = [r * a + base[0]! * (1 - a), g * a + base[1]! * (1 - a), b * a + base[2]! * (1 - a)];
  return base;
}

/** [C24 layers] Every floating surface takes a layer role and follows the
 * window's theme (Ibis's own choice, or the system's when it follows it): a
 * dark window never shows a light menu, and the surface's words keep 4.5:1. */
export function layerRoles(root: ParentNode = document): string[] {
  const failures: string[] = [];
  const chosen = document.documentElement.dataset.appearance;
  const dark = chosen === "dark" || (chosen !== "light" && matchMedia("(prefers-color-scheme: dark)").matches);
  for (const surface of root.querySelectorAll(surfaces.selector)) {
    if (!surface.getBoundingClientRect().width || getComputedStyle(surface).visibility === "hidden") continue;
    const own = getComputedStyle(surface).backgroundColor, fill = paint(own);
    const colour = backdrop(surface), light = luminance(colour);
    if (dark ? light > .2 : light < .4) failures.push(`[C24 layers] ${label(surface)}: luminance ${light.toFixed(2)} in a ${dark ? "dark" : "light"} window`);
    const ink = paint(getComputedStyle(surface).color), mixed = ink.slice(0, 3).map((value, index) => value * ink[3] + colour[index]! * (1 - ink[3]));
    const [high, low] = [luminance(mixed), light].sort((a, b) => b - a), ratio = (high! + .05) / (low! + .05);
    if (ratio < 4.5) failures.push(`[C24 layers] ${label(surface)}: words ${ratio.toFixed(2)}:1 on it`);
    if (fill[3] === 0) continue;
    // Its own fill is a role token, resolved where it sits.
    const probe = document.createElement("i");
    probe.style.cssText = "position:absolute;visibility:hidden";
    surface.append(probe);
    const roles = surfaces.roles.map((role) => { probe.style.backgroundColor = `var(${role})`; return paint(getComputedStyle(probe).backgroundColor); });
    probe.remove();
    if (!roles.some((role) => role[3] > 0 && role.every((value, index) => Math.abs(value - fill[index]!) < (index === 3 ? .02 : 2))))
      failures.push(`[C24 layers] ${label(surface)}: background ${own} is no layer role`);
  }
  return failures;
}

/** [C24 words] Slots with a fixed number of lines. canvas/audit.ts sets every
 * catalogue message each can show in it, at the narrowest window. */
export const slots = {
  /** What passes, Undo and remarks (ui/NoticeSlot), in the frame's notice line. */
  noticeSlot: { selector: ".ui-notice-line .notice-band > span", lines: 1, scenario: "undo-notice", catalogue: true },
  /** The frame's notice line (ui/Frame), its first notice's words. */
  /** It also shows the engine's own sentences (a settings notice, a project
   * refusal), which no table can list: only the catalogue's are measured. */
  noticeLine: { selector: ".ui-notice-line > .ui-line-notice:first-of-type > .ui-line-words", lines: 1, scenario: "project-check-conflict", catalogue: false },
} as const;

/** Why words do not fit their slot: cut, or on more lines than it has. */
export function overflow(node: HTMLElement, lines: number): string | null {
  const style = getComputedStyle(node), line = parseFloat(style.lineHeight) || parseFloat(style.fontSize) * 1.3;
  if (node.scrollWidth > node.clientWidth + tolerance) return `cut by ${node.scrollWidth - node.clientWidth}px`;
  if (node.scrollHeight > lines * line + tolerance) return `${Math.round(node.scrollHeight / line)} lines`;
  return null;
}
