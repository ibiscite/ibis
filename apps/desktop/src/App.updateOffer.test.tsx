import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { App } from "./App";
import { backend, resetApp } from "./testing/app";

afterEach(async () => { await resetApp(); vi.restoreAllMocks(); });

test("the format chooser blocks update readiness and closing rearms a fresh idle ticket", async () => {
  vi.useFakeTimers();
  vi.spyOn(document, "hasFocus").mockReturnValue(true);
  const reports: (number | null)[] = [];
  let ticket = 0;
  backend({ update_picker_activity: ({ ticket: value }: { ticket: unknown }) => {
    if (value !== null && typeof value !== "number") throw new Error("Expected an update activity ticket");
    reports.push(value);
    return value === null ? ++ticket : value;
  } });
  await act(async () => { render(<App />); });
  const format = screen.getByRole("button", { name: /^Copy as/ });
  await act(async () => { fireEvent.click(format); });
  expect(screen.getByRole("menu")).toBeTruthy();
  reports.length = 0;
  await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
  expect(reports.filter((value) => value !== null)).toEqual([]);
  await act(async () => {
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape", code: "Escape" });
    fireEvent.keyUp(window, { key: "Escape", code: "Escape" });
  });
  expect(screen.queryByRole("menu")).toBeNull();
  // React Aria restores focus on the next frame, which also rearms idle.
  await act(async () => { await vi.advanceTimersByTimeAsync(50); });
  const closeTicket = ticket;
  await act(async () => { await vi.advanceTimersByTimeAsync(1400); });
  expect(reports.filter((value) => value !== null)).toEqual([]);
  await act(async () => { await vi.advanceTimersByTimeAsync(100); });
  expect(reports.filter((value) => value !== null)).toEqual([closeTicket]);
  await act(async () => { fireEvent.click(format); });
  expect(ticket).toBeGreaterThan(closeTicket);
  reports.length = 0;
  await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
  expect(reports.filter((value) => value !== null)).toEqual([]);
});

test.each(["library menu", "source chooser"])("the %s blocks update readiness until it closes", async (layer) => {
  vi.useFakeTimers();
  vi.spyOn(document, "hasFocus").mockReturnValue(true);
  const reports: (number | null)[] = [];
  let ticket = 0;
  backend({ update_picker_activity: ({ ticket: value }: { ticket: unknown }) => {
    if (value !== null && typeof value !== "number") throw new Error("Expected an update activity ticket");
    reports.push(value);
    return value === null ? ++ticket : value;
  } });
  await act(async () => { render(<App />); });
  await act(async () => {
    if (layer === "library menu") fireEvent.click(screen.getByRole("button", { name: "Library" }));
    else {
      fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "o", code: "KeyO", ctrlKey: true, shiftKey: true });
      fireEvent.keyUp(window, { key: "o", code: "KeyO" });
    }
  });
  reports.length = 0;
  await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
  expect(reports.filter((value) => value !== null)).toEqual([]);
  await act(async () => {
    fireEvent.keyDown(document.activeElement!, { key: "Escape", code: "Escape" });
    fireEvent.keyUp(window, { key: "Escape", code: "Escape" });
    await vi.advanceTimersByTimeAsync(50);
  });
  await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
  expect(reports.filter((value) => value !== null)).toHaveLength(1);
});
