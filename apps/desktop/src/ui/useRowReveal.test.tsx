import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { useRef, useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { REVEAL_SETTLE_MS, useRowReveal } from "./useRowReveal";

afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); });

const ROW = 40;
/** jsdom lays nothing out: each row reports its place from its order in the page, less the scroll. */
function layout(page: HTMLElement) {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const index = [...page.querySelectorAll(".ui-row")].indexOf(this);
    return { top: index < 0 ? 0 : index * ROW - page.scrollTop } as DOMRect;
  });
}

let addAbove: (count: number) => void = () => {};
function Page({ target }: { target: { id: string; ticket: number } | null }) {
  const page = useRef<HTMLElement>(null);
  const [above, setAbove] = useState(0);
  addAbove = setAbove;
  useRowReveal(target, page);
  return <main ref={page}>
    {Array.from({ length: above }, (_, index) => <div key={index} className="ui-row">Loaded late {index}</div>)}
    <div className="ui-row"><span id="target">Let AI apps read attachment text</span><input type="checkbox" role="switch" aria-labelledby="target" /></div>
  </main>;
}

test("a revealed row stays in view while content above it loads, then lets go", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const { container, rerender } = render(<Page target={null} />);
  const page = container.querySelector("main")!;
  layout(page);
  rerender(<Page target={{ id: "target", ticket: 1 }} />);
  const offset = () => document.getElementById("target")!.closest<HTMLElement>(".ui-row")!.getBoundingClientRect().top;
  expect(offset()).toBe(0);
  expect(document.activeElement?.getAttribute("role")).toBe("switch");
  // An AI apps list read late pushes the row down: it is brought back to the top.
  await act(async () => { addAbove(3); });
  expect(page.scrollTop).toBe(3 * ROW);
  expect(offset()).toBe(0);
  // Once the person scrolls, the page is theirs.
  fireEvent.wheel(page);
  await act(async () => { addAbove(5); });
  expect(offset()).toBe(2 * ROW);
});

test("the hold ends by itself, so a later change never moves the page", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const { container, rerender } = render(<Page target={null} />);
  const page = container.querySelector("main")!;
  layout(page);
  rerender(<Page target={{ id: "target", ticket: 1 }} />);
  await act(() => vi.advanceTimersByTimeAsync(REVEAL_SETTLE_MS));
  await act(async () => { addAbove(2); });
  expect(page.scrollTop).toBe(0);
});
