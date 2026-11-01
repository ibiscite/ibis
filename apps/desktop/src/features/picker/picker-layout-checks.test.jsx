import { act, fireEvent, queryAllByRole, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "../../App";
import { CitationShelf } from "./CitationShelf";
import { Notices } from "./Notices";
import { pickerText } from "../../messages";
import { backend, resetApp } from "../../testing/app";
import { dismissMenu, hasMiddleViewport, isAnchoredDetails, isWholeNotice, isWholeRadioTarget, isWholeShelf, layoutGeometry, noticeText, refusalGeometry, refusalPresentation, shelfControl, waitForLayout } from "./picker-layout-checks.mjs";

const { readFileSync } = await vi.importActual("node:fs");
const { URL: FileURL } = await vi.importActual("node:url");
const pickerStyles = readFileSync(new FileURL("../../styles.css", import.meta.url), "utf8");

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

test("shared CSS reserves a readable middle viewport before allocating spare band space", () => {
  const style = document.createElement("style");
  style.textContent = pickerStyles.replace(/@import[^;]+;/g, "");
  document.head.append(style);
  try {
    render(<main><div className="picker-content" /></main>);
    const middle = document.querySelector(".picker-content");
    // jsdom cannot allocate flex geometry. Reject the actual zero minimum
    // that allowed the enlarged fixed bands to consume the whole frame; the
    // browser's hasMiddleViewport check verifies the resulting geometry.
    expect(["", "auto", "0", "0px"]).not.toContain(getComputedStyle(middle).minHeight);
  } finally { style.remove(); }
});

test("the refusal oracle distinguishes offscreen text, clipping and lost keyboard reach", () => {
  vi.stubGlobal("innerWidth", 480); vi.stubGlobal("innerHeight", 360);
  render(<div className="picker-content"><Notices library={null} failures={{}} startAtLogin={null} error={null} citationProblem={noticeText.formatting}
    copyError={null} copied={null} onDismissCopied={() => {}} /></div>);
  const refusal = screen.getByRole("alert"), middle = document.querySelector(".picker-content");
  // jsdom has no layout. The probe supplies real boxes; this fixture proves
  // that its individual diagnostics reject each failure without conflating them.
  refusal.style.whiteSpace = "normal"; refusal.style.overflowY = "visible";
  let top = 80, scrollWidth = 432, scrollHeight = 1176;
  vi.spyOn(middle, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 80, 480, 96));
  vi.spyOn(refusal, "getBoundingClientRect").mockImplementation(() => new DOMRect(24, top, 432, 1176));
  Object.defineProperties(refusal, {
    clientWidth: { get: () => 432 }, scrollWidth: { get: () => scrollWidth },
    clientHeight: { get: () => 1176 }, scrollHeight: { get: () => scrollHeight },
  });
  const whole = { startsInView: true, wrapsInFull: true, keyboardReachable: true };
  expect(refusalPresentation(refusal)).toEqual(whole);
  top = -1824;
  expect(refusalPresentation(refusal)).toEqual({ ...whole, startsInView: false });
  middle.scrollTop = 1904;
  expect(refusalGeometry(refusal)).toEqual({ rect: { top: -1824, bottom: -648, width: 432 }, bounds: { top: 80, bottom: 176 }, scrollTop: 1904,
    viewport: { width: 480, height: 360 } });
  top = 80; scrollWidth += 2;
  expect(refusalPresentation(refusal)).toEqual({ ...whole, wrapsInFull: false });
  scrollWidth = 432; scrollHeight += 2;
  expect(refusalPresentation(refusal)).toEqual({ ...whole, wrapsInFull: false });
  scrollHeight = 1176; refusal.style.whiteSpace = "nowrap";
  expect(refusalPresentation(refusal)).toEqual({ ...whole, wrapsInFull: false });
  refusal.style.whiteSpace = "normal"; refusal.style.overflowY = "hidden";
  expect(refusalPresentation(refusal)).toEqual({ ...whole, wrapsInFull: false });
  refusal.style.overflowY = "visible"; refusal.tabIndex = -1;
  expect(refusalPresentation(refusal)).toEqual({ ...whole, keyboardReachable: false });
});

test("the radio pointer oracle refuses displaced, clipped and covered controls", () => {
  const label = document.createElement("label"), radio = document.createElement("input");
  label.className = "citation-form"; radio.type = "radio"; label.append(radio);
  const row = new DOMRect(24, 80, 400, 48);
  let target = new DOMRect(0, 0, 13, 13), hit = label;
  vi.spyOn(label, "getBoundingClientRect").mockReturnValue(row);
  vi.spyOn(radio, "getBoundingClientRect").mockImplementation(() => target);
  const original = Object.getOwnPropertyDescriptor(document, "elementFromPoint");
  Object.defineProperty(document, "elementFromPoint", { configurable: true, value: () => hit });
  try {
    expect(isWholeRadioTarget(radio)).toBe(false);
    target = row;
    expect(isWholeRadioTarget(radio)).toBe(false);
    hit = radio;
    expect(isWholeRadioTarget(radio)).toBe(true);
    target = new DOMRect(24, 80, 13, 13);
    expect(isWholeRadioTarget(radio)).toBe(false);
  } finally {
    if (original) Object.defineProperty(document, "elementFromPoint", original); else delete document.elementFromPoint;
  }
});

test.each([[720, 480, 1], [480, 360, 1], [480, 360, 2]])("the probe selects one rendered reference-count control at %s×%s/%s× text, including with its menu open", async (width, height, scale) => {
  vi.stubGlobal("innerWidth", width); vi.stubGlobal("innerHeight", height);
  const computed = window.getComputedStyle;
  vi.spyOn(window, "getComputedStyle").mockImplementation((node) => {
    const style = computed(node);
    if (node === document.documentElement) Object.defineProperty(style, "fontSize", { value: `${16 * scale}px` });
    return style;
  });
  const records = Array.from({ length: 9 }, (_, i) => ({
    revision: "1", id: { source: "invented.bib", native_key: `reference${i}` }, key: `reference${i}`, title: `Invented reference ${i}`,
    authors: "Aster", first_author: "Aster", author_count: 1, year: 2026, container: null, item_type: "book", partial: false, matched: ["title"],
  }));
  render(<main><CitationShelf announce={false} selected={new Map(records.map((r) => [r.key, r]))} removeLabel={(r) => `Remove ${r.key} from the selection`}
    onRemove={() => {}} style={null} keys focusable={false} outputWarnings={[]} shown={{ markup: [], text: "Synthetic citation", html: "", warnings: [] }} /><footer /></main>);
  const band = document.querySelector("main > .shelf");
  const name = "9 references";
  const trigger = within(band).getByRole("button", { name, exact: true });
  // Model just the Locator intersection and CSS visibility used by this probe.
  // jsdom has no boxes; hidden/display/visibility exclude retained controls,
  // while aria-hidden deliberately does not change physical visibility.
  const locator = (read) => ({ get nodes() { return read(); }, and: (other) => locator(() => read().filter((node) => other.nodes.includes(node))) });
  const page = {
    getByRole: (role, { includeHidden, ...options }) => locator(() => queryAllByRole(document.body, role, { ...options, hidden: includeHidden })),
    locator: (selector) => locator(() => [...document.querySelectorAll(selector.replaceAll(":visible", ""))].filter((node) => {
      if (!selector.includes(":visible")) return true;
      for (let part = node; part; part = part.parentElement) {
        const style = getComputedStyle(part);
        if (part.hidden || style.display === "none" || style.visibility === "hidden") return false;
      }
      return true;
    })),
  };
  const control = shelfControl(page, name), foldedAnchor = shelfControl(page, /^\d+ references$/);
  expect(control.nodes).toEqual([trigger]);
  expect(foldedAnchor.nodes).toEqual([trigger]);
  fireEvent.click(trigger);
  const dialog = screen.getByRole("dialog", { name: "Selected references", exact: true });
  expect(within(dialog).getAllByRole("button", { name: /^Edit / })).toHaveLength(9);
  expect(band.closest('[aria-hidden="true"]')).not.toBeNull();
  expect(control.nodes).toEqual([trigger]);
  expect(foldedAnchor.nodes).toEqual([trigger]);
  fireEvent.keyDown(dialog, { key: "Escape" });
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(within(band).getByRole("button", { name, exact: true })).toBe(trigger);
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

test("the details oracle rejects a centred panel, clipped menu and unrelated corner token", () => {
  let rect = new DOMRect(100, 100, 320, 180);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(() => rect);
  render(<div className="details-menu" data-placement="top" style={{ "--menu-radius": "12px", "--r-menu": "12px" }} />);
  const anchor = { top: 286, bottom: 310, left: 100, right: 240 };
  expect(isAnchoredDetails(anchor)).toBe(true);
  rect = new DOMRect(100, 180, 320, 180);
  expect(isAnchoredDetails(anchor)).toBe(false);
  rect = new DOMRect(-2, 100, 320, 180);
  expect(isAnchoredDetails(anchor)).toBe(false);
  rect = new DOMRect(100, 100, 320, 180);
  document.querySelector(".details-menu").style.setProperty("--menu-radius", "24px");
  expect(isAnchoredDetails(anchor)).toBe(false);
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
