import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test } from "vitest";
import { SettingsApp } from "./SettingsApp";
import { demoApi } from "./demo";

const nameLine = "Named for the bird ancient Egyptians linked with Thoth, god of writing and keeper of records.";

test("Small surprises defaults on, persists off, and thoth still reveals the passive name line", async () => {
  const api = demoApi();
  const mounted = await act(async () => render(<SettingsApp api={api} initialPage="general" />));
  const toggle = screen.getByRole<HTMLInputElement>("switch", { name: "Bird facts and milestones" });
  expect(toggle.checked).toBe(true);
  fireEvent.click(toggle);
  await waitFor(() => expect(toggle.checked).toBe(false));
  mounted.unmount();
  await act(async () => render(<SettingsApp api={api} initialPage="general" />));
  expect(screen.getByRole<HTMLInputElement>("switch", { name: "Bird facts and milestones" }).checked).toBe(false);
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "thoth" } });
  await act(async () => fireEvent.click(screen.getByRole("option", { name: /^About Ibis/ })));
  expect(screen.getByText(nameLine).closest("[data-highlighted]")).not.toBeNull();
  const icon = screen.getByRole("button", { name: "Ibis" });
  for (let i = 0; i < 30; i++) await act(async () => fireEvent.click(icon));
  expect(document.querySelector("[data-dip]")).toBeNull();
  expect(screen.queryByText(/White ibises|Scarlet ibises|Northern bald/)).toBeNull();
});
