// Shared by the real-browser probe and its DOM regression; no browser launch here.
export const noticeText = {
  startup: "Synthetic startup settings notice.",
  failedSearch: "Ibis could not search your library. Use Reload, then try again.",
  formatting: "The invented reference needs a page or another citation form. ".repeat(20).trim(),
  entryWarning: "The invented page is not included in this reference entry.",
};

// A closed React Aria menu stays mounted during its exit animation. Await
// removal before another overlay or picker action can take keyboard input.
export async function dismissMenu(menu, keyTarget = menu) {
  await keyTarget.press("Escape");
  await menu.waitFor({ state: "detached" });
}

export function shelfControl(page, name) {
  // An open menu marks its visible background aria-hidden; intersect physical
  // shelf controls so menu rows never become a second match for their cause.
  return page.getByRole("button", { name, exact: true, includeHidden: true })
    .and(page.locator("main > .shelf button:visible"));
}

// A visible/enabled hidden radio can still resolve outside its label or be
// covered by it. Test the native control's real hit area, not just its name.
export function isWholeRadioTarget(input) {
  const label = input.closest(".citation-form");
  if (!label || input.type !== "radio") return false;
  const target = input.getBoundingClientRect(), row = label.getBoundingClientRect();
  return row.width > 0 && row.height > 0 && ["left", "right", "top", "bottom"].every((edge) => Math.abs(target[edge] - row[edge]) <= 1) &&
    document.elementFromPoint(target.left + target.width / 2, target.top + target.height / 2) === input;
}

// Keep this function self-contained: Playwright serializes it into the page.
export function isWholeNotice(text) {
  const node = [...document.querySelectorAll('.notices [role="alert"]')].find((node) => node.textContent === text);
  if (!node) return false;
  const rect = node.getBoundingClientRect(), port = node.closest(".picker-content").getBoundingClientRect();
  return rect.height > 0 && rect.top >= port.top - 1 && rect.bottom <= port.bottom + 1;
}

// Report the three refusal requirements separately so the built probe identifies
// whether visibility, complete wrapping or keyboard reach failed. Self-contained
// because Playwright serializes this function into the page.
export function refusalPresentation(node) {
  const style = getComputedStyle(node);
  const rect = node.getBoundingClientRect(), bounds = node.closest(".picker-content").getBoundingClientRect();
  return {
    startsInView: rect.top >= bounds.top - 1 && rect.top < bounds.bottom,
    wrapsInFull: style.whiteSpace === "normal" && style.overflowY === "visible" &&
      node.scrollWidth <= node.clientWidth + 1 && node.scrollHeight <= node.clientHeight + 1,
    keyboardReachable: node.tabIndex === 0,
  };
}

// Diagnostic values stay separate from the three pass/fail requirements.
export function refusalGeometry(node) {
  const owner = node.closest(".picker-content");
  const rect = node.getBoundingClientRect(), bounds = owner.getBoundingClientRect();
  return { rect: { top: rect.top, bottom: rect.bottom, width: rect.width }, bounds: { top: bounds.top, bottom: bounds.bottom }, scrollTop: owner.scrollTop,
    viewport: { width: innerWidth, height: innerHeight } };
}

// A long notice cannot be read in a collapsed scrollport, even when its own
// box is whole and starts at the scrollport's top. Keep this independent of
// the refusal checks so every picker state must retain a middle viewport.
export function hasMiddleViewport() {
  const middle = document.querySelector("main > .picker-content");
  return Boolean(middle && middle.clientHeight > 0 && middle.getBoundingClientRect().height > 0);
}

export function isWholeShelf() {
  const shelf = document.querySelector("main > .shelf");
  if (!shelf) return document.querySelector("main .shelf") === null;
  const rect = shelf.getBoundingClientRect();
  const output = shelf.querySelector(":scope > .shelf-output");
  if (!output || shelf.children.length !== 1 || Math.abs(output.getBoundingClientRect().height - rect.height) > 1) return false;
  const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
  const pixels = (value) => value.endsWith("rem") ? parseFloat(value) * rem : value.endsWith("px") ? parseFloat(value) : NaN;
  const style = getComputedStyle(shelf);
  const line = pixels(style.getPropertyValue("--space-10").trim());
  const target = pixels(style.getPropertyValue(matchMedia("(pointer: coarse)").matches ? "--touch-target" : "--control-h").trim());
  const expected = Math.max(line, target);
  if (Number.isFinite(expected) && Math.abs(rect.height - expected) > 1) return false;
  const header = document.querySelector(".search-bar").getBoundingClientRect();
  const footer = document.querySelector("footer").getBoundingClientRect();
  return rect.height > 0 && rect.top >= header.bottom - 1 && rect.bottom <= footer.top + 1 &&
    rect.left >= 0 && rect.right <= innerWidth + 1 && footer.bottom <= innerHeight + 1 &&
    getComputedStyle(shelf).flexShrink === "0" && !/auto|scroll/.test(getComputedStyle(shelf).overflowY);
}

// A centred panel can fit the viewport and still fail the segment-menu rule.
// Check the actual anchor gap and the same corner token as the shared menu.
export function isAnchoredDetails(anchor) {
  const menus = document.querySelectorAll(".details-menu");
  if (menus.length !== 1) return false;
  const menu = menus[0], rect = menu.getBoundingClientRect(), style = getComputedStyle(menu);
  const gap = menu.dataset.placement === "top" ? anchor.top - rect.bottom
    : menu.dataset.placement === "bottom" ? rect.top - anchor.bottom : NaN;
  const radius = style.getPropertyValue("--menu-radius").trim();
  return rect.width > 0 && rect.height > 0 && rect.left >= 7 && rect.top >= 7 && rect.right <= innerWidth - 7 && rect.bottom <= innerHeight - 7 &&
    Math.abs(gap - 6) <= 2 && Math.min(rect.right, anchor.right) > Math.max(rect.left, anchor.left) &&
    radius !== "" && radius === style.getPropertyValue("--r-menu").trim();
}

// Only fixed keys and numeric geometry may cross out of the page on failure.
export function layoutGeometry() {
  const box = (node) => {
    if (!node) return [];
    const rect = node.getBoundingClientRect();
    return [rect.left, rect.top, rect.width, rect.height, node.clientHeight, node.scrollHeight, node.scrollTop, node.scrollLeft];
  };
  return {
    viewport: [innerWidth, innerHeight],
    root: box(document.documentElement), body: box(document.body),
    main: box(document.querySelector("main")), search: box(document.querySelector(".search-bar")),
    middle: box(document.querySelector(".picker-content")), results: box(document.querySelector(".results-area")),
    active: box(document.querySelector('[role="option"][aria-selected="true"]')),
    shelf: box(document.querySelector(".shelf")), footer: box(document.querySelector("footer")),
    menu: box(document.querySelector(".details-menu")), menuScroll: box(document.querySelector(".details-menu-scroll")),
    notices: [...document.querySelectorAll(".notices")].map(box),
    alerts: [...document.querySelectorAll('.notices [role="alert"]')].map(box),
    segments: [...document.querySelectorAll(".shelf-segment")].map(box),
    radios: [...document.querySelectorAll(".citation-form input")].map(box),
    radioRows: [...document.querySelectorAll(".citation-form")].map(box),
  };
}

export async function waitForLayout(page, caseLabel, checkLabel, predicate, arg) {
  try {
    await page.waitForFunction(predicate, arg);
  } catch (error) {
    const geometry = await page.evaluate(layoutGeometry).catch(() => ({}));
    const reason = error?.name === "TimeoutError" ? "timed out" : "failed";
    // Do not attach the original error: it may contain page text or IPC data.
    throw new Error(`Picker layout wait ${reason}: case=${caseLabel}; check=${checkLabel}; geometry=${JSON.stringify(geometry)}`);
  }
}
