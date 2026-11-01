import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { expect, test } from "vitest";
import { SettingsApp } from "./SettingsApp";
import { demoApi } from "./demo";

test("Shortcuts filters action names, keeps alternatives whole, and edits only Open Ibis", async () => {
  await act(async () => render(<SettingsApp api={demoApi()} initialPage="shortcuts" />));
  const table = screen.getByRole("table", { name: "Keyboard Shortcuts" });
  expect(within(table).getAllByRole("columnheader").map((node) => node.textContent)).toEqual(["Action", "Shortcut"]);
  expect(within(table).getAllByRole("rowgroup").slice(1).map((node) => within(node).getAllByRole("row")[0]!.textContent))
    .toEqual(["Open Ibis (global)", "While searching", "Anywhere in Ibis", "In a menu"]);
  const quit = within(table).getByRole("row", { name: /Quit Ibis/ });
  expect(within(quit).getByText("System")).toBeTruthy();
  expect(quit.querySelector("kbd")?.textContent).toBeTruthy();
  expect(within(quit).queryByRole("button")).toBeNull();
  const move = within(table).getByRole("row", { name: /Move through results/ });
  expect([...move.querySelectorAll("kbd")].map((node) => node.textContent)).toEqual(["↓", "↑"]);
  fireEvent.change(screen.getByRole("searchbox", { name: "Filter shortcuts" }), { target: { value: "quit" } });
  expect(within(table).getByRole("row", { name: /Quit Ibis/ })).toBeTruthy();
  expect(within(table).queryByText("Move through results")).toBeNull();
  fireEvent.change(screen.getByRole("searchbox", { name: "Filter shortcuts" }), { target: { value: "unmatched action" } });
  expect(screen.getByText("No matching actions")).toBeTruthy();
  fireEvent.change(screen.getByRole("searchbox", { name: "Filter shortcuts" }), { target: { value: "" } });
  expect(within(table).getByRole("button", { name: /^Open Ibis:/ })).toBeTruthy();
});
