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
  marks: [".result-tile", ".search-bar .search-mark", ".library-bar .status-dot", ".library-bar .update-ring"],
  words: [".result-title", ".search-bar input", ".library-bar .library-words", ".library-bar .notice-band > span:first-child", ".library-bar .update-text", ".shelf-citation"],
  outside: ".choice-screen, .inspector, .menu-popover, .ui-view, [inert]",
} as const;

/** [C21] Pop-ups that fill a form row: the menu sits under the button's leading
 * edge and is never narrower than it. */
export const anchored = { popover: '.toolbar-menu-popover[data-anchor="field"]', trigger: '.citation-field .ui-popup-button[aria-expanded="true"]' } as const;

/** [C9] Action menus fit their labels until the window limits their width,
 * scrolling or not: a scroll bar never takes its width from a label. Pop-up
 * menus of choices deliberately keep one narrow column. */
export const menuLabels = { label: ".menu-popover .menu-label", outside: ".toolbar-menu-popover" } as const;

/** [C19] Rows of one form share both edges: every control's drawn fill. */
export const forms = { form: ".citation-details", controls: ".citation-field .ui-popup-button, .citation-field input" } as const;

/** [C19] A pane divider runs the whole region it splits, as a Mac split view's:
 * from the search bar's divider to the shelf. */
export const panes = { pane: ".inspector", top: ".search-bar", bottom: ".shelf" } as const;

/** Stacked parts never cover each other: each starts below the one before. */
export const stacks = [".inspector-titles", ".inspector-column", ".citation-details", ".peek-card section"] as const;

/** [C12] A grid of choice cards: one height, and each glyph level with its title's first line. */
export const cards = { grid: '.choice-screen[data-grid] .choice-list', card: ".choice-row", glyph: ".choice-tile", title: ".result-title" } as const;

/** [D8] A ring hugs the control it names: a sheet, a menu or anything else only
 * the app focuses (tabindex -1) never draws one around itself. */
export const rings = { containers: '[role="dialog"], [role="menu"], [tabindex="-1"]' } as const;
/** [C8] A status is never cut to a stub ("Z…"): a line whose words end in "…"
 * keeps at least --status-short of them, and the footer folds its hint labels
 * before that. A part that drops whole (an update's phase) is not cut. */
export const stubs = { line: ".library-bar .library-words, .update-text", words: ".library-source, .library-state, .library-age, .update-text > *" } as const;

const tolerance = 1;

/** Every rule broken on the mounted screen, as "[tag] part: detail". Empty when all hold. */
export function geometry(root: ParentNode = document): string[] {
  const failures: string[] = [];
  const rtl = getComputedStyle(document.documentElement).direction === "rtl";
  const width = document.documentElement.clientWidth;
  // Distance from the window's leading edge, in either direction.
  const lead = (rect: DOMRect, end = false) => rtl ? width - (end ? rect.left : rect.right) : end ? rect.right : rect.left;
  const visible = (node: Element) => { const rect = node.getBoundingClientRect(); return rect.width > 0 && rect.height > 0 && getComputedStyle(node).visibility !== "hidden"; };
  const name = (node: Element) => `${node.tagName.toLowerCase()}.${[...node.classList].join(".")}`;

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
    const tileCentre = lead(tile) + tile.width / 2, titleStart = lead(title.getBoundingClientRect());
    probe.remove(); title.remove();
    const held = (selector: string) => [...root.querySelectorAll(selector)].filter((node) => visible(node) && !node.closest(columns.outside));
    for (const node of held(columns.marks.join(", "))) {
      const rect = node.getBoundingClientRect(), centre = lead(rect) + rect.width / 2;
      if (Math.abs(centre - tileCentre) > tolerance) failures.push(`[C3] ${name(node)}: centre ${centre.toFixed(1)}, tile column ${tileCentre.toFixed(1)}`);
    }
    for (const node of held(columns.words.join(", "))) {
      const rect = node.getBoundingClientRect(), style = getComputedStyle(node);
      // A field's words start inside its padding and border.
      const inset = node.matches("input") ? parseFloat(rtl ? style.paddingRight : style.paddingLeft) + parseFloat(rtl ? style.borderRightWidth : style.borderLeftWidth) : 0;
      const start = lead(rect) + inset;
      if (Math.abs(start - titleStart) > tolerance) failures.push(`[C3] ${name(node)}: starts ${start.toFixed(1)}, title column ${titleStart.toFixed(1)}`);
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
  return failures;
}
