import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, resetApp, result } from "./testing/app";

afterEach(resetApp);

test("Enter waits for a reloaded picker's native identity before copying and pasting", async () => {
  let finish!: (value: number) => void;
  const reading = new Promise<number>((resolve) => { finish = resolve; });
  const read = vi.fn(() => reading), copy = vi.fn(() => ({ text: "invented citation", warnings: [] }));
  let visible = true;
  const hide = vi.fn(({ presentation }) => { if (presentation !== 41) return false; visible = false; return true; });
  const paste = vi.fn(() => ({ outcome: "pasted", app: "Invented Writer" }));
  backend({ picker_presentation: read, copy_file_citation: copy, hide_picker: hide, show_copied_notice: paste,
    search: () => [result("a", "Invented reference")] });
  render(<App />);
  await screen.findByText("Invented reference");
  await waitFor(() => expect(read).toHaveBeenCalledOnce());
  await screen.findByText("@a");
  await act(async () => { fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" }); });
  expect(copy).not.toHaveBeenCalled();
  await act(async () => { finish(41); });
  await waitFor(() => expect(paste).toHaveBeenCalledOnce());
  expect(hide).toHaveBeenCalledExactlyOnceWith({ presentation: 41 });
  expect(copy).toHaveBeenCalledOnce();
  expect(visible).toBe(false);
});

test("Enter recovers a failed initialization without another show event", async () => {
  const read = vi.fn().mockRejectedValueOnce(new Error("invented read failure")).mockResolvedValue(41);
  const hide = vi.fn(({ presentation }) => presentation === 41);
  const paste = vi.fn(() => ({ outcome: "pasted", app: "Invented Writer" }));
  backend({ picker_presentation: read, hide_picker: hide, show_copied_notice: paste,
    search: () => [result("a", "Invented reference")] });
  render(<App />);
  await screen.findByText("@a");
  await act(async () => { fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" }); });
  await waitFor(() => expect(paste).toHaveBeenCalledOnce());
  expect(read).toHaveBeenCalledTimes(2);
  expect(hide).toHaveBeenCalledExactlyOnceWith({ presentation: 41 });
});

test("a failed read with Enter already queued retries once and pastes once", async () => {
  let fail!: (error: Error) => void;
  const reading = new Promise<number>((_resolve, reject) => { fail = reject; });
  const read = vi.fn().mockReturnValueOnce(reading).mockResolvedValue(41);
  const paste = vi.fn(() => ({ outcome: "pasted", app: "Invented Writer" }));
  const copy = vi.fn(() => ({ text: "invented citation", warnings: [] }));
  backend({ picker_presentation: read, copy_file_citation: copy, hide_picker: ({ presentation }) => presentation === 41,
    show_copied_notice: paste, search: () => [result("a", "Invented reference")] });
  render(<App />);
  await screen.findByText("@a");
  await act(async () => {
    fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
    fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  });
  expect(copy).not.toHaveBeenCalled();
  expect(read).toHaveBeenCalledOnce();
  await act(async () => { fail(new Error("invented read failure")); });
  await waitFor(() => expect(paste).toHaveBeenCalledOnce());
  expect(copy).toHaveBeenCalledOnce();
  expect(read).toHaveBeenCalledTimes(2);
});

test("a late initialization cannot copy A after close and reopen B", async () => {
  let finish!: (value: number) => void;
  const reading = new Promise<number>((resolve) => { finish = resolve; });
  const read = vi.fn(() => reading), copy = vi.fn(() => ({ text: "invented citation", warnings: [] }));
  const hide = vi.fn(({ presentation }) => presentation === 43);
  backend({ picker_presentation: read, copy_file_citation: copy, hide_picker: hide,
    search: () => [result("a", "Invented reference")] });
  render(<App />);
  await screen.findByText("@a");
  await act(async () => {
    fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
    await emit("picker:hidden");
    await emit("picker:shown", 43);
    finish(41);
  });
  expect(copy).not.toHaveBeenCalled();
  expect(hide).not.toHaveBeenCalled();
  await screen.findByText("@a");
  await act(async () => { fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" }); });
  await waitFor(() => expect(hide).toHaveBeenCalledExactlyOnceWith({ presentation: 43 }));
});

test("a failed visibility read cannot discard a successful presentation initialization", async () => {
  const hide = vi.fn(() => true);
  backend({ "plugin:window|is_visible": () => Promise.reject(new Error("invented visibility failure")),
    picker_presentation: () => 41, hide_picker: hide, search: () => [result("a", "Invented reference")] });
  await act(async () => { render(<App />); });
  await screen.findByText("Invented reference");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Escape" });
  await waitFor(() => expect(hide).toHaveBeenCalledExactlyOnceWith({ presentation: 41 }));
});

test("persistent initialization failures stop retrying until another Enter", async () => {
  const read = vi.fn().mockRejectedValue(new Error("invented read failure"));
  const copy = vi.fn(() => ({ text: "invented citation", warnings: [] }));
  const paste = vi.fn(() => ({ outcome: "pasted", app: "Invented Writer" }));
  backend({ picker_presentation: read, copy_file_citation: copy, hide_picker: ({ presentation }) => presentation === 41,
    show_copied_notice: paste, search: () => [result("a", "Invented reference")] });
  render(<App />);
  await screen.findByText("@a");
  await act(async () => { fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" }); });
  expect(read).toHaveBeenCalledTimes(3); // initial read, action read, one recovery
  expect(copy).not.toHaveBeenCalled();
  expect(paste).not.toHaveBeenCalled();
  read.mockResolvedValue(41);
  await act(async () => { fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" }); });
  await waitFor(() => expect(paste).toHaveBeenCalledOnce());
  expect(read).toHaveBeenCalledTimes(4);
  expect(copy).toHaveBeenCalledOnce();
});

test("repeated Escape keeps its known owner until the native hide completes", async () => {
  const hide = vi.fn(() => new Promise<boolean>(() => {}));
  backend({ picker_presentation: () => 41, hide_picker: hide, search: () => [result("a", "Invented reference")] });
  await act(async () => { render(<App />); });
  await screen.findByText("Invented reference");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Escape" });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Escape" });
  expect(hide.mock.calls).toEqual([[{ presentation: 41 }], [{ presentation: 41 }]]);
});

test.each(["pending", "failed"])("Escape hides a reloaded picker while initialization is %s", async (outcome) => {
  const read = vi.fn(() => outcome === "pending" ? new Promise<number>(() => {}) : Promise.reject(new Error("invented read failure")));
  let visible = true;
  const hide = vi.fn(({ presentation }) => {
    if (presentation !== null && presentation !== 41) return false;
    visible = false;
    return true;
  });
  backend({ picker_presentation: read, hide_picker: hide, search: () => [result("a", "Invented reference")] });
  render(<App />);
  await screen.findByText("Invented reference");
  await waitFor(() => expect(read).toHaveBeenCalledOnce());
  await act(async () => { fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Escape" }); });
  expect(hide).toHaveBeenCalledExactlyOnceWith({ presentation: null });
  expect(visible).toBe(false);
});

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
