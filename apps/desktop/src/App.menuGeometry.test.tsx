import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { App } from "./App";
import * as api from "./api";
import { backend, resetApp, result } from "./testing/app";

afterEach(async () => { await resetApp(); vi.restoreAllMocks(); });

test.each(["More", "output"])("an empty compact picker opens and closes %s without requesting a taller frame", async (layer) => {
  const fit = vi.fn(async () => undefined);
  const calls = backend({ search: () => [] });
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(180);
  await act(async () => { render(<App api={{ ...api, fitPicker: fit }} />); });
  expect(fit).toHaveBeenLastCalledWith(180, expect.any(Boolean));
  const count = fit.mock.calls.length;
  const search = screen.getByRole("combobox");
  if (layer === "More") fireEvent.keyDown(search, { key: "k", ctrlKey: true });
  else fireEvent.click(screen.getByRole("button", { name: /^Copy as/ }));
  const menu = await screen.findByRole("menu");
  if (layer === "More") {
    fireEvent.keyDown(menu, { key: "End" });
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: /^Quit Ibis/ })));
  }
  expect(document.querySelector("main")?.hasAttribute("data-compact")).toBe(true);
  expect(fit).toHaveBeenCalledTimes(count);
  fireEvent.keyDown(layer === "More" ? menu : screen.getByRole("searchbox"), { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  await waitFor(() => expect(document.activeElement).toBe(layer === "More" ? screen.getByRole("button", { name: /^More/ }) : search));
  expect(fit).toHaveBeenCalledTimes(count);
  expect(calls).not.toContain("copy_file_citation");
  expect(calls).not.toContain("hide_picker");
});

test("results and selected references retain their expanded content fit while More opens", async () => {
  const fit = vi.fn(async () => undefined);
  backend({ search: ({ query }) => query.text ? [result("a", "First invented work")] : [] });
  await act(async () => { render(<App api={{ ...api, fitPicker: fit }} />); });
  const search = screen.getByRole("combobox");
  fireEvent.change(search, { target: { value: "work" } });
  await screen.findByRole("option", { name: /First invented work/ });
  expect(fit).toHaveBeenLastCalledWith(null, expect.any(Boolean));
  fireEvent.keyDown(search, { key: "Enter", shiftKey: true });
  await screen.findByRole("list", { name: "Selected references" });
  const count = fit.mock.calls.length;
  fireEvent.keyDown(search, { key: "k", ctrlKey: true });
  await screen.findByRole("menu");
  expect(fit).toHaveBeenCalledTimes(count);
  expect(document.querySelector("main")?.hasAttribute("data-compact")).toBe(false);
});

test.each([{ isComposing: true }, { keyCode: 229 }])("composing %j cannot open More or request a larger frame", async (composition) => {
  const fit = vi.fn(async () => undefined);
  const calls = backend({ search: () => [] });
  await act(async () => { render(<App api={{ ...api, fitPicker: fit }} />); });
  const count = fit.mock.calls.length;
  fireEvent.keyDown(screen.getByRole("combobox"), { key: "k", ctrlKey: true, ...composition });
  expect(screen.queryByRole("menu")).toBeNull();
  expect(fit).toHaveBeenCalledTimes(count);
  expect(calls).not.toContain("copy_file_citation");
});
