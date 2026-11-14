import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "../../App";
import { say } from "../../i18n";
import { backend, resetApp } from "../../testing/app";
import { dismissMenu, hasMiddleViewport, isSeatedInspector, isWholeNotice, isWholeShelf, layoutGeometry, noticeText, waitForLayout } from "./picker-layout-checks.mjs";

afterEach(async () => {
  await resetApp();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.documentElement.removeAttribute("data-text-size");
});

test("a collapsed middle viewport fails even when the refusal starts exactly at its top", () => {
  vi.stubGlobal("innerWidth", 480); vi.stubGlobal("innerHeight", 360);
  let height = 0, clientHeight = 0;
  render(<main><div className="picker-content"><p>Invented refusal</p></div></main>);
  const middle = document.querySelector(".picker-content");
  vi.spyOn(middle, "getBoundingClientRect").mockImplementation(() => new DOMRect(0, 185, 480, height));
  Object.defineProperty(middle, "clientHeight", { get: () => clientHeight });
  expect(hasMiddleViewport()).toBe(false);
  height = 40;
  expect(hasMiddleViewport()).toBe(false);
  clientHeight = 40;
  expect(hasMiddleViewport()).toBe(true);
  height = 0;
  expect(hasMiddleViewport()).toBe(false);
});

test.each([false, true])("the probe waits for menu exit before the next action (filter=%s)", async (filtered) => {
  let finishExit;
  const detached = new Promise((resolve) => { finishExit = resolve; });
  const menu = { press: vi.fn().mockResolvedValue(undefined), waitFor: vi.fn(() => detached) };
  const filter = { press: vi.fn().mockResolvedValue(undefined) };
  const nextAction = vi.fn();
  const sequence = (async () => {
    await dismissMenu(menu, filtered ? filter : undefined);
    nextAction();
  })();
  // Escape has closed the trigger; the animated menu is still on screen.
  await Promise.resolve();
  expect((filtered ? filter : menu).press).toHaveBeenCalledExactlyOnceWith("Escape");
  expect(menu.waitFor).toHaveBeenCalledExactlyOnceWith({ state: "detached" });
  expect(nextAction).not.toHaveBeenCalled();
  finishExit();
  await sequence;
  expect(nextAction).toHaveBeenCalledOnce();
  if (filtered) expect(menu.press).not.toHaveBeenCalled();
});

test.each([false, true])("a failed search notice takes the notice line in a steady frame (Larger=%s)", async (larger) => {
  const noticeHeight = larger ? 36 : 32;
  let clipTop = 0, clipBottom = 0, visibleHeight = noticeHeight - 8;
  // jsdom supplies an oracle regression; the browser probe checks real layout.
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function () {
    if (this.matches('.ui-notice-line [role="alert"]')) return new DOMRect(64, 57 - clipTop, 600, visibleHeight);
    if (this.matches(".ui-notice-line")) return new DOMRect(0, 53, 720, noticeHeight - clipBottom);
    return new DOMRect();
  });
  const calls = backend({
    picker_preferences: () => ({ larger_text: larger }),
    search: ({ query }) => { if (query.text === "refuse") throw Error("synthetic search refusal"); return []; },
  });
  render(<App />);
  await screen.findByText("Type to find a reference");
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "refuse" } });
  expect((await screen.findByText(say("picker-search-error"))).getAttribute("role")).toBe("alert");
  expect(calls).not.toContain("fit_picker");
  expect(isWholeNotice(noticeText.failedSearch)).toBe(true);
  expect(isWholeNotice("Unrelated synthetic notice")).toBe(false);
  clipBottom = 8;
  expect(isWholeNotice(noticeText.failedSearch)).toBe(false);
  clipBottom = 0; clipTop = 6;
  expect(isWholeNotice(noticeText.failedSearch)).toBe(false);
  clipTop = 0; visibleHeight = 0;
  expect(isWholeNotice(noticeText.failedSearch)).toBe(false);
});

test("the shelf oracle rejects clipping, shrinking and placement inside the middle scroller", () => {
  vi.stubGlobal("matchMedia", () => ({ matches: false }));
  let bounds = new DOMRect(0, 320, 480, 40);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function () {
    if (this.matches(".search-bar")) return new DOMRect(0, 0, 480, 80);
    if (this.matches("footer")) return new DOMRect(0, 360, 480, 40);
    return bounds;
  });
  render(<main><header className="search-bar" /><div className="picker-content" /><div className="shelf" style={{ flexShrink: 0, overflowY: "visible" }}><section className="shelf-output" /></div><footer /></main>);
  const band = document.querySelector(".shelf");
  expect(isWholeShelf()).toBe(true);
  bounds = new DOMRect(0, 320, 480, 42);
  expect(isWholeShelf()).toBe(false);
  bounds = new DOMRect(-2, 320, 480, 40);
  expect(isWholeShelf()).toBe(false);
  bounds = new DOMRect(0, 320, 480, 40);
  band.style.flexShrink = "1";
  expect(isWholeShelf()).toBe(false);
  band.style.flexShrink = "0"; band.style.overflowY = "auto";
  expect(isWholeShelf()).toBe(false);
  band.style.overflowY = "visible";
  document.querySelector(".picker-content").append(band);
  expect(isWholeShelf()).toBe(false);
});

test.each([{ coarse: false, doubled: false }, { coarse: true, doubled: false }, { coarse: false, doubled: true }])("a second shelf line fails with room inside the window (%j)", ({ coarse, doubled }) => {
  vi.stubGlobal("matchMedia", () => ({ matches: coarse }));
  vi.stubGlobal("innerHeight", 600); vi.stubGlobal("innerWidth", 720);
  let height = doubled ? 80 : coarse ? 44 : 40;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function () {
    if (this.matches(".search-bar")) return new DOMRect(0, 0, 720, 80);
    if (this.matches("footer")) return new DOMRect(0, 500, 720, 40);
    return new DOMRect(0, 400, 720, height);
  });
  render(<main><header className="search-bar" /><div className="shelf" style={{ flexShrink: 0, overflowY: "visible", "--space-10": doubled ? "80px" : "40px", "--control-h": doubled ? "64px" : "32px", "--touch-target": "44px" }}><section className="shelf-output" /></div><footer /></main>);
  expect(isWholeShelf()).toBe(true);
  height = doubled ? 96 : 80;
  expect(isWholeShelf()).toBe(false);
});

test("the inspector oracle rejects a panel over the list, the bands or past the window", () => {
  const rects = { inspector: new DOMRect(432, 61, 288, 323), "search-bar": new DOMRect(0, 0, 720, 53), shelf: new DOMRect(8, 392, 704, 40), "picker-content": new DOMRect(0, 53, 432, 339) };
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function () { return rects[this.className]; });
  render(<main><div className="search-bar" /><div className="picker-content" /><section className="inspector" /><div className="shelf" /></main>);
  vi.stubGlobal("innerWidth", 720); vi.stubGlobal("innerHeight", 480);
  expect(isSeatedInspector()).toBe(true);
  rects["picker-content"] = new DOMRect(0, 53, 720, 339);
  expect(isSeatedInspector()).toBe(false);
  rects["picker-content"] = new DOMRect(0, 53, 432, 339);
  rects.inspector = new DOMRect(432, 61, 288, 400);
  expect(isSeatedInspector()).toBe(false);
  rects.inspector = new DOMRect(440, 61, 288, 323);
  expect(isSeatedInspector()).toBe(false);
  vi.unstubAllGlobals();
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
