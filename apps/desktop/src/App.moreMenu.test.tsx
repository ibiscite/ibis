import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "./App";
import { backend, chooseFromMore, resetApp } from "./testing/app";

afterEach(resetApp);

test("More holds working actions, leaving preferences and update checks to Settings", async () => {
  const calls = backend({ login_item: () => ({ name: "Ibis", enabled: true, problem: null }),
    update_checks: () => true, update_available: () => true, check_for_updates: () => null });
  render(<App />);
  await chooseFromMore("Rows: Comfortable");
  // Opening Rows never changes the view or saves a preference.
  expect(document.documentElement.dataset.density).toBe("comfortable");
  expect(calls).not.toContain("set_picker_preferences");
  const more = screen.getByRole("menu", { name: "More" });
  expect([...more.querySelectorAll('[role^="menuitem"]')].map((item) => item.getAttribute("aria-label") ?? item.textContent))
    .toEqual(["Open ZoteroCtrl+O", "Rows: Comfortable", "Keep window openCtrl+Shift+P", "Show what will be pastedCtrl+I", "Keyboard shortcutsCtrl+/", "Settings…Ctrl+,", "Quit IbisCtrl+Q"]);
});

test.each(["Compact", "Comfortable", "Spacious"])("Rows offers every density and its line count, then chooses %s directly", async (choice) => {
  const saved: unknown[] = [];
  backend({ set_picker_preferences: ({ preferences }) => { saved.push(preferences); return null; } });
  render(<App />);
  await chooseFromMore("Rows: Comfortable");
  const rows = await screen.findByRole("menu", { name: "Rows" });
  const options = within(rows).getAllByRole("menuitemradio");
  expect(options.map((item) => item.textContent)).toEqual(["Compact1 line", "Comfortable2 lines", "Spacious3 lines"]);
  expect(options.map((item) => document.getElementById(item.getAttribute("aria-describedby")!)?.textContent))
    .toEqual(["1 line", "2 lines", "3 lines"]);
  expect(options.map((item) => item.getAttribute("aria-checked"))).toEqual(["false", "true", "false"]);
  const option = within(rows).getByRole("menuitemradio", { name: choice });
  act(() => option.focus());
  if (choice === "Spacious") {
    fireEvent.keyDown(option, { key: "Enter" });
    fireEvent.keyUp(option, { key: "Enter" });
  } else fireEvent.click(option);
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  await waitFor(() => expect(document.documentElement.dataset.density).toBe(choice.toLowerCase()));
  expect(saved).toEqual(choice === "Comfortable" ? [] : [{ density: choice.toLowerCase() }]);
  await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText("Search references")));
  await chooseFromMore(`Rows: ${choice}`);
  expect(within(await screen.findByRole("menu", { name: "Rows" })).getByRole("menuitemradio", { name: choice }).getAttribute("aria-checked")).toBe("true");
  expect(saved).toEqual(choice === "Comfortable" ? [] : [{ density: choice.toLowerCase() }]);
});

test("Rows supports arrow navigation and Escape returns to More without saving", async () => {
  const calls = backend();
  render(<App />);
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "k", ctrlKey: true });
  await screen.findByRole("menu", { name: "More" });
  const trigger = screen.getByRole("menuitem", { name: "Rows: Comfortable" });
  act(() => trigger.focus());
  fireEvent.keyDown(trigger, { key: "ArrowRight" });
  const submenu = await screen.findByRole("menu", { name: "Rows" });
  fireEvent.keyDown(document.activeElement!, { key: "End" });
  expect(document.activeElement).toBe(within(submenu).getByRole("menuitemradio", { name: "Spacious" }));
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("menu", { name: "Rows" })).toBeNull());
  expect(screen.getByRole("menu", { name: "More" })).toBeTruthy();
  expect(document.activeElement).toBe(trigger);
  expect(calls).not.toContain("set_picker_preferences");
});

test("Tab from Rows dismisses both menus and returns typing to Search", async () => {
  backend();
  render(<App />);
  await chooseFromMore("Rows: Comfortable");
  const submenu = await screen.findByRole("menu", { name: "Rows" });
  fireEvent.keyDown(within(submenu).getByRole("menuitemradio", { name: "Compact" }), { key: "Tab" });
  expect(screen.queryByRole("menu")).toBeNull();
  expect(document.activeElement).toBe(screen.getByLabelText("Search references"));
  fireEvent.input(document.activeElement!, { target: { value: "immediate" } });
  expect(screen.getByLabelText<HTMLInputElement>("Search references").value).toBe("immediate");
});

test("Command-O from Rows leaves an accessible failure and working recovery", async () => {
  const calls = backend({ open_library_source: () => { throw { code: "failed" }; }, choose_library: () => null });
  render(<App />);
  await chooseFromMore("Rows: Comfortable");
  fireEvent.keyDown(await screen.findByRole("menu", { name: "Rows" }), { key: "o", ctrlKey: true });
  const failure = await screen.findByRole("alert");
  expect(failure.textContent).toBe("Zotero could not open. Try again.");
  expect(screen.queryByRole("menu")).toBeNull();
  await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText("Search references")));
  fireEvent.keyDown(document.activeElement!, { key: "o", ctrlKey: true });
  await screen.findByRole("heading", { name: "Where do your references live?" });
  expect(calls.filter((call) => call === "open_library_source")).toHaveLength(1);
});
