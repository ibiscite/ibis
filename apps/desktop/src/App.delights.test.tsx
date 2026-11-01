import { act, fireEvent, render, screen } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { backend, resetApp } from "./testing/app";

beforeEach(() => vi.useFakeTimers());
afterEach(async () => { document.documentElement.style.removeProperty("zoom"); await resetApp(); });
const view = (status: string | null, peck = false) => ({ status, peck, fun: true, fact: null, birthday: false, library: "invented.bib", revision: 1, anniversary: false, build: "0.0.0 · invented · 2026-11-01" });

test("a claimed tip shares the footer with library status and never copies or changes focus", async () => {
  const calls = backend({ delight_view: () => view("paste-tip") });
  await act(async () => { render(<App />); });
  expect(screen.getByText("⇧⌘P keeps Ibis open").closest("footer")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Library" })).toBeTruthy();
  expect(document.activeElement).toBe(screen.getByLabelText("Search references"));
  expect(calls).not.toContain("copy_file_citation");
  const input = screen.getByLabelText("Search references");
  fireEvent.keyDown(input, { key: "Enter" });
  expect(screen.getByText("Choose a reference to paste.")).toBeTruthy();
  await act(async () => fireEvent.change(input, { target: { value: "invented" } }));
  expect(screen.queryByText("⇧⌘P keeps Ibis open")).toBeNull();
});

test("empty Enter explains refusal and zoom reports for one second, without copying", async () => {
  const calls = backend({ delight_view: () => view(null) });
  const { container } = await act(async () => render(<App />));
  const input = screen.getByLabelText("Search references");
  fireEvent.keyDown(input, { key: "Enter" });
  expect(screen.getByText("Choose a reference to paste.")).toBeTruthy();
  expect(container.querySelector("[data-empty-action]")).not.toBeNull();
  act(() => vi.advanceTimersByTime(240));
  expect(container.querySelector("[data-empty-action]")).toBeNull();
  fireEvent.keyDown(input, { key: "+", metaKey: true });
  expect(screen.getByText("110%")).toBeTruthy();
  expect(document.documentElement.style.zoom).toBe("1.1");
  act(() => vi.advanceTimersByTime(1000));
  expect(screen.queryByText("110%")).toBeNull();
  fireEvent.keyDown(input, { key: "-", metaKey: true });
  expect(screen.getByText("100%")).toBeTruthy();
  expect(document.activeElement).toBe(input);
  expect(calls).not.toContain("copy_file_citation");
});

test("delayed claims and closed windows cannot replace newer visible feedback", async () => {
  const pending: ((value: ReturnType<typeof view>) => void)[] = [];
  backend({ delight_view: () => new Promise((resolve) => pending.push(resolve)) });
  const mounted = await act(async () => render(<App />));
  await act(async () => { await emit("picker:shown", 2); });
  await act(async () => { pending.at(-1)!(view("library1000")); });
  expect(screen.getByText("1,000 references in invented.bib. A solid library.")).toBeTruthy();
  await act(async () => { pending[0]!(view("paste-tip")); });
  expect(screen.queryByText("⇧⌘P keeps Ibis open")).toBeNull();
  mounted.unmount();
  backend({ delight_view: () => view(null) });
  await act(async () => { render(<App />); });
  expect(screen.queryByText("1,000 references in invented.bib. A solid library.")).toBeNull();
});

test("unavailable feedback persistence leaves search and empty refusal working", async () => {
  backend({ delight_view: () => { throw new Error("invented write failure"); } });
  await act(async () => { render(<App />); });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(screen.getByText("Choose a reference to paste.")).toBeTruthy();
  expect(screen.queryByText(/invented write failure/)).toBeNull();
});

test.each([
  ["library100", "100 references in invented.bib. A good start."],
  ["library1000", "1,000 references in invented.bib. A solid library."],
  ["library10000", "10,000 references in invented.bib. A remarkable library."],
])("%s renders the Rust answer beside the loaded library", async (status, message) => {
  backend({ delight_view: () => view(status) });
  await act(async () => { render(<App />); });
  expect(screen.getByText(message).closest("footer")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Library" })).toBeTruthy();
});

test("the welcome peck expires and cannot replay when empty results remount or the picker reopens", async () => {
  let claimed = false;
  backend({ delight_view: () => { const next = view(null, !claimed); claimed = true; return next; } });
  const { container } = await act(async () => render(<App />));
  expect(container.querySelector('[data-motion="peck"]')).not.toBeNull();
  const input = screen.getByLabelText("Search references");
  await act(async () => fireEvent.change(input, { target: { value: "invented" } }));
  await act(async () => fireEvent.change(input, { target: { value: "" } }));
  expect(container.querySelector('[data-motion="peck"]')).toBeNull();
  act(() => vi.advanceTimersByTime(240));
  expect(container.querySelector('[data-motion="peck"]')).toBeNull();
  await act(async () => fireEvent.change(input, { target: { value: "invented" } }));
  await act(async () => fireEvent.change(input, { target: { value: "" } }));
  expect(container.querySelector('[data-motion="peck"]')).toBeNull();
  await act(async () => { await emit("picker:hidden"); await emit("picker:shown", 2); });
  expect(container.querySelector('[data-motion="peck"]')).toBeNull();
});
