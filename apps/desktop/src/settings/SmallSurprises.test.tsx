import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { SettingsApp } from "./SettingsApp";
import { demoApi } from "./demo";

const nameLine = "Named for the bird ancient Egyptians linked with Thoth, god of writing and keeper of records.";

test("Small surprises defaults on, persists off, and thoth still reveals the passive name line", async () => {
  const api = demoApi();
  const mounted = await act(async () => render(<SettingsApp api={api} initialPage="general" />));
  const toggle = screen.getByRole<HTMLInputElement>("switch", { name: "Small surprises" });
  expect(toggle.checked).toBe(true);
  fireEvent.click(toggle);
  await waitFor(() => expect(toggle.checked).toBe(false));
  mounted.unmount();
  await act(async () => render(<SettingsApp api={api} initialPage="general" />));
  expect(screen.getByRole<HTMLInputElement>("switch", { name: "Small surprises" }).checked).toBe(false);
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "thoth" } });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "About Ibis" })));
  const line = screen.getByText(nameLine);
  expect(line.closest("[data-highlighted]")).not.toBeNull();
  expect(line.closest('[role="status"], [aria-live]')).toBeNull();
  const icon = screen.getByRole("button", { name: "Ibis" });
  for (let i = 0; i < 30; i++) await act(async () => fireEvent.click(icon));
  expect(document.querySelector('[data-motion="blink"]')).toBeNull();
  expect(screen.queryByText(/White ibises|Scarlet ibises|Northern bald/)).toBeNull();
});

test("each five icon clicks reveals the next passive fact and retains the name line", async () => {
  const api = demoApi();
  await act(async () => render(<SettingsApp api={api} initialPage="about" />));
  const icon = screen.getByRole("button", { name: "Ibis" });
  for (const fact of [
    "Ancient Egyptians associated the ibis with Thoth, god of writing.",
    "White ibises find food by touch as they probe the mud.",
    "Scarlet ibises get their colour from pigments in the shellfish they eat.",
    "Ancient Egyptians mummified ibises as offerings to Thoth.",
    "The scarlet ibis is a national bird of Trinidad and Tobago.",
    "Northern bald ibises take turns leading their V formation.",
  ]) {
    for (let i = 0; i < 4; i++) await act(async () => fireEvent.click(icon));
    expect(screen.queryByText(fact)).toBeNull();
    await act(async () => fireEvent.click(icon));
    expect(screen.getByText(fact).closest('[role="status"], [aria-live]')).toBeNull();
    expect(screen.getByText(nameLine)).toBeTruthy();
  }
});

test("a failed switch write restores the authoritative choice without losing other settings", async () => {
  const api = demoApi();
  vi.spyOn(api, "setPickerPreferences").mockRejectedValueOnce(new Error("invented write failure"));
  await act(async () => render(<SettingsApp api={api} initialPage="general" />));
  fireEvent.click(screen.getByRole("switch", { name: "Small surprises" }));
  await waitFor(() => expect(screen.getByRole<HTMLInputElement>("switch", { name: "Small surprises" }).checked).toBe(true));
  expect((await api.pickerPreferences()).paste).toBe(true);
});

test("one anniversary line and the factual name stay visible without competing facts or announcements", async () => {
  const api = { ...demoApi(), delightView: vi.fn().mockResolvedValue({ fun: true, birthday: true, anniversary: true, fact: "thoth", status: null, library: null, revision: 2, peck: false, build: "invented" }) };
  await act(async () => render(<SettingsApp api={api} initialPage="about" />));
  expect(screen.getByText("Ibis turns two today.").closest('[aria-live], [role="status"]')).toBeNull();
  expect(screen.queryByText("One year with Ibis")).toBeNull();
  expect(screen.queryByText("Ancient Egyptians associated the ibis with Thoth, god of writing.")).toBeNull();
  expect(screen.getByText(nameLine)).toBeTruthy();
});
