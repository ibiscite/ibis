import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "../../App";
import { pickerText } from "../../messages";
import { backend, resetApp } from "../../testing/app";
import { isWholeNotice, layoutGeometry, noticeText, waitForLayout } from "./picker-layout-checks.mjs";

afterEach(async () => {
  await resetApp();
  vi.restoreAllMocks();
  document.documentElement.removeAttribute("data-text-size");
});

test.each([false, true])("a failed search notice stays reachable in a steady frame (Larger=%s)", async (larger) => {
  const frame = larger ? 552 : 480, noticeHeight = larger ? 64 : 32;
  let clipTop = 0, clipBottom = 0, visibleHeight = noticeHeight;
  // jsdom supplies an oracle regression; the browser probe checks real layout.
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function () {
    if (this.matches('.notices [role="alert"]')) return new DOMRect(0, 53 - clipTop, 720, visibleHeight);
    if (this.matches(".picker-content")) return new DOMRect(0, 53, 720, frame - 109 - clipBottom);
    return new DOMRect();
  });
  const calls = backend({
    picker_preferences: () => ({ larger_text: larger }),
    search: ({ query }) => { if (query.text === "refuse") throw Error("synthetic search refusal"); return []; },
  });
  render(<App />);
  await screen.findByText("Type to find a reference");
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "refuse" } });
  expect((await screen.findByText(pickerText.searchError)).getAttribute("role")).toBe("alert");
  expect(calls).not.toContain("fit_picker");
  expect(isWholeNotice(noticeText.failedSearch)).toBe(true);
  expect(isWholeNotice("Unrelated synthetic notice")).toBe(false);
  clipBottom = frame - 109 - noticeHeight + 2;
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
  await expect(waitForLayout(page, "480/normal/light/scale-2/default", "minimum-text-scroll", () => false))
    .rejects.toThrow("Picker layout wait timed out: case=480/normal/light/scale-2/default; check=minimum-text-scroll; geometry={}");
});
