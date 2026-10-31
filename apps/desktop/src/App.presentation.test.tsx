import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, resetApp, result } from "./testing/app";

afterEach(resetApp);

test("Escape carries the native startup presentation after a webview reload", async () => {
  const hide = vi.fn(() => true);
  backend({ picker_presentation: () => 41, hide_picker: hide, search: () => [result("a", "Invented reference")] });
  await act(async () => { render(<App />); });
  await screen.findByText("Invented reference");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Escape" });
  await waitFor(() => expect(hide).toHaveBeenCalledExactlyOnceWith({ presentation: 41 }));
});

test.each(["late", "failed"])("a %s startup presentation read cannot replace reopened B's owner", async (outcome) => {
  let finish!: (value: number) => void, fail!: (reason: Error) => void;
  const reading = new Promise<number>((resolve, reject) => { finish = resolve; fail = reject; });
  const read = vi.fn(() => reading), hide = vi.fn(() => true);
  backend({ picker_presentation: read, hide_picker: hide, search: () => [result("a", "Invented reference")] });
  render(<App />);
  await waitFor(() => expect(read).toHaveBeenCalledOnce());
  await act(async () => {
    await emit("picker:hidden");
    await emit("picker:shown", 43);
    if (outcome === "late") finish(41);
    else fail(new Error("invented I/O failure"));
  });
  await screen.findByText("Invented reference");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Escape" });
  await waitFor(() => expect(hide).toHaveBeenCalledExactlyOnceWith({ presentation: 43 }));
});
