import { afterEach, expect, test } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { App } from "./App";

afterEach(async () => {
  cleanup();
  // Event listeners are removed asynchronously after unmount; let that
  // finish before the mocked Tauri internals are cleared.
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
});

type Handler = (args: Record<string, unknown>) => unknown;

/** Mock the Rust commands; returns the list of commands called. */
function backend(overrides: Record<string, Handler> = {}): string[] {
  const calls: string[] = [];
  const defaults: Record<string, Handler> = {
    shortcut_status: () => ({ shortcut: "Cmd+Shift+Space", registered: true, error: null }),
    search: () => [],
    hide_picker: () => null,
  };
  mockIPC(
    (cmd, args) => {
      calls.push(cmd);
      const handler = overrides[cmd] ?? defaults[cmd];
      if (!handler) throw new Error(`unexpected command ${cmd}`);
      return handler((args ?? {}) as Record<string, unknown>);
    },
    { shouldMockEvents: true },
  );
  return calls;
}

test("typing a query shows results from the search command", async () => {
  backend({
    search: ({ query }) =>
      query ? [{ key: "smith2020", title: "Sleep and memory", authors: "Smith", year: 2020, matched: ["title"] }] : [],
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  expect(await screen.findByText("Sleep and memory")).toBeTruthy();
  expect(screen.getByText("smith2020")).toBeTruthy();
});

test("errors from the backend are shown, not swallowed", async () => {
  backend({
    search: () => {
      throw new Error("no library is open");
    },
  });
  render(<App />);
  expect((await screen.findByRole("alert")).textContent).toContain("no library is open");
});

test("Escape hides the picker", async () => {
  const calls = backend();
  render(<App />);
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Escape" });
  await waitFor(() => expect(calls).toContain("hide_picker"));
});

test("a shortcut that could not be registered is explained", async () => {
  backend({
    shortcut_status: () => ({ shortcut: "Cmd+Shift+Space", registered: false, error: "another app may be using it" }),
  });
  render(<App />);
  expect((await screen.findByRole("status")).textContent).toContain("another app may be using it");
});

test("a failed search clears the results of the previous query", async () => {
  backend({
    search: ({ query }) => {
      if (query === "diet") throw new Error("search failed");
      return query ? [{ key: "smith2020", title: "Sleep and memory", authors: "Smith", year: 2020, matched: ["title"] }] : [];
    },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "sleep" } });
  expect(await screen.findByText("Sleep and memory")).toBeTruthy();
  fireEvent.change(box, { target: { value: "diet" } });
  expect((await screen.findByRole("alert")).textContent).toContain("search failed");
  expect(screen.queryByText("Sleep and memory")).toBeNull();
});

test("Quit is available from the picker and from the keyboard", async () => {
  const calls = backend({ quit: () => null });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "Quit Ibis" }));
  await waitFor(() => expect(calls.filter((c) => c === "quit")).toHaveLength(1));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "q", ctrlKey: true });
  await waitFor(() => expect(calls.filter((c) => c === "quit")).toHaveLength(2));
});

test("a rejected shortcut setting is explained even when the default works", async () => {
  backend({
    shortcut_status: () => ({
      shortcut: "Cmd+Shift+Space",
      registered: true,
      error: 'the shortcut "Control+Nonsense" in settings is not valid; using Cmd+Shift+Space',
    }),
  });
  render(<App />);
  expect((await screen.findByRole("status")).textContent).toContain("Control+Nonsense");
});
