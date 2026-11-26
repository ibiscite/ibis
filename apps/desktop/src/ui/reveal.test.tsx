import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { useMemo, useRef, useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { reveal, useReveal } from "./reveal";
import { settingTarget } from "./settingFocus";

afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); });

const ROW = 40, TITLE = 48;
/** jsdom lays nothing out: the page is 400 px tall under a sticky title of
 * TITLE px, and each row reports its place from its order, less the scroll. */
function layout(page: HTMLElement) {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this === page) return new DOMRect(0, 0, 600, 400);
    if (this.matches("h1")) return new DOMRect(0, 0, 600, TITLE);
    const index = [...page.querySelectorAll(".ui-row")].indexOf(this);
    return new DOMRect(0, index < 0 ? 0 : TITLE + index * ROW - page.scrollTop, 600, ROW);
  });
}

let addAbove: (count: number) => void = () => {};
let setBusy: (busy: boolean) => void = () => {};
function Page({ target, busy: startBusy = false, before = 0 }: { target: { id: string; ticket: number } | null; busy?: boolean; before?: number }) {
  const page = useRef<HTMLElement>(null);
  const [above, setAbove] = useState(before);
  const [busy, changeBusy] = useState(startBusy);
  addAbove = setAbove;
  setBusy = changeBusy;
  useReveal(page, useMemo(() => target && settingTarget(target.id), [target]));
  return <main ref={page}>
    <h1 style={{ position: "sticky" }}>Look</h1>
    {Array.from({ length: above }, (_, index) => <div key={index} className="ui-row">Loaded late {index}</div>)}
    <div className="ui-row" aria-busy={busy || undefined}><span id="target">Window size</span><input type="radio" aria-labelledby="target" defaultChecked /></div>
  </main>;
}
const offset = () => document.getElementById("target")!.closest<HTMLElement>(".ui-row")!.getBoundingClientRect().top;

test("a revealed row lands under the sticky title and stays there while content above it loads, until the person scrolls", async () => {
  const { container, rerender } = render(<Page target={null} />);
  const page = container.querySelector("main")!;
  layout(page);
  rerender(<Page target={{ id: "target", ticket: 1 }} />);
  expect(offset()).toBe(TITLE);
  expect(document.activeElement?.getAttribute("type")).toBe("radio");
  // A list read late pushes the row down: it is brought back where it landed.
  await act(async () => { addAbove(3); });
  expect(page.scrollTop).toBe(3 * ROW);
  expect(offset()).toBe(TITLE);
  // Once the person scrolls, the page is theirs.
  fireEvent.wheel(page);
  await act(async () => { addAbove(5); });
  expect(offset()).toBe(TITLE + 2 * ROW);
});

// The case found in review: Settings at 920 × 560, "window size", Enter, and
// 1.7 s later a block twice the window's height lands above it. A hold that
// ended on a timer (1.5 s before) let the focused control fall far out of the
// page; the hold now lasts until the person moves on, as scroll anchoring does.
test("content that arrives long after the jump still leaves the focused control in view", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const { container, rerender } = render(<Page target={null} />);
  const page = container.querySelector("main")!;
  layout(page);
  rerender(<Page target={{ id: "target", ticket: 1 }} />);
  await act(() => vi.advanceTimersByTimeAsync(1700));
  await act(async () => { addAbove(28); });
  expect(offset()).toBe(TITLE);
  expect(document.activeElement?.closest(".ui-row")).toBe(document.getElementById("target")!.closest(".ui-row"));
});

test("a reveal waits until its row is ready, and typing or a newer reveal ends it", async () => {
  const { container, rerender } = render(<Page target={null} busy />);
  const page = container.querySelector("main")!;
  layout(page);
  rerender(<Page target={{ id: "target", ticket: 1 }} busy />);
  // Still loading: nothing moves and the keyboard stays where it was.
  expect(document.activeElement).toBe(document.body);
  await act(async () => { setBusy(false); });
  expect(document.activeElement?.getAttribute("type")).toBe("radio");
  fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
  await act(async () => { addAbove(2); });
  expect(offset()).toBe(TITLE + 2 * ROW);
  // A newer reveal on the same scroll owner replaces the last.
  const first = reveal(page, { find: () => page.querySelector<HTMLElement>(".ui-row") });
  expect(page.querySelector<HTMLElement>(".ui-row")!.getBoundingClientRect().top).toBe(TITLE);
  reveal(page, settingTarget("target"));
  expect(offset()).toBe(TITLE);
  first();
  await act(async () => { addAbove(4); });
  expect(offset()).toBe(TITLE);
});

// Found in review of the first version: the Enter that asks for the jump is
// still bubbling when the reveal starts, and reached the listener that ends it.
test("the key that asks for a reveal does not end it", async () => {
  const { container } = render(<Page target={null} />);
  const page = container.querySelector("main")!;
  layout(page);
  const search = document.createElement("input");
  document.body.append(search);
  search.addEventListener("keydown", () => { reveal(page, settingTarget("target")); });
  try {
    fireEvent.keyDown(search, { key: "Enter" });
    await act(async () => { addAbove(12); });
    expect(offset()).toBe(TITLE);
    // The next key is the person's.
    fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    await act(async () => { addAbove(14); });
    expect(offset()).toBe(TITLE + 2 * ROW);
  } finally { search.remove(); }
});

// Found in review too: scrolling with no wheel, key or pointer (a screen reader,
// a scroll bar, a script) left the hold on, and the next change snapped back.
test("scrolling by any other means ends the hold; the reveal's own scrolling does not", async () => {
  const { container, rerender } = render(<Page target={null} before={10} />);
  const page = container.querySelector("main")!;
  layout(page);
  rerender(<Page target={{ id: "target", ticket: 1 }} before={10} />);
  expect(page.scrollTop).toBe(10 * ROW);
  // Its own write fires a scroll too, which leaves it holding.
  fireEvent.scroll(page);
  await act(async () => { addAbove(11); });
  expect(offset()).toBe(TITLE);
  page.scrollTop -= 160;
  fireEvent.scroll(page);
  await act(async () => { addAbove(12); });
  expect(offset()).toBe(TITLE + 160 + ROW);
});
