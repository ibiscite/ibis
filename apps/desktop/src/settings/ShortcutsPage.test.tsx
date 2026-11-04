import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { expect, test } from "vitest";
import { SettingsApp } from "./SettingsApp";
import { demoApi } from "./demo";

test("Shortcuts filters action names, keeps alternatives whole, edits only Open Ibis and lists every other key as plain text", async () => {
  await act(async () => render(<SettingsApp api={demoApi()} initialPage="shortcuts" />));
  const table = screen.getByRole("table", { name: "Keyboard Shortcuts" });
  expect(within(table).getAllByRole("columnheader").map((node) => node.textContent)).toEqual(["Action", "Shortcut"]);
  expect(within(table).getAllByRole("rowgroup").slice(1).map((node) => within(node).getAllByRole("row")[0]!.textContent))
    .toEqual(["Open Ibis", "While searching", "In the citation", "Your library", "Anywhere in Ibis", "In a menu"]);
  const quit = within(table).getByRole("row", { name: /Quit Ibis/ });
  expect(quit.querySelector("kbd")?.textContent).toBeTruthy();
  // Only the recorder edits a key, so every key the table lists is plain text, none a cap.
  const listed = [...table.querySelectorAll("tbody:not(#shortcuts-system) kbd")];
  expect(listed.length).toBeGreaterThan(10);
  expect(listed.filter((kbd) => !kbd.hasAttribute("data-plain"))).toEqual([]);
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
