import { afterEach, expect, test, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { App } from "../../App";
import { backend, resetApp } from "../../testing/app";

afterEach(resetApp);

test("dragging the search bar reports a move of the shown picker until the pointer comes back unpressed", async () => {
  const moving = vi.fn(() => null);
  backend({ picker_moving: moving, picker_presentation: () => 7 });
  render(<App />);
  const header = (await screen.findByLabelText("Search references")).closest("header")!;
  await vi.waitFor(() => {
    fireEvent.pointerDown(header, { button: 0, clientX: 100, clientY: 20 });
    expect(moving).toHaveBeenCalledWith(expect.objectContaining({ presentation: 7, moving: true }));
  });
  // Movement still pressed is the drag itself; the move goes on.
  fireEvent.pointerMove(window, { buttons: 1 });
  expect(moving).not.toHaveBeenCalledWith(expect.objectContaining({ moving: false }));
  // The compositor gives the pointer back with no button held.
  fireEvent.pointerMove(window, { buttons: 0 });
  expect(moving).toHaveBeenLastCalledWith(expect.objectContaining({ presentation: 7, moving: false }));
  const calls = moving.mock.calls.length;
  fireEvent.pointerMove(window, { buttons: 0 });
  expect(moving).toHaveBeenCalledTimes(calls);
});

test("a press inside the picker's content is not a move", async () => {
  const moving = vi.fn(() => null);
  backend({ picker_moving: moving, picker_presentation: () => 7 });
  render(<App />);
  const search = await screen.findByLabelText("Search references");
  fireEvent.pointerDown(search.closest("main")!.querySelector("footer")!, { button: 0, clientX: 100, clientY: 100 });
  fireEvent.pointerDown(search, { button: 0, clientX: 100, clientY: 20 });
  fireEvent.pointerDown(search.closest("header")!, { button: 2, clientX: 100, clientY: 20 });
  expect(moving).not.toHaveBeenCalled();
});
