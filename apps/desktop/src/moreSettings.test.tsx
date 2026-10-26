import { afterEach, expect, test } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import * as api from "./api";
import { backend, chooseFromMore, resetApp } from "./testing/app";

afterEach(resetApp);

test("More shows whether Ibis starts at login as the system reports it, and follows changes made elsewhere", async () => {
  let system = true;
  const calls = backend({
    login_item: () => ({ name: "Ibis", enabled: system, problem: null }),
    set_login_item: ({ enabled }) => { system = Boolean(enabled); return { enabled: system, problem: null }; },
  });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  const item = await screen.findByRole("menuitemcheckbox", { name: "Start on Login" });
  await waitFor(() => expect(item.getAttribute("aria-checked")).toBe("true"));
  fireEvent.click(item);
  await waitFor(() => expect(calls).toContain("set_login_item"));
  expect(system).toBe(false);
  // Removed or added again in System Settings while Ibis was hidden.
  system = true;
  await act(async () => { await emit("picker:shown"); });
  if (!screen.queryByRole("menu")) fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  await waitFor(() => expect(screen.getByRole("menuitemcheckbox", { name: "Start on Login" }).getAttribute("aria-checked")).toBe("true"));
  expect(screen.queryByRole("alert")).toBeNull();
});

test("a refused login item is explained kindly and never shown as on", async () => {
  backend({ set_login_item: () => ({ name: "Ibis", enabled: false, problem: "refused" }) });
  render(<App />);
  await chooseFromMore("Start on Login", "menuitemcheckbox");
  const alert = await screen.findByText(/^The system did not let Ibis change this\. You can change it in /);
  expect(alert.getAttribute("role")).toBe("alert");
  if (!screen.queryByRole("menu")) fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  expect((await screen.findByRole("menuitemcheckbox", { name: "Start on Login" })).getAttribute("aria-checked")).toBe("false");
});

test("a failed login item command restores the previous state and says so", async () => {
  backend({ set_login_item: () => { throw new Error("ipc"); } });
  render(<App />);
  await chooseFromMore("Start on Login", "menuitemcheckbox");
  expect(await screen.findByText("Ibis could not change whether it starts when you log in. Try again.")).toBeTruthy();
  if (!screen.queryByRole("menu")) fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  expect((await screen.findByRole("menuitemcheckbox", { name: "Start on Login" })).getAttribute("aria-checked")).toBe("false");
});

test("systems without login items, and pickers without the commands, offer no choice", async () => {
  backend({ login_item: () => ({ name: "Ibis", enabled: false, problem: "unsupported" }) });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  await screen.findByRole("menuitemcheckbox", { name: "Larger text" });
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  expect(screen.queryByRole("menuitemcheckbox", { name: "Start on Login" })).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
  cleanup();
  const { loginItem: _read, setLoginItem: _set, ...withoutLoginItem } = api;
  backend();
  render(<App api={withoutLoginItem} />);
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  await screen.findByRole("menuitemcheckbox", { name: "Larger text" });
  expect(screen.queryByRole("menuitemcheckbox", { name: "Start on Login" })).toBeNull();
});

test("More has the tray's update controls, in builds that have updates", async () => {
  const asked: unknown[] = [];
  const calls = backend({
    update_checks: () => false,
    set_update_checks: ({ enabled }) => { asked.push(enabled); return null; },
    check_for_updates: () => null,
  });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  const box = () => screen.getByRole("menuitemcheckbox", { name: "Check for updates automatically" });
  await screen.findByRole("menuitemcheckbox", { name: "Check for updates automatically" });
  expect(box().getAttribute("aria-checked")).toBe("false");
  // Each click asks natively, at once, for the opposite of what More shows,
  // even if a newer saved choice is still on its way; nothing shows until saved.
  fireEvent.click(box());
  if (!screen.queryByRole("menu")) fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  fireEvent.click(box());
  await waitFor(() => expect(asked).toEqual([true, true]));
  expect(box().getAttribute("aria-checked")).toBe("false");
  await act(async () => { await emit("update:automatic", true); });
  expect(box().getAttribute("aria-checked")).toBe("true");
  // Turned off from the tray while More is open.
  await act(async () => { await emit("update:automatic", false); });
  expect(box().getAttribute("aria-checked")).toBe("false");
  fireEvent.click(box());
  await waitFor(() => expect(asked).toEqual([true, true, true]));
  await chooseFromMore("Check for updates…");
  await waitFor(() => expect(calls.filter((c) => c === "check_for_updates")).toHaveLength(1));
  expect(screen.queryByRole("menu")).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
});

test("a saved change heard before the first read answers is kept", async () => {
  let answer!: (on: boolean) => void;
  backend({ update_checks: () => new Promise((resolve) => { answer = resolve; }), set_update_checks: () => null, check_for_updates: () => null });
  render(<App />);
  await waitFor(() => expect(answer).toBeDefined());
  await act(async () => { await emit("update:automatic", true); });
  await act(async () => { answer(false); await new Promise((resolve) => setTimeout(resolve, 0)); });
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  const box = await screen.findByRole("menuitemcheckbox", { name: "Check for updates automatically" });
  expect(box.getAttribute("aria-checked")).toBe("true");
});

test("builds without updates, and pickers without the commands, offer no update controls", async () => {
  backend();
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  await screen.findByRole("menuitemcheckbox", { name: "Larger text" });
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  expect(screen.queryByRole("menuitem", { name: "Check for updates…" })).toBeNull();
  expect(screen.queryByRole("menuitemcheckbox", { name: "Check for updates automatically" })).toBeNull();
  cleanup();
  const { updateChecks: _read, ...withoutRead } = api;
  backend({ update_checks: () => true });
  render(<App api={withoutRead} />);
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  await screen.findByRole("menuitemcheckbox", { name: "Larger text" });
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  expect(screen.queryByRole("menuitem", { name: "Check for updates…" })).toBeNull();
});
