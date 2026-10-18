import { afterEach, expect, test } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { App } from "./App";

afterEach(() => {
  cleanup();
  clearMocks();
});

test("typing a query shows results from the search command", async () => {
  mockIPC((cmd, args) => {
    if (cmd === "search") {
      const query = (args as { query: string }).query;
      return query
        ? [{ key: "smith2020", title: "Sleep and memory", authors: "Smith", year: 2020, matched: ["title"] }]
        : [];
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  expect(await screen.findByText("Sleep and memory")).toBeTruthy();
  expect(screen.getByText("smith2020")).toBeTruthy();
});

test("errors from the backend are shown, not swallowed", async () => {
  mockIPC(() => {
    throw new Error("no library is open");
  });
  render(<App />);
  expect((await screen.findByRole("alert")).textContent).toContain("no library is open");
});
