import { act, cleanup, render } from "@testing-library/react";
import { useRef } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { markScrollEdges, useScrollEdges } from "./ScrollArea";

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function Box() {
  const ref = useRef<HTMLDivElement>(null);
  useScrollEdges(ref);
  return <div ref={ref} className="scroll-area" data-testid="box"><p>One</p></div>;
}

test("a scroll area marks the edges with more content beyond them, as it scrolls and its content changes", async () => {
  const { getByTestId } = render(<Box />);
  const box = getByTestId("box");
  let height = 100, client = 100;
  Object.defineProperty(box, "scrollHeight", { get: () => height });
  Object.defineProperty(box, "clientHeight", { get: () => client });
  const marks = () => [box.hasAttribute("data-more-above"), box.hasAttribute("data-more-below")];
  fireScroll(box);
  expect(marks()).toEqual([false, false]);
  // More content arrives: the bottom edge has rows beyond it.
  height = 300;
  await act(async () => { box.append(document.createElement("p")); await Promise.resolve(); });
  expect(marks()).toEqual([false, true]);
  box.scrollTop = 100;
  fireScroll(box);
  expect(marks()).toEqual([true, true]);
  box.scrollTop = 200;
  fireScroll(box);
  expect(marks()).toEqual([true, false]);
  client = 300;
  box.scrollTop = 0;
  fireScroll(box);
  expect(marks()).toEqual([false, false]);
});

function fireScroll(node: HTMLElement) {
  act(() => { node.dispatchEvent(new Event("scroll")); });
}

test("fades yield to the entire active row without changing the user's scroll position", () => {
  const box = document.createElement("div"), row = document.createElement("div");
  box.append(row); document.body.append(box); row.dataset.focused = "true";
  Object.defineProperty(box, "clientHeight", { value: 100 });
  Object.defineProperty(box, "scrollHeight", { value: 300 });
  vi.spyOn(box, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 200, 100));
  let top = 60, height = 40;
  vi.spyOn(row, "getBoundingClientRect").mockImplementation(() => new DOMRect(0, top, 200, height));
  box.scrollTop = 50;
  markScrollEdges(box);
  expect(box.style.getPropertyValue("--scroll-fade-below-space")).toBe("0px");
  expect(box.style.getPropertyValue("--scroll-fade-above-space")).toBe("60px");
  top = 0; height = 28; markScrollEdges(box);
  expect(box.style.getPropertyValue("--scroll-fade-above-space")).toBe("0px");
  expect(box.style.getPropertyValue("--scroll-fade-below-space")).toBe("72px");
  top = -10; height = 120; markScrollEdges(box);
  expect(box.style.getPropertyValue("--scroll-fade-above-space")).toBe("0px");
  expect(box.style.getPropertyValue("--scroll-fade-below-space")).toBe("0px");
  expect(box.scrollTop).toBe(50);
  row.removeAttribute("data-focused"); markScrollEdges(box);
  expect(box.style.getPropertyValue("--scroll-fade-below-space")).toBe("");
  box.scrollTop = 0; markScrollEdges(box);
  expect(box.hasAttribute("data-more-above")).toBe(false);
  box.scrollTop = 200; markScrollEdges(box);
  expect(box.hasAttribute("data-more-below")).toBe(false);
  box.remove();
});

test("text reflow inside an unchanged viewport refreshes its edges, and late resizes after close cannot update them", () => {
  const callbacks: (() => void)[] = [], observed: Element[] = [];
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: () => void) { callbacks.push(callback); }
    observe(node: Element) { observed.push(node); }
    disconnect() {}
  });
  const view = render(<Box />), box = view.getByTestId("box");
  let contentHeight = 100;
  Object.defineProperty(box, "clientHeight", { value: 100 });
  Object.defineProperty(box, "scrollHeight", { get: () => contentHeight });
  expect(observed).toContain(box.firstElementChild);
  contentHeight = 300;
  act(() => callbacks[0]!());
  expect(box.hasAttribute("data-more-below")).toBe(true);
  view.unmount();
  box.removeAttribute("data-more-below");
  act(() => callbacks[0]!());
  expect(box.hasAttribute("data-more-below")).toBe(false);
});
