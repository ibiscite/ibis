import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { CitationDetails } from "../src/ui/CitationDetails";
import { Band } from "../src/ui/Band";
import { Progress } from "../src/ui/Progress";
import { anchorMoves, focusInView, geometry, layerRoles, menuRows, occlusion, overflow, paintOnly, rowsInView } from "./geometry";

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

test("footer geometry catches a short selector, status off the leading edge and a menu covering its anchor", () => {
  const view = render(<main><div className="result-tile" /><footer className="library-bar">
    <button type="button" className="library-status-trigger" data-band="leading"><span className="library-status">thesis.bib</span></button>
    <span className="more"><button type="button" className="ui-button more-button">More</button></span>
  </footer><div className="menu-popover"><div id="library-menu" /></div></main>);
  let height = 24, statusX = 20, menuX = 8, menuBottom = 442, moreEnd = 712;
  const boxes = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.matches(".library-status-trigger")) return new DOMRect(8, 448, 210, height);
    if (this.matches(".more .ui-button")) return new DOMRect(moreEnd - 72, 448, 72, 24);
    if (this.matches(".library-status")) return new DOMRect(statusX, 448, 190, 24);
    if (this.matches(".menu-popover")) return new DOMRect(menuX, 200, 240, menuBottom - 200);
    if (this.style.insetInlineEnd === "var(--inset)") return new DOMRect(680, 0, 32, 1);
    if (this.style.insetInlineStart === "var(--inset)") return new DOMRect(8, 0, 32, 1);
    if (this.style.insetInlineStart.includes("--title-start")) return new DOMRect(64, 0, 32, 1);
    return new DOMRect(20, 0, 32, 32);
  });
  const failures = () => geometry(view.container).filter((failure) => /^\[C[38]\]/.test(failure));
  try {
    expect(failures()).toEqual([]);
    height = 20;
    expect(failures()).toContainEqual(expect.stringContaining("library selector 20.0px tall"));
    height = 24; statusX = 24;
    expect(failures()).toContainEqual(expect.stringContaining("starts 24.0, leading edge 20.0"));
    // More is the band's last control: its plate ends on the inset, as the rows' fills do (#1923).
    statusX = 20; moreEnd = 702;
    expect(failures()).toContainEqual("[C3] button.ui-button.more-button: plate ends 702.0, window inset 712.0");
    moreEnd = 712;
    statusX = 20; menuX = 20; menuBottom = 460;
    expect(failures()).toContainEqual(expect.stringContaining("library menu spans 20.0–260.0, not its selector 8.0–218.0"));
    expect(failures()).toContainEqual(expect.stringContaining("must leave a gap above selector"));
  } finally { boxes.mockRestore(); view.unmount(); }
});

test("view geometry holds the back chevron, search mark and palette tiles on one column and centres Back vertically", () => {
  vi.stubGlobal("matchMedia", () => ({ matches: false }));
  const view = render(<main><div className="search-bar"><span className="search-mark" /></div>
    <div className="ui-view-head"><button type="button" className="ui-view-back"><span className="ui-icon" /></button>
      <input className="ui-view-field" aria-label="Search formats" style={{ border: 0, padding: 0 }} />
    </div><Band icon={<span />}>Label</Band></main>);
  let glyphX = 28, backY = 10;
  const boxes = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.matches(".search-mark")) return new DOMRect(24, 14, 24, 24);
    if (this.matches(".ui-view-back")) return new DOMRect(20, backY, 32, 32);
    if (this.matches("[data-band-slot=lead]")) return new DOMRect(glyphX - 8, 10, 32, 32);
    if (this.parentElement?.matches("[data-band]")) return new DOMRect(64, 10, 100, 24);
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

test("Band slots hold update progress and new rows to their declared columns", () => {
  const view = render(<main><Progress title="Update" label="Downloading" value={50} /><Band>Heading</Band></main>);
  let titleX = 64, leadX = 20;
  const boxes = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.style.insetInlineStart === "var(--inset)") return new DOMRect(8, 0, 32, 1);
    if (this.style.insetInlineStart.includes("--title-start")) return new DOMRect(64, 0, 32, 1);
    if (this.matches(".update-text")) return new DOMRect(titleX, 0, 100, 24);
    if (this.matches("[data-band-slot=lead]")) return new DOMRect(leadX, 0, 32, 32);
    return new DOMRect(20, 0, 32, 32);
  });
  const failures = () => geometry(view.container).filter((failure) => failure.startsWith("[C3]"));
  try {
    expect(failures()).toEqual([]);
    titleX = 20;
    expect(failures()).toContainEqual(expect.stringContaining("title column 64.0"));
    titleX = 64; leadX = 24;
    expect(failures()).toContainEqual(expect.stringContaining("tile column 36.0"));
  } finally { boxes.mockRestore(); view.unmount(); }
});

test("the frame guard catches a frame that ends short of the window or past it", () => {
  const view = render(<main className="ui-frame" />);
  const edge = vi.spyOn(document.documentElement, "clientHeight", "get").mockReturnValue(480);
  const frame = (bottom: number) => vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 720, bottom));
  const frameFailures = () => geometry(view.container).filter((failure) => failure.includes("the frame spans"));
  try {
    // Text at 80% that zoomed the whole document left the footer 96 px above the edge.
    let boxes = frame(384);
    expect(frameFailures()).toEqual(["[C24 frame] the frame spans 0.0–384.0px of a 480px window: it ends 96.0px short of its edge"]);
    boxes.mockRestore(); boxes = frame(576);
    expect(frameFailures()).toEqual(["[C24 frame] the frame spans 0.0–576.0px of a 480px window: it ends 96.0px past its edge"]);
    boxes.mockRestore(); boxes = frame(480);
    expect(frameFailures()).toEqual([]);
    boxes.mockRestore();
  } finally { edge.mockRestore(); view.unmount(); }
});

test("the dense list counts only rows wholly inside the list's scroller", () => {
  const view = render(<main><div className="picker-content"><ul id="results">
    {Array.from({ length: 10 }, (_, row) => <li key={row} role="option" aria-selected={row === 0} data-row={row} />)}
  </ul></div></main>);
  let bottom = 400;
  const boxes = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.matches(".picker-content")) return new DOMRect(0, 90, 720, bottom - 90);
    // The highlighted row is two lines (52 px), the rest one (34 px).
    const row = Number(this.dataset.row);
    return new DOMRect(8, 90 + (row ? 52 + (row - 1) * 34 : 0), 704, row ? 32 : 52);
  });
  try {
    expect(rowsInView(view.container)).toBe(8);
    bottom = 340;
    expect(rowsInView(view.container)).toBe(6);
  } finally { boxes.mockRestore(); view.unmount(); }
});

/** An open menu of `count` 28 px rows from y = 100, with a filter above them,
 * inside `clip` (a clipping ancestor) when given. */
function openMenu(count: number, clip?: { top: number; bottom: number; right?: number }) {
  const rows = Array.from({ length: count }, (_, row) => <div key={row} role="menuitemradio" aria-checked={false} data-row={row}>Term {row}</div>);
  const view = render(<div className="clip" style={clip ? { overflowX: "hidden", overflowY: "hidden" } : undefined}><div className="menu-popover"><input aria-label="Filter types" />
    <div role="menu" aria-label="Locator type">{rows}</div></div></div>);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.matches(".clip")) return clip ? new DOMRect(0, clip.top, clip.right ?? 300, clip.bottom - clip.top) : new DOMRect(0, 0, 1024, 768);
    if (this.matches("input")) return new DOMRect(10, 64, 280, 32);
    if (this.dataset.row) return new DOMRect(10, 100 + Number(this.dataset.row) * 28, 280, 28);
    return new DOMRect(0, 60, 300, 100 + count * 28);
  });
  const paintedAt = vi.fn<(x: number, y: number) => HTMLElement | null>((x, y) => [...view.container.querySelectorAll<HTMLElement>("input, [data-row]")].find((node) => {
    const box = node.getBoundingClientRect();
    return x >= box.left && x <= box.right && y >= box.top && y <= box.bottom;
  }) ?? null);
  return { view, paintedAt, readPaint: menuPaint(paintedAt) };
}

/** jsdom supplies the geometry. Return the pixel that a screenshot would see
 * from the topmost paint at the sample. The audit exercises real screenshots. */
function menuPaint(paintedAt: (x: number, y: number) => HTMLElement | null): Parameters<typeof menuRows>[0] {
  return async (points) => points.map(({ x, y }) => paintedAt(x, y)?.style.boxShadow.endsWith("white") ? 255 : 0);
}

test("the rows guard passes a menu that shows four whole rows and its filter", async () => {
  const { view, readPaint } = openMenu(29);
  try { expect(await menuRows(readPaint, view.container)).toEqual([]); } finally { vi.restoreAllMocks(); view.unmount(); }
});

test("the rows guard fails a menu whose rows an opaque layer covers", async () => {
  const { view, paintedAt, readPaint } = openMenu(29);
  const cover = document.createElement("div");
  // Planted: the third and following rows' sampled pixels come from the cover.
  const under = paintedAt.getMockImplementation()!;
  paintedAt.mockImplementation((x, y) => y > 150 ? cover : under(x, y));
  try { expect(await menuRows(readPaint, view.container)).toEqual(['[C9 rows] menu "Locator type": 2 of 4 rows visible and whole in a 1024 × 768 window']); } finally { vi.restoreAllMocks(); view.unmount(); }
});

test("a sampled pixel covered by opaque paint fails even when hit testing still finds the row", async () => {
  const { view, paintedAt, readPaint } = openMenu(4);
  const under = paintedAt.getMockImplementation()!;
  document.elementsFromPoint = (x, y) => [under(x, y) ?? document.body];
  try {
    expect(await menuRows(readPaint, view.container)).toEqual([]);
    paintedAt.mockImplementation((x, y) => y === 156 + 14 ? null : under(x, y));
    expect(await menuRows(readPaint, view.container)).toEqual(['[C9 rows] menu "Locator type": 3 of 4 rows visible and whole in a 1024 × 768 window']);
  } finally { vi.restoreAllMocks(); view.unmount(); }
});

test("the rows guard measures again when the first capture settles the menu's position", async () => {
  const { view, readPaint } = openMenu(4);
  const row = view.container.querySelector<HTMLElement>('[data-row="0"]')!;
  const box = row.getBoundingClientRect();
  let shift = 18;
  row.getBoundingClientRect = () => new DOMRect(box.x, box.y + shift, box.width, box.height);
  try {
    expect(await menuRows((points) => {
      shift = 0;
      return readPaint(points);
    }, view.container)).toEqual([]);
  } finally { vi.restoreAllMocks(); view.unmount(); }
});

test("the rows guard fails a menu its clipping ancestor cuts, and one that shows only its filter", async () => {
  // Planted: the menu's box ends after one and a half rows.
  let { view, readPaint } = openMenu(29, { top: 0, bottom: 142 });
  try { expect(await menuRows(readPaint, view.container)).toEqual(['[C9 rows] menu "Locator type": 1 of 4 rows visible and whole in a 1024 × 768 window']); } finally { vi.restoreAllMocks(); view.unmount(); }
  ({ view, readPaint } = openMenu(29, { top: 0, bottom: 98 }));
  try { expect(await menuRows(readPaint, view.container)).toEqual(['[C9 rows] menu "Locator type": 0 of 4 rows visible and whole in a 1024 × 768 window']); } finally { vi.restoreAllMocks(); view.unmount(); }
});

test("the rows guard fails a menu its clipping ancestor or the window cuts across, and one that is not drawn", async () => {
  // Planted: the menu's box is narrower than its filter and rows, which it clips.
  const { view, readPaint } = openMenu(29, { top: 0, bottom: 768, right: 200 });
  try { expect(await menuRows(readPaint, view.container)).toEqual(['[C9 rows] menu "Locator type": its filter is not visible', '[C9 rows] menu "Locator type": 0 of 4 rows visible and whole in a 1024 × 768 window']); } finally { vi.restoreAllMocks(); view.unmount(); }
  const opened = openMenu(29);
  try {
    // Planted: the whole menu at opacity 0 still takes the point.
    opened.view.container.querySelector<HTMLElement>(".menu-popover")!.style.opacity = "0";
    expect(await menuRows(opened.readPaint, opened.view.container)).toEqual(['[C9 rows] menu "Locator type": its filter is not visible', '[C9 rows] menu "Locator type": 0 of 4 rows visible and whole in a 1024 × 768 window']);
  } finally { vi.restoreAllMocks(); opened.view.unmount(); }
});

test("the rows guard asks only for the rows a filtered menu has, and for the filter when nothing matches", async () => {
  let { view, paintedAt, readPaint } = openMenu(2);
  try { expect(await menuRows(readPaint, view.container)).toEqual([]); } finally { vi.restoreAllMocks(); view.unmount(); }
  ({ view, paintedAt, readPaint } = openMenu(0));
  try {
    paintedAt.mockImplementation((_, y) => y < 100 ? view.container.querySelector("input") : view.container.querySelector('[role="menu"]'));
    expect(await menuRows(readPaint, view.container)).toEqual([]);
    // Planted: the filter under another layer.
    paintedAt.mockImplementation(() => document.body);
    expect(await menuRows(readPaint, view.container)).toEqual(['[C9 rows] menu "Locator type": its filter is not visible', '[C9 rows] menu "Locator type": its empty state is not visible']);
  } finally { vi.restoreAllMocks(); view.unmount(); }
});

test("the reveal guard fails focus under the sticky title or out of the scroll box, and passes it in view", () => {
  const view = render(<main className="settings-page"><h1 style={{ position: "sticky" }}>Appearance</h1><div><input aria-label="Medium" type="radio" /></div></main>);
  const page = view.container.querySelector("main")!, radio = screen.getByRole("radio");
  let top = 20;
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this === page) return new DOMRect(200, 0, 700, 560);
    if (this.matches("h1")) return new DOMRect(200, 0, 700, 48);
    if (this === radio) return new DOMRect(220, top, 24, 24);
    return new DOMRect();
  });
  document.elementsFromPoint = () => [radio];
  try {
    radio.focus();
    // Planted: the focused control behind the sticky title.
    expect(focusInView(page)).toEqual(["[C9 reveal] input[Medium] spans 20–44px, outside what shows of main.settings-page (48–560px)"]);
    // Planted: pushed far below the window by content that arrived late.
    top = 1400;
    expect(focusInView(page)).toEqual(["[C9 reveal] input[Medium] spans 1400–1424px, outside what shows of main.settings-page (48–560px)"]);
    top = 60;
    expect(focusInView(page)).toEqual([]);
    // Planted: in the box, but a layer is drawn over it.
    document.elementsFromPoint = () => [page.querySelector("h1")!, radio];
    expect(focusInView(page)).toEqual(["[C9 reveal] input[Medium] is in view but covered by h1"]);
    // Planted: in the box and on top, but not drawn.
    document.elementsFromPoint = () => [radio];
    radio.style.opacity = "0";
    expect(focusInView(page)).toEqual(["[C9 reveal] input[Medium] is in view but not drawn (opacity 0.00)"]);
    radio.style.opacity = "";
    radio.blur();
    expect(focusInView(page)[0]).toContain("the keyboard is not in main.settings-page");
  } finally { vi.restoreAllMocks(); view.unmount(); }
});

test("the rows guard holds a menu opened as a view to every row its list holds whole, and fails one a layer covers", async () => {
  const view = render(<div className="ui-view"><input aria-label="Find a type" /><ul role="listbox" aria-label="Locator type" style={{ overflowX: "hidden", overflowY: "auto" }}>
    {Array.from({ length: 29 }, (_, row) => <li key={row} role="option" aria-selected={row === 0} data-row={row}>Term {row}</li>)}
  </ul></div>);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.matches("input")) return new DOMRect(64, 10, 500, 32);
    if (this.matches('[role="listbox"]')) return new DOMRect(0, 53, 640, 146);
    if (this.dataset.row) return new DOMRect(8, 61 + Number(this.dataset.row) * 42, 624, 40);
    return new DOMRect(0, 0, 640, 240);
  });
  const rows = [...view.container.querySelectorAll<HTMLElement>("input, [data-row]")];
  let covered = -1;
  const readPaint = menuPaint((_, y) => rows.find((node, index) => (covered < 0 || index !== covered + 1) && y >= node.getBoundingClientRect().top && y <= node.getBoundingClientRect().bottom) ?? null);
  try {
    // A short window: three rows fit its list whole, and that is all a view can show.
    expect(await menuRows(readPaint, view.container)).toEqual([]);
    // Planted: the second row under another layer.
    covered = 1;
    expect(await menuRows(readPaint, view.container)).toEqual(['[C9 rows] view "Locator type": 2 of 3 rows visible and whole in a 1024 × 768 window']);
  } finally { vi.restoreAllMocks(); view.unmount(); }
});
