import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "../../App";
import * as api from "../../api";
import { pickerText } from "../../messages";
import { backend, resetApp } from "../../testing/app";
import { isWholeNotice, layoutGeometry, noticeText, waitForLayout } from "./picker-layout-checks.mjs";

afterEach(async () => {
  await resetApp();
  vi.restoreAllMocks();
  document.documentElement.removeAttribute("data-text-size");
});

test.each([false, true])("a compact failed search is measured and found whole by the browser oracle (Larger=%s)", async (larger) => {
  const edges = 109, hint = 96, noticeHeight = larger ? 64 : 32;
  let frame = edges + hint, clipTop = 0, clipBottom = 0, visibleHeight = noticeHeight;
  // jsdom has no flex layout. Model the containment boundary and native growth;
  // only the real browser probe can establish the CSS sizes and scroll owners.
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function () {
    if (this.tagName !== "MAIN") return 0;
    const middle = this.querySelector(".picker-content");
    const natural = this.style.height === "auto" && middle.style.containerType === "normal";
    return natural ? edges + hint + (middle.querySelector('[role="alert"]') ? noticeHeight : 0) : frame;
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function () {
    if (this.matches('.notices [role="alert"]')) return new DOMRect(0, 53 - clipTop, 720, visibleHeight);
    if (this.matches(".picker-content")) return new DOMRect(0, 53, 720, frame - edges - clipBottom);
    return new DOMRect();
  });
  const fitPicker = vi.fn(async (height) => { if (height !== null) frame = Math.max(frame, height); });
  backend({
    picker_preferences: () => ({ larger_text: larger }),
    search: ({ query }) => { if (query.text === "refuse") throw Error("synthetic search refusal"); return []; },
  });
  const view = render(<App api={{ ...api, fitPicker }} />);
  await screen.findByText("Type to find a reference");
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "refuse" } });
  const alert = await screen.findByText(pickerText.searchError);
  expect(alert.getAttribute("role")).toBe("alert");
  expect(view.container.querySelector("main").hasAttribute("data-compact")).toBe(true);
  expect(fitPicker).toHaveBeenLastCalledWith(edges + hint + noticeHeight, expect.any(Boolean));
  expect(isWholeNotice(noticeText.failedSearch)).toBe(true);

  // The same oracle must refuse missing, zero-height and clipped notices.
  expect(isWholeNotice("Unrelated synthetic notice")).toBe(false);
  clipBottom = frame - edges - noticeHeight + 2;
  expect(isWholeNotice(noticeText.failedSearch)).toBe(false);
  clipBottom = 0; clipTop = 2;
  expect(isWholeNotice(noticeText.failedSearch)).toBe(false);
  clipTop = 0; visibleHeight = 0;
  expect(isWholeNotice(noticeText.failedSearch)).toBe(false);
});

test.each(["TimeoutError", "Error"])("a %s during a probe wait reports its case, check and numeric geometry only", async (name) => {
  render(<main className="unreported synthetic class"><div className="picker-content"><div className="notices"><p role="alert">unreported synthetic payload</p></div></div></main>);
  const error = Object.assign(new Error("unreported synthetic exception"), { name });
  const page = { waitForFunction: vi.fn().mockRejectedValue(error), evaluate: vi.fn(async (read) => read()) };
  const predicate = () => false;
  const geometry = layoutGeometry();
  const numbersOnly = (value) => typeof value === "number" || Object.values(value).every(numbersOnly);
  expect(numbersOnly(geometry)).toBe(true);
  const reason = name === "TimeoutError" ? "timed out" : "failed";
  await expect(waitForLayout(page, "720/normal/light/scale-1/default", "failed-search-notice-whole", predicate, 32))
    .rejects.toThrow(`Picker layout wait ${reason}: case=720/normal/light/scale-1/default; check=failed-search-notice-whole; geometry=${JSON.stringify(geometry)}`);
  expect(page.waitForFunction).toHaveBeenCalledExactlyOnceWith(predicate, 32);
});

test("a probe timeout keeps its fixed labels when the page cannot supply geometry", async () => {
  const page = {
    waitForFunction: vi.fn().mockRejectedValue({ name: "TimeoutError" }),
    evaluate: vi.fn().mockRejectedValue(new Error("unreported synthetic page failure")),
  };
  await expect(waitForLayout(page, "360/normal/light/scale-3/default", "extreme-wheel-scroll", () => false))
    .rejects.toThrow("Picker layout wait timed out: case=360/normal/light/scale-3/default; check=extreme-wheel-scroll; geometry={}");
});
