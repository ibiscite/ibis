import { afterEach, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { App } from "./App";
import { backend, resetApp, result } from "./testing/app";

afterEach(resetApp);

test("repeated Escape keeps the snapshot's owner until the native hide completes", async () => {
  const hide = vi.fn(() => new Promise<boolean>(() => {}));
  backend({ hide_picker: hide, search: () => [result("a", "Invented reference")] });
  render(<App />);
  await screen.findByText("Invented reference");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Escape" });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Escape" });
  expect(hide.mock.calls).toEqual([[{ presentation: 0 }], [{ presentation: 0 }]]);
});

test("Escape carries the snapshot's presentation after startup", async () => {
  const hide = vi.fn(() => true);
  backend({ hide_picker: hide, search: () => [result("a", "Invented reference")] });
  render(<App />);
  await screen.findByText("Invented reference");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Escape" });
  await waitFor(() => expect(hide).toHaveBeenCalledExactlyOnceWith({ presentation: 0 }));
});
