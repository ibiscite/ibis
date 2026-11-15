import { cleanup, render } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { PickerScroll } from "./PickerScroll";

afterEach(cleanup);

/** jsdom has no layout: each part's top comes from `layout`, given whether the
 * list scrolls itself in the DOM as it stands, and its scroll offsets from `scroll`. */
type Part = "middle" | "area" | "row";
function stubLayout(container: HTMLElement, layout: (scrolls: boolean) => Record<Part, number>, scroll: Record<"middle" | "area", number>) {
  const parts: Record<Part, HTMLElement> = {
    middle: container.querySelector(".picker-content")!,
    area: container.querySelector(".results-area")!,
    row: container.querySelector('[role="option"]')!,
  };
  // The middle region scrolls whenever the list does not (ui/Window.css).
  parts.middle.style.overflowY = "auto";
  for (const [name, node] of Object.entries(parts) as [Part, HTMLElement][]) {
    node.getBoundingClientRect = () => {
      // A row moves up as either region above it scrolls.
      const top = layout(parts.area.style.overflowY === "auto")[name] - (name === "row" ? scroll.area + scroll.middle : name === "area" ? scroll.middle : 0);
      return { top, bottom: top + 40, left: 0, right: 720, height: 40, width: 720, x: 0, y: top, toJSON: () => null };
    };
    if (name !== "row") Object.defineProperty(node, "scrollTop", { configurable: true, get: () => scroll[name], set: (value: number) => { scroll[name] = Math.max(0, value); } });
  }
}

function List({ scrolls }: { scrolls: boolean }) {
  return <PickerScroll>
    <div className="results-area" style={{ overflowY: scrolls ? "auto" : "visible" }}><div role="option" aria-selected={false}>Invented reference</div></div>
  </PickerScroll>;
}

test("a frame slot that opens as the list takes over scrolling moves the list, never scrolls it", () => {
  // One commit draws the rows before the frame's heading line reports itself, so
  // the middle region still scrolls and the row sits 8 px into it (at 61).
  // The next commit opens the 32 px heading line: the list scrolls itself, its
  // first row at the top of it (85). Nothing was scrolled, so nothing is.
  const scroll = { middle: 0, area: 0 };
  const view = render(<List scrolls={false} />);
  stubLayout(view.container, (scrolls) => scrolls ? { middle: 85, area: 85, row: 85 } : { middle: 53, area: 53, row: 61 }, scroll);
  view.rerender(<List scrolls />);
  expect(scroll).toEqual({ middle: 0, area: 0 });
});

test("a scrolled list handed to another scroller keeps its first row where it was in it", () => {
  const scroll = { middle: 0, area: 120 };
  const view = render(<List scrolls />);
  stubLayout(view.container, () => ({ middle: 53, area: 61, row: 61 }), scroll);
  // A shorter window: the middle region takes over, the row 120 px above its top as before.
  view.rerender(<List scrolls={false} />);
  expect(scroll.area).toBe(0);
  expect(view.container.querySelector('[role="option"]')!.getBoundingClientRect().top - 53).toBe(-120);
});
