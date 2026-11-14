import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { CitationDetails } from "../src/ui/CitationDetails";
import { anchorMoves, geometry, layerRoles, occlusion, overflow, paintOnly } from "./geometry";

test("the frame guard catches an anchor that moves between states, and not an edge it does not hold", () => {
  const idle = { search: { start: 64, end: 600, top: 18, height: 36 }, "first row": { start: 8, end: 712, top: 90, height: 52 } };
  // A wider trailing control moves the field's end, which is not held.
  expect(anchorMoves(idle, { ...idle, search: { ...idle.search, end: 500 } }, "loading")).toEqual([]);
  expect(anchorMoves(idle, { ...idle, "first row": { ...idle["first row"], top: 122 } }, "project-check-conflict"))
    .toEqual(["[C24 frame] first row top +32.0px in project-check-conflict"]);
  // A state that draws no rows moves none.
  expect(anchorMoves(idle, { search: idle.search }, "error")).toEqual([]);
});

test("the cover guard catches a control under another layer and skips inert ones", () => {
  const view = render(<main><button type="button">Covered</button><div className="cover" /><div inert><button type="button">Behind a sheet</button></div></main>);
  const cover = view.container.querySelector(".cover")!;
  const boxes = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(new DOMRect(10, 10, 80, 24));
  const point = vi.fn<(x: number, y: number) => Element | null>(() => cover);
  document.elementFromPoint = point;
  try {
    expect(occlusion(view.container)).toEqual(["[C24 cover] button is covered by div.cover"]);
    point.mockImplementation(() => screen.getByText("Covered"));
    expect(occlusion(view.container)).toEqual([]);
  } finally { boxes.mockRestore(); view.unmount(); }
});

test("the states guard catches hover that grows a chip and moves its neighbour", () => {
  const view = render(<main><button type="button" className="chip">Vale 2021</button><button type="button" className="next">Aster 2026</button></main>);
  const boxes = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const grown = view.container.querySelector(".chip[data-hovered]") ? 16 : 0;
    return this.matches(".chip") ? new DOMRect(10, 10, 80 + grown, 24) : new DOMRect(94 + grown, 10, 80, 24);
  });
  try {
    expect(paintOnly(view.container)).toEqual(["[C24 states] button.chip [data-hovered] changes its own width by 16.0px"]);
  } finally { boxes.mockRestore(); view.unmount(); }
});

test("the layer guard catches a light menu in a dark window", () => {
  document.documentElement.dataset.appearance = "dark";
  vi.stubGlobal("matchMedia", () => ({ matches: false }));
  const view = render(<div className="menu-popover" style={{ background: "rgb(246, 246, 247)", color: "rgb(0, 0, 0)" }}>Menu</div>);
  const boxes = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(new DOMRect(10, 10, 200, 100));
  try {
    expect(layerRoles(view.container)).toContainEqual(expect.stringContaining("[C24 layers] div.menu-popover: luminance 0.92 in a dark window"));
    view.container.querySelector<HTMLElement>(".menu-popover")!.style.background = "rgb(44, 44, 46)";
    view.container.querySelector<HTMLElement>(".menu-popover")!.style.color = "rgb(255, 255, 255)";
    expect(layerRoles(view.container).filter((failure) => !failure.includes("no layer role"))).toEqual([]);
  } finally { boxes.mockRestore(); view.unmount(); delete document.documentElement.dataset.appearance; vi.unstubAllGlobals(); }
});

test("the words guard catches a message cut in its one-line slot", () => {
  const words = document.createElement("span");
  words.style.lineHeight = "18px";
  Object.defineProperties(words, { scrollWidth: { value: 340, configurable: true }, clientWidth: { value: 220 }, scrollHeight: { value: 18, configurable: true } });
  expect(overflow(words, 1)).toBe("cut by 120px");
  Object.defineProperties(words, { scrollWidth: { value: 220 }, scrollHeight: { value: 36 } });
  expect(overflow(words, 1)).toBe("2 lines");
});

test("form alignment measures a combined locator's fill and still reports a misplaced field", () => {
  const view = render(<CitationDetails locator={null} form="normal" prefix="see" suffix="" choices={{ current_form: "normal", forms: [
    { form: "normal", example: "(Vale, 2021)", reason: null },
    { form: "narrative", example: "Vale (2021)", reason: null },
  ] }} onLocatorChange={() => {}} onFormChange={() => {}} onPrefixChange={() => {}} onSuffixChange={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Text after" }));
  const after = screen.getByRole<HTMLInputElement>("textbox", { name: "Text after" });
  let misplaced: Element | null = null;
  for (const input of view.container.querySelectorAll("input")) input.style.border = "4px solid transparent";
  const boxes = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    // The type selector and value are deliberately narrower than their shared fill.
    const inner = this.closest(".citation-locator");
    if (inner && this !== inner) return new DOMRect(this.matches("input") ? 150 : 100, 0, 50, 24);
    const input = this.matches("input");
    return new DOMRect((input ? 96 : 100) + (this === misplaced ? 8 : 0), 0, input ? 208 : 200, 24);
  });
  const alignment = () => geometry(view.container).filter((failure) => failure.startsWith("[C19]"));
  try {
    expect(alignment()).toEqual([]);
    misplaced = view.container.querySelector(".citation-locator");
    expect(alignment()).toEqual(expect.arrayContaining([expect.stringContaining("div.citation-locator: start 108.0, the form's 100.0")]));
    misplaced = after;
    expect(alignment().some((failure) => failure.startsWith("[C19] input.") && failure.includes("start 108.0, the form's 100.0"))).toBe(true);
  } finally { boxes.mockRestore(); view.unmount(); }
});

test("footer geometry catches a short selector, a shifted dot and a menu covering its anchor", () => {
  const view = render(<main><div className="result-tile" /><footer className="library-bar">
    <button type="button" className="library-status-trigger"><span className="status-dot" /></button>
    <span className="more"><button type="button" className="ui-button more-button">More</button></span>
  </footer><div className="menu-popover"><div id="library-menu" /></div></main>);
  let height = 24, dotX = 32, menuX = 8, menuBottom = 442;
  const boxes = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.matches(".library-status-trigger")) return new DOMRect(8, 448, 210, height);
    if (this.matches(".more .ui-button")) return new DOMRect(640, 448, 72, 24);
    if (this.matches(".status-dot")) return new DOMRect(dotX, 456, 8, 8);
    if (this.matches(".menu-popover")) return new DOMRect(menuX, 200, 240, menuBottom - 200);
    if (this.style.insetInlineStart === "var(--inset)") return new DOMRect(8, 0, 32, 1);
    if (this.style.insetInlineStart.includes("--title-start")) return new DOMRect(64, 0, 32, 1);
    return new DOMRect(20, 0, 32, 32);
  });
  const failures = () => geometry(view.container).filter((failure) => /^\[C[38]\]/.test(failure));
  try {
    expect(failures()).toEqual([]);
    height = 20;
    expect(failures()).toContainEqual(expect.stringContaining("library selector 20.0px tall"));
    height = 24; dotX = 36;
    expect(failures()).toContainEqual(expect.stringContaining("centre 40.0, tile column 36.0"));
    dotX = 32; menuX = 20; menuBottom = 460;
    expect(failures()).toContainEqual(expect.stringContaining("library menu starts 20.0, selector 8.0"));
    expect(failures()).toContainEqual(expect.stringContaining("must leave a gap above selector"));
  } finally { boxes.mockRestore(); view.unmount(); }
});

test("view geometry holds the back chevron, search mark and palette tiles on one column and centres Back vertically", () => {
  vi.stubGlobal("matchMedia", () => ({ matches: false }));
  const view = render(<main><div className="search-bar"><span className="search-mark" /></div>
    <div className="ui-view-head"><button type="button" className="ui-view-back"><span className="ui-icon" /></button>
      <input className="ui-view-field" aria-label="Search formats" style={{ border: 0, padding: 0 }} />
    </div><li className="ui-palette-row"><span className="ui-band-icon" /></li></main>);
  let glyphX = 28, backY = 10;
  const boxes = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.matches(".search-mark")) return new DOMRect(24, 14, 24, 24);
    if (this.matches(".ui-view-back")) return new DOMRect(20, backY, 32, 32);
    if (this.matches(".ui-icon")) return new DOMRect(glyphX, 18, 16, 16);
    if (this.matches(".ui-view-field")) return new DOMRect(64, 8, 400, 36);
    if (this.style.insetInlineStart === "var(--inset)") return new DOMRect(8, 0, 32, 1);
    if (this.style.insetInlineStart.includes("--title-start")) return new DOMRect(64, 0, 32, 1);
    return new DOMRect(20, 0, 32, 32);
  });
  const failures = () => geometry(view.container).filter((failure) => /^\[(C3|Back)\]/.test(failure));
  try {
    expect(failures()).toEqual([]);
    glyphX = 30;
    expect(failures()).toContainEqual(expect.stringContaining("centre 38.0, tile column 36.0"));
    glyphX = 28; backY = 14;
    expect(failures()).toEqual(["[Back] centre 4.0px off the search field vertically"]);
  } finally { boxes.mockRestore(); vi.unstubAllGlobals(); view.unmount(); }
});
