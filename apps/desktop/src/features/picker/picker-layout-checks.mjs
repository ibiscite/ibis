// Shared by the real-browser probe and its DOM regression; no browser launch here.
export const noticeText = {
  startup: "Synthetic startup settings notice.",
  failedSearch: "Ibis could not search your library. Use Reload, then try again.",
};

// A closed React Aria menu stays mounted during its exit animation. Await
// removal before another overlay or picker action can take keyboard input.
export async function dismissMenu(menu, keyTarget = menu) {
  await keyTarget.press("Escape");
  await menu.waitFor({ state: "detached" });
}

// Keep this function self-contained: Playwright serializes it into the page.
export function isWholeNotice(text) {
  const node = [...document.querySelectorAll('.notices [role="alert"]')].find((node) => node.textContent === text);
  if (!node) return false;
  const rect = node.getBoundingClientRect(), port = node.closest(".picker-content").getBoundingClientRect();
  return rect.height > 0 && rect.top >= port.top - 1 && rect.bottom <= port.bottom + 1;
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
    notices: [...document.querySelectorAll(".notices")].map(box),
    alerts: [...document.querySelectorAll('.notices [role="alert"]')].map(box),
    chips: [...document.querySelectorAll(".shelf-chips > .chip")].map(box),
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
