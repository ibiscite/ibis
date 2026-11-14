import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { App } from "./App";
import { backend, findInCitation, libraryButton, openCitationDetails, resetApp, result } from "./testing/app";
import { tabStops } from "./ui/focus";
import { SettingsApp } from "./settings/SettingsApp";
import { demoApi } from "./settings/demo";
import { say } from "./i18n";
import { currentPlatform } from "./platform";

afterEach(resetApp);

// jsdom has no browser Tab default. Dispatch the real event first, then supply
// only that native default when the component has not consumed it.
function tab(back = false) {
  const active = document.activeElement as HTMLElement;
  act(() => {
    if (!fireEvent.keyDown(active, { key: "Tab", shiftKey: back })) return;
    const stops = tabStops(document.body), at = stops.indexOf(active);
    stops[(at + (back ? -1 : 1) + stops.length) % stops.length]?.focus();
  });
}
function walk(ordered: HTMLElement[]) {
  act(() => ordered[0]!.focus());
  for (const next of ordered.slice(1)) { tab(); expect(document.activeElement).toBe(next); }
  for (const previous of ordered.slice(0, -1).reverse()) { tab(true); expect(document.activeElement).toBe(previous); }
}

async function pickerWithCitation() {
  backend({ search: () => [result("aster", "Invented Aster work")] });
  render(<App />);
  await screen.findByText("Invented Aster work");
  const search = screen.getByRole<HTMLInputElement>("combobox", { name: say("picker-search-label") });
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  const token = await findInCitation("aster");
  const format = screen.getByRole("button", { name: /^Citation format/ });
  return { search, token, format };
}

test("keyboard walks picker, shelf, preview and library menu", async () => {
  const { search, token, format } = await pickerWithCitation();
  const footer = screen.getByRole("button", { name: /^More/ }).closest("footer")!;
  walk([search, format, token, ...tabStops(footer)]);

  act(() => search.focus());
  fireEvent.keyDown(search, { key: "y", [currentPlatform() === "macos" ? "metaKey" : "ctrlKey"]: true });
  await screen.findByRole("region", { name: say("peek-label", { title: "Invented Aster work" }) });
  expect(document.activeElement).toBe(search);
  fireEvent.keyDown(search, { key: "Escape" });
  expect(document.activeElement).toBe(search);

  const library = libraryButton();
  act(() => library.focus()); fireEvent.click(library);
  const menu = await screen.findByRole("menu");
  act(() => within(menu).getAllByRole("menuitem")[0]!.focus());
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await waitFor(() => expect(document.activeElement).toBe(library));
});

test("keyboard walks citation details and format picker", async () => {
  const { token, format } = await pickerWithCitation();
  act(() => token.focus());
  const page = await openCitationDetails();
  const dialog = screen.getByRole("dialog", { name: /^Citation details/ });
  const fields = tabStops(dialog);
  expect(fields).toContain(page);
  walk(fields);
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await waitFor(() => expect(document.activeElement).toBe(token));

  act(() => format.focus()); fireEvent.click(format);
  const palette = await screen.findByRole("dialog", { name: "Format" });
  const field = within(palette).getByRole("combobox");
  const action = within(palette).getByRole("button", { name: /^Use/ });
  const back = within(palette).getByRole("button", { name: "Back" });
  walk([field, action, back]);
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await waitFor(() => expect(document.activeElement).toBe(format));
});

test.each(["general", "about"] as const)("Settings %s walks sidebar, page and notices in visual order", async (page) => {
  render(<SettingsApp api={demoApi()} initialPage={page} />);
  const search = screen.getByRole("searchbox", { name: say("settings-search") });
  await screen.findByRole("button", { name: page === "about" ? /^Version/ : /^Open Ibis:/ });
  await waitFor(() => expect(screen.getAllByRole("switch").every((node) => !node.hasAttribute("data-disabled"))).toBe(true));
  const sidebar = screen.getByRole("navigation");
  const main = screen.getByRole("main");
  const pageStops = tabStops(main);
  const navStops = tabStops(sidebar).map((node) => node.getAttribute("role") === "listbox" ? node.querySelector<HTMLElement>('[aria-selected="true"]')! : node);
  expect(navStops[0]).toBe(search);
  expect(pageStops.length).toBeGreaterThan(2);
  // The page's first controls are the visible hero (About) or its first row.
  expect(pageStops[0]).toBe(page === "about" ? screen.getByRole("button", { name: say("delight-ibis") }) : screen.getByRole("button", { name: /^Open Ibis:/ }));
  walk([...navStops, ...pageStops]);
});
