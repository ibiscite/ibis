import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { SettingsApp } from "./SettingsApp";
import { demoApi } from "./demo";

afterEach(() => vi.useRealTimers());
const details = { fun: true, fact: null, birthday: false, library: null, revision: 1, status: null, peck: false, anniversary: true, build: "0.0.0+invented · invented-build · 2026-11-01" } as const;

test("About reveals full build with Option and uses the same copy path, including failed I/O", async () => {
  const write = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: write } });
  const api = { ...demoApi(), delightView: vi.fn().mockResolvedValue(details) };
  const mounted = render(<SettingsApp api={api} initialPage="about" />);
  expect(await screen.findByText("One year with Ibis")).toBeTruthy();
  expect(screen.queryByText(/invented-build/)).toBeNull();
  fireEvent.keyDown(window, { key: "Alt", altKey: true });
  const row = await screen.findByRole("button", { name: `Ibis ${details.build}` });
  fireEvent.click(row);
  await waitFor(() => expect(write).toHaveBeenLastCalledWith(`Ibis ${details.build}`));
  fireEvent.keyUp(window, { key: "Alt" });
  expect(screen.queryByRole("button", { name: `Ibis ${details.build}` })).toBeNull();
  write.mockRejectedValueOnce(new Error("invented clipboard failure"));
  fireEvent.click(screen.getByRole("button", { name: "Ibis 0.2.1" }));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "The version could not be copied.");
  expect(screen.getByRole("button", { name: "Copy Version" })).toBeTruthy();
  mounted.unmount();
  render(<SettingsApp api={{ ...api, delightView: vi.fn().mockResolvedValue({ ...details, anniversary: false }) }} initialPage="about" />);
  await screen.findByRole("button", { name: "Ibis 0.2.1" });
  expect(screen.queryByText("One year with Ibis")).toBeNull();
});

test("a search blink cannot replay when About opens later from navigation", async () => {
  const api = { ...demoApi(), delightView: vi.fn().mockResolvedValue(details) };
  const { container } = await act(async () => render(<SettingsApp api={api} initialPage="general" />));
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "ibis" } });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "About Ibis" })));
  expect(container.querySelector('[data-motion="blink"]')).not.toBeNull();
  fireEvent.click(screen.getByRole("option", { name: "General" }));
  fireEvent.click(screen.getByRole("option", { name: "About Ibis" }));
  expect(container.querySelector('[data-motion="blink"]')).toBeNull();
});

test("the anniversary expires at local midnight and a delayed older read cannot restore it", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date(2027, 10, 1, 23, 59, 59));
  const api = { ...demoApi(), delightView: vi.fn().mockResolvedValueOnce(details).mockResolvedValue({ ...details, anniversary: false }) };
  await act(async () => { render(<SettingsApp api={api} initialPage="about" />); });
  expect(screen.getByText("One year with Ibis")).toBeTruthy();
  await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  expect(screen.queryByText("One year with Ibis")).toBeNull();
});

test("ibis search offers About, blinks on opening, and a stale About read cannot leak to General", async () => {
  let complete!: (value: typeof details) => void;
  const api = { ...demoApi(), delightView: vi.fn(() => new Promise<typeof details>((resolve) => { complete = resolve; })) };
  const { container } = await act(async () => render(<SettingsApp api={api} initialPage="general" />));
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "ibis" } });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "About Ibis" })));
  expect(container.querySelector('[data-motion="blink"]')).toBeNull();
  fireEvent.click(screen.getByRole("option", { name: "General" }));
  await act(async () => { complete(details); });
  expect(screen.queryByText("One year with Ibis")).toBeNull();
});
