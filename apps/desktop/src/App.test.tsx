import { afterEach, expect, test } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";

afterEach(async () => {
  cleanup();
  // Event listeners are removed asynchronously after unmount; let that
  // finish before the mocked Tauri internals are cleared.
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
});

function result(key: string, title: string, source = "refs.bib") {
  return { id: { source, native_key: key }, key, title, authors: "Smith", year: 2020, matched: ["title"] };
}

type Handler = (args: Record<string, unknown>) => unknown;

/** Mock the Rust commands; returns the list of commands called. */
function backend(overrides: Record<string, Handler> = {}): string[] {
  const calls: string[] = [];
  const defaults: Record<string, Handler> = {
    shortcut_status: () => ({ shortcut: "Cmd+Shift+Space", registered: true, error: null }),
    search: () => [],
    hide_picker: () => null,
    copy_format: () => "pandoc",
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
      query ? [result("smith2020", "Sleep and memory")] : [],
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
      return query ? [result("smith2020", "Sleep and memory")] : [];
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

test("the query is searched again when a library finishes opening", async () => {
  let open = false;
  backend({
    search: ({ query }) => {
      if (!open) throw new Error("no library is open");
      return query ? [result("smith2020", "Sleep and memory")] : [];
    },
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "sleep" } });
  expect((await screen.findByRole("alert")).textContent).toContain("no library is open");
  open = true;
  await emit("library:changed", { path: "refs.bib", references: 1, warnings: 0, error: null });
  expect(await screen.findByText("Sleep and memory")).toBeTruthy();
  expect(screen.queryByRole("alert")).toBeNull();
});

test("Enter copies the highlighted result in the remembered format and hides the picker", async () => {
  const copies: unknown[] = [];
  const calls = backend({
    copy_format: () => "typst",
    search: ({ query }) => (query ? [result("a", "First"), result("b", "Second")] : []),
    copy_citation: (args) => {
      copies.push(args);
      return { text: "@b", warnings: [] };
    },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  await waitFor(() => expect((screen.getByLabelText("Copy as") as HTMLSelectElement).value).toBe("typst"));
  fireEvent.change(box, { target: { value: "s" } });
  await screen.findByText("Second");
  fireEvent.keyDown(box, { key: "ArrowDown" });
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(calls).toContain("hide_picker"));
  expect(copies).toEqual([{ ids: [{ source: "refs.bib", native_key: "b" }], format: "typst" }]);
  expect(screen.getByRole("status").textContent).toContain("Copied @b");
});

test("the selection is kept by record identity across queries and copied in order", async () => {
  const copies: unknown[] = [];
  backend({
    search: ({ query }) =>
      query === "one" ? [result("same", "One", "a.bib")] : query === "two" ? [result("same", "Two", "b.bib")] : [],
    copy_citation: (args) => {
      copies.push(args);
      return { text: "[@same; @same]", warnings: [] };
    },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "one" } });
  await screen.findByText("One");
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  fireEvent.change(box, { target: { value: "two" } });
  fireEvent.click(await screen.findByText("Two"));
  expect(screen.getAllByRole("button", { name: /Remove same/ })).toHaveLength(2);
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(copies[0]).toEqual({
    ids: [
      { source: "a.bib", native_key: "same" },
      { source: "b.bib", native_key: "same" },
    ],
    format: "pandoc",
  });
});

test("warnings keep the picker open and failures say nothing was copied", async () => {
  let fail = false;
  const calls = backend({
    search: ({ query }) => (query ? [result("a", "First")] : []),
    copy_citation: () => {
      if (fail) throw new Error("the clipboard could not be written");
      return { text: "a", warnings: ["bare keys cannot carry page numbers"] };
    },
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "f" } });
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Enter" });
  expect((await screen.findByLabelText("Copy warnings")).textContent).toContain("page numbers");
  expect(calls).not.toContain("hide_picker");
  fail = true;
  fireEvent.keyDown(box, { key: "Enter" });
  expect((await screen.findByRole("alert")).textContent).toContain("Nothing was copied");
});

test("Enter while composing text with an input method copies nothing", async () => {
  const calls = backend({ search: ({ query }) => (query ? [result("a", "First")] : []) });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "f" } });
  await screen.findByText("First");
  fireEvent.keyDown(box, { key: "Enter", isComposing: true });
  fireEvent.keyDown(box, { key: "Enter", keyCode: 229 });
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(calls).not.toContain("copy_citation");
});

test("Enter never copies a result left over from the previous query", async () => {
  let release: (value: unknown) => void = () => undefined;
  const calls = backend({
    search: ({ query }) =>
      query === "old" ? [result("old", "Old result")] : query ? new Promise((resolve) => (release = resolve)) : [],
    copy_citation: () => ({ text: "x", warnings: [] }),
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "old" } });
  await screen.findByText("Old result");
  fireEvent.change(box, { target: { value: "new" } });
  fireEvent.keyDown(box, { key: "Enter" });
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(calls).not.toContain("copy_citation");
  release([result("new", "New result")]);
  await screen.findByText("New result");
});
