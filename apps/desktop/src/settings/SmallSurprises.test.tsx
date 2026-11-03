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
  await act(async () => fireEvent.click(screen.getByRole("option", { name: /^About Ibis/ })));
  expect(screen.getByText(nameLine).closest("[data-highlighted]")).not.toBeNull();
  const icon = screen.getByRole("button", { name: "Ibis" });
  for (let i = 0; i < 30; i++) await act(async () => fireEvent.click(icon));
  expect(document.querySelector("[data-dip]")).toBeNull();
  expect(screen.queryByText(/White ibises|Scarlet ibises|Northern bald/)).toBeNull();
});

test("each click dips the bird and says the next fact, and leaving About returns the name line", async () => {
  const api = demoApi();
  await act(async () => render(<SettingsApp api={api} initialPage="about" />));
  expect(screen.getByText(nameLine)).toBeTruthy();
  const icon = screen.getByRole("button", { name: "Ibis" });
  for (const fact of [
    "White ibises find food by touch as they probe the mud.",
    "Scarlet ibises get their colour from pigments in the shellfish they eat.",
    "Ancient Egyptians mummified ibises as offerings to Thoth.",
    "The scarlet ibis is a national bird of Trinidad and Tobago.",
    "Northern bald ibises take turns leading their V formation.",
  ]) {
    await act(async () => fireEvent.click(icon));
    expect(screen.getByText(fact).closest("[aria-live]")).not.toBeNull();
    expect(screen.queryByText(nameLine)).toBeNull();
    expect(document.querySelector("[data-dip]")).not.toBeNull();
  }
  fireEvent.click(screen.getByRole("option", { name: "General" }));
  await act(async () => fireEvent.click(screen.getByRole("option", { name: "About Ibis" })));
  expect(screen.getByText(nameLine)).toBeTruthy();
  expect(document.querySelector("[data-dip]")).toBeNull();
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Ibis" })));
  expect(screen.getByText("White ibises find food by touch as they probe the mud.")).toBeTruthy();
});

test("a focus read while a click is pending cannot lose that click's fact", async () => {
  const api = demoApi(), answer = api.delightView!;
  let release!: () => void;
  const held = new Promise<void>((resolve) => { release = resolve; });
  api.delightView = async (now, presentation, revision, click) => {
    const view = await answer(now, presentation, revision, click);
    if (click) await held;
    return view;
  };
  await act(async () => render(<SettingsApp api={api} initialPage="about" />));
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Ibis" })));
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  await act(async () => { release(); await held; });
  expect(screen.getByText("White ibises find food by touch as they probe the mud.")).toBeTruthy();
  expect(document.querySelector("[data-dip]")).not.toBeNull();
});

test("a focus read keeps the fact a click just said", async () => {
  await act(async () => render(<SettingsApp api={demoApi()} initialPage="about" />));
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Ibis" })));
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  expect(screen.getByText("White ibises find food by touch as they probe the mud.")).toBeTruthy();
});

test("a failed switch write restores the authoritative choice without losing other settings", async () => {
  const api = demoApi();
  vi.spyOn(api, "setPickerPreferences").mockRejectedValueOnce(new Error("invented write failure"));
  await act(async () => render(<SettingsApp api={api} initialPage="general" />));
  fireEvent.click(screen.getByRole("switch", { name: "Small surprises" }));
  await waitFor(() => expect(screen.getByRole<HTMLInputElement>("switch", { name: "Small surprises" }).checked).toBe(true));
  expect((await api.pickerPreferences()).paste).toBe(true);
});

test("the anniversary takes the line for its day and a click can still say a fact", async () => {
  const view = { fun: true, anniversary: true, fact: null, status: null, revision: 2, peck: false, build: "invented" };
  const delightView = vi.fn(async (_now, _presentation, _revision, click) => ({ ...view, fact: click ? "touch" as const : null }));
  await act(async () => render(<SettingsApp api={{ ...demoApi(), delightView }} initialPage="about" />));
  expect(screen.getByText("One year with Ibis today.")).toBeTruthy();
  expect(screen.queryByText(nameLine)).toBeNull();
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Ibis" })));
  expect(screen.getByText("White ibises find food by touch as they probe the mud.")).toBeTruthy();
});
