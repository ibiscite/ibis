import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { usePickerMove } from "../features/picker/usePickerMove";
import type { usePickerSession } from "../features/picker/usePickerSession";
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
