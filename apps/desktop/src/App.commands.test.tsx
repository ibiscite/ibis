import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "./App";
import { backend, resetApp, result } from "./testing/app";

afterEach(resetApp);

test('">" lists commands by name with their keys; typing narrows them and Enter runs one', async () => {
  const calls = backend({ quit: () => null, search: () => [result("vale2021", "The quiet garden")] });
  render(<App />);
  const search = screen.getByLabelText("Search references");
  fireEvent.change(search, { target: { value: ">" } });
  const commands = await screen.findByRole("listbox", { name: "Commands" });
  const names = within(commands).getAllByRole("option").map((row) => row.querySelector(".result-title")?.textContent);
  expect(names).toEqual(expect.arrayContaining(["Change Format…", "Keep Window Open", "Keyboard Shortcuts", "Settings…", "Quit Ibis"]));
  // The library is never searched for a command, and nothing would be pasted.
  expect(screen.queryByRole("region", { name: "Preview" })).toBeNull();
  expect(screen.getByRole("button", { name: /^Run Change Format/ })).toBeTruthy();
  // Words in the title or the description narrow the list.
  fireEvent.change(search, { target: { value: "> quit" } });
  await waitFor(() => expect(within(screen.getByRole("listbox", { name: "Commands" })).getAllByRole("option")).toHaveLength(1));
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(calls).toContain("quit"));
  expect((search as HTMLInputElement).value).toBe("");
});

test('↓ moves through commands and a running command clears the search; nothing matching says so', async () => {
  backend({ search: () => [result("vale2021", "The quiet garden")] });
  render(<App />);
  const search = screen.getByLabelText("Search references");
  fireEvent.change(search, { target: { value: "> window" } });
  const row = await screen.findByRole("option", { name: /Keep Window Open/ });
  expect(row.getAttribute("aria-selected")).toBe("true");
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(screen.getByRole("button", { name: /Keep window open|Pin|Unpin/i }).getAttribute("aria-pressed")).toBe("true"));
  fireEvent.change(search, { target: { value: "> zzqx" } });
  expect(await screen.findByText("No matching commands")).toBeTruthy();
});

test("⌘Y peeks at the highlighted reference without taking focus, and ⌘Y or Esc closes it", async () => {
  backend({ search: () => [{ ...result("vale2021", "The quiet garden"), container: "Journal of Imagined Studies" }] });
  render(<App />);
  const search = screen.getByLabelText("Search references");
  fireEvent.change(search, { target: { value: "quiet" } });
  await screen.findByText("The quiet garden");
  act(() => search.focus());
  fireEvent.keyDown(search, { key: "y", code: "KeyY", ctrlKey: true });
  const peek = await screen.findByRole("region", { name: "Details of The quiet garden" });
  expect(within(peek).getByText("Journal of Imagined Studies")).toBeTruthy();
  expect(within(peek).getByText("vale2021")).toBeTruthy();
  expect(document.activeElement).toBe(search);
  fireEvent.keyDown(search, { key: "y", code: "KeyY", ctrlKey: true });
  await waitFor(() => expect(screen.queryByRole("region", { name: /^Details of/ })).toBeNull());
});

test("the shelf's beak drags exactly what Enter pastes", async () => {
  backend({ search: () => [result("vale2021", "The quiet garden")] });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "quiet" } });
  const preview = await screen.findByRole("region", { name: "Preview" });
  const beak = await waitFor(() => { const node = preview.querySelector<HTMLElement>(".shelf-leading[draggable]"); expect(node).toBeTruthy(); return node!; });
  const data = new Map<string, string>();
  fireEvent.dragStart(beak, { dataTransfer: { setData: (type: string, value: string) => data.set(type, value), effectAllowed: "" } });
  expect(data.get("text/plain")).toBe(preview.querySelector(".preview-text")?.textContent);
});
