import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { usePickerMove } from "../features/picker/usePickerMove";
import type { usePickerSession } from "../features/picker/usePickerSession";
import { Band } from "./Band";
import { Button } from "./Button";
import { IbisMark } from "./IbisMark";
import { SearchBar, SearchField } from "./SearchBar";
import { startsWindowDrag } from "./windowDrag";

afterEach(cleanup);

type Session = ReturnType<typeof usePickerSession>["session"];
const session = { getSnapshot: () => ({ presentation: 1 }) } as unknown as Session;

/** The picker's search bar as the picker draws it, with the window's move hook. */
function Picker({ pickerMoving, onResetPosition }: { pickerMoving: () => Promise<void>; onResetPosition: () => void }) {
  usePickerMove({ pickerMoving }, session);
  return <SearchBar title="Drag to move" mark={<IbisMark className="search-mark" size="search" />} onResetPosition={onResetPosition}>
    <SearchField id="search" label="Search references" value="" onChange={() => undefined} />
    <Button variant="quiet">Invented format</Button>
  </SearchBar>;
}

function setup() {
  const pickerMoving = vi.fn(async () => undefined);
  const onResetPosition = vi.fn();
  // Stands in for Tauri's drag script, which listens on the document after the app.
  const script = vi.fn();
  document.addEventListener("mousedown", script);
  document.addEventListener("mouseup", script);
  const { container } = render(<Picker pickerMoving={pickerMoving} onResetPosition={onResetPosition} />);
  const bar = container.querySelector("header")!;
  const bird = bar.querySelector('[data-band-slot="lead"]')!;
  const dispose = () => { document.removeEventListener("mousedown", script); document.removeEventListener("mouseup", script); };
  return { pickerMoving, onResetPosition, script, bar, bird, dispose };
}
// Away from the window's edges, where any press resizes.
const press = (target: Element) => { fireEvent.pointerDown(target, { button: 0, clientX: 300, clientY: 20 }); fireEvent.pointerUp(target, { button: 0, clientX: 300, clientY: 20 }); };
const doubleClick = (target: Element) => {
  fireEvent.mouseDown(target, { button: 0, detail: 1 }); fireEvent.mouseUp(target, { button: 0, detail: 1 });
  fireEvent.mouseDown(target, { button: 0, detail: 2 }); fireEvent.mouseUp(target, { button: 0, detail: 2 });
};

test("the bird and the search bar's empty room move the window; the field and its controls never do", () => {
  const { pickerMoving, bar, bird, dispose } = setup();
  const input = screen.getByRole("searchbox", { name: "Search references" });
  const format = screen.getByRole("button", { name: "Invented format" });
  // The bird is drawn with no pointer events, so a press lands on its slot.
  expect(bird.querySelector("svg")).not.toBeNull();
  for (const target of [bird, bar]) {
    expect(startsWindowDrag(target)).toBe(true);
    pickerMoving.mockClear();
    press(target);
    expect(pickerMoving).toHaveBeenCalledWith(1, expect.any(Number), true);
  }
  for (const target of [input, format, input.parentElement!]) {
    expect(startsWindowDrag(target)).toBe(false);
    pickerMoving.mockClear();
    press(target);
    expect(pickerMoving).not.toHaveBeenCalledWith(1, expect.any(Number), true);
  }
  expect(input.hasAttribute("data-tauri-drag-region")).toBe(false);
  dispose();
});

test("double-clicking the bird or the empty bar resets the position instead of zooming; the field only edits", () => {
  const { onResetPosition, script, bar, bird, dispose } = setup();
  for (const target of [bird, bar]) {
    onResetPosition.mockClear(); script.mockClear();
    doubleClick(target);
    expect(onResetPosition).toHaveBeenCalledTimes(1);
    // The first click reaches the drag script (a move); the second never does (a zoom).
    expect(script.mock.calls.map(([event]) => (event as MouseEvent).detail)).toEqual([1, 1]);
  }
  onResetPosition.mockClear();
  doubleClick(screen.getByRole("searchbox", { name: "Search references" }));
  expect(onResetPosition).not.toHaveBeenCalled();
  dispose();
});

test("every drag band marks its own slots, and a band that does not drag marks nothing", () => {
  const { container } = render(<>
    <Band drag icon={<span>Invented glyph</span>} trailing={<button type="button">Invented action</button>}>Invented words</Band>
    <Band icon={<span>Plain glyph</span>} trailing={<span>Plain end</span>}>Plain words</Band>
  </>);
  const [drag, plain] = [...container.querySelectorAll(".ui-band")] as [Element, Element];
  expect([drag, ...drag.querySelectorAll("[data-band-slot]")].every((node) => startsWindowDrag(node))).toBe(true);
  expect(startsWindowDrag(screen.getByRole("button", { name: "Invented action" }))).toBe(false);
  expect(plain.querySelector("[data-tauri-drag-region]") ?? plain.getAttribute("data-tauri-drag-region")).toBeNull();
});

test("the window's drag rule is Tauri's: a control blocks, false blocks, deep reaches inside", () => {
  const { container } = render(<div data-tauri-drag-region="deep"><span>Deep words</span>
    <div role="button">Invented control</div><div data-tauri-drag-region="false"><span>Kept still</span></div></div>);
  expect(startsWindowDrag(screen.getByText("Deep words"))).toBe(true);
  expect(startsWindowDrag(container.firstElementChild)).toBe(true);
  expect(startsWindowDrag(screen.getByText("Invented control"))).toBe(false);
  expect(startsWindowDrag(screen.getByText("Kept still"))).toBe(false);
});
