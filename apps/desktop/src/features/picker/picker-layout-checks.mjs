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

// Keep this function self-contained: Playwright serializes it into the page.
// A notice takes the frame's one notice line (ui/Frame): shown there, inside the
// line's box, with its whole sentence kept for its tooltip and screen readers.
export function isWholeNotice(text) {
  const node = [...document.querySelectorAll('.ui-notice-line [role="alert"]')].find((node) => node.textContent === text);
  if (!node) return false;
  const rect = node.getBoundingClientRect(), line = node.closest(".ui-notice-line").getBoundingClientRect();
  return rect.height > 0 && rect.top >= line.top - 1 && rect.bottom <= line.bottom + 1 && node.title === text;
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

// The inspector sits in the middle row and never overlaps a list in view.
export function isSeatedInspector() {
  const panes = document.querySelectorAll(".inspector");
  if (panes.length !== 1) return false;
  const rect = panes[0].getBoundingClientRect();
  const search = document.querySelector(".search-bar").getBoundingClientRect(), shelf = document.querySelector(".shelf").getBoundingClientRect();
  const list = document.querySelector(".picker-content");
  const beside = getComputedStyle(list).visibility === "hidden" || list.getBoundingClientRect().right <= rect.left + 1;
  return beside && rect.width > 0 && rect.height > 0 && rect.left >= 0 && rect.right <= innerWidth && rect.top >= search.bottom - 1 && rect.bottom <= shelf.top + 1;
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
    inspector: box(document.querySelector(".inspector")), inspectorScroll: box(document.querySelector(".inspector-body")),
    notices: [...document.querySelectorAll(".ui-notice-line")].map(box),
    alerts: [...document.querySelectorAll('.ui-notice-line [role="alert"]')].map(box),
    segments: [...document.querySelectorAll(".citation-token")].map(box),
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
