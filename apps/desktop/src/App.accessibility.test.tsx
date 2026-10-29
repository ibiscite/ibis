import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, chooseFromMore, resetApp, type Handler } from "./testing/app";
import type { PickerPreferences } from "./api";

const saved: PickerPreferences = { citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true, search_mark: false, paste: true };
const observers: MutationObserver[] = [];
afterEach(async () => { observers.splice(0).forEach((observer) => observer.disconnect()); await resetApp(); });
const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
};
async function open(overrides: Record<string, Handler> = {}) {
  const calls = backend({ picker_preferences: () => saved, ...overrides });
  await act(async () => { render(<App />); });
  return calls;
}
function feedback() {
  const region = screen.getByRole("status", { name: "View changes" });
  const updates: string[] = [];
  const observer = new MutationObserver(() => { if (region.textContent) updates.push(region.textContent); });
  observer.observe(region, { childList: true, subtree: true, characterData: true });
  observers.push(observer);
  return { region, updates };
}

test("Rows applies once in a persistent polite region; initial load, same-value reads and save acknowledgements stay silent", async () => {
  const save = deferred<null>();
  let loaded = saved;
  const calls = await open({ picker_preferences: () => loaded, set_picker_preferences: () => save.promise });
  const { region, updates } = feedback();
  expect(region.textContent).toBe("");
  expect(region.getAttribute("aria-live")).toBe("polite");
  expect(region.getAttribute("aria-atomic")).toBe("true");
  // A hidden announcer must not turn the empty visual notices into a padded row.
  expect(region.closest(".notices")).toBeNull();
  expect(document.querySelector(".notices")?.matches(":empty")).toBe(true);
  await chooseFromMore("Rows: Comfortable");
  await waitFor(() => expect(region.textContent).toBe("Rows: Spacious"));
  expect(document.documentElement.dataset.density).toBe("spacious");
  expect(updates).toEqual(["Rows: Spacious"]);
  await act(async () => { save.resolve(null); });
  loaded = { ...saved, density: "spacious" };
  await act(async () => { await emit("preferences:changed"); });
  expect(screen.getByRole("status", { name: "View changes" })).toBe(region);
  expect(updates).toEqual(["Rows: Spacious"]);
  expect(calls.filter((call) => call === "set_picker_preferences")).toHaveLength(1);
  expect(calls).not.toContain("copy_file_citation");
});

test.each([{ isComposing: true }, { keyCode: 229 }])("composition confirmation %j cannot change Rows, save or announce", async (composition) => {
  const calls = await open();
  const { region, updates } = feedback();
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  const rows = await screen.findByRole("menuitem", { name: "Rows: Comfortable" });
  act(() => rows.focus());
  fireEvent.keyDown(rows, { key: "Enter", code: "Enter", ...composition });
  fireEvent.keyUp(rows, { key: "Enter", code: "Enter", ...composition });
  expect(screen.getByRole("menu")).toBeTruthy();
  expect(document.documentElement.dataset.density).toBe("comfortable");
  expect(region.textContent).toBe("");
  expect(updates).toEqual([]);
  expect(calls).not.toContain("set_picker_preferences");
});

test("a failed Rows save keeps the applied view and reports a fixed error without claiming persistence", async () => {
  const save = deferred<null>();
  await open({ set_picker_preferences: () => save.promise });
  const { region, updates } = feedback();
  await chooseFromMore("Rows: Comfortable");
  await waitFor(() => expect(region.textContent).toBe("Rows: Spacious"));
  await act(async () => { save.reject(new Error("SYNTHETIC_PRIVATE_SAVE_ERROR")); });
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Ibis could not read or save your picker choices. They apply until Ibis restarts.");
  expect(document.documentElement.dataset.density).toBe("spacious");
  expect(updates).toEqual(["Rows: Spacious"]);
  expect(region.textContent).not.toMatch(/saved/i);
  expect(document.body.textContent).not.toContain("SYNTHETIC_PRIVATE_SAVE_ERROR");
});

test("a delayed preference read cannot restore or announce A after the user applies B", async () => {
  const old = deferred<PickerPreferences>();
  let reads = 0;
  await open({ picker_preferences: () => ++reads === 1 ? saved : old.promise });
  const { region, updates } = feedback();
  await act(async () => { await emit("preferences:changed"); });
  expect(reads).toBe(2);
  await chooseFromMore("Rows: Comfortable");
  await waitFor(() => expect(region.textContent).toBe("Rows: Spacious"));
  await act(async () => { old.resolve({ ...saved, density: "compact" }); });
  expect(document.documentElement.dataset.density).toBe("spacious");
  expect(region.textContent).toBe("Rows: Spacious");
  expect(updates).toEqual(["Rows: Spacious"]);
});

test("hide/reopen retires Rows feedback before an old save completes", async () => {
  const save = deferred<null>();
  await open({ set_picker_preferences: () => save.promise });
  const { region, updates } = feedback();
  await chooseFromMore("Rows: Comfortable");
  await waitFor(() => expect(region.textContent).toBe("Rows: Spacious"));
  await act(async () => { await emit("picker:hidden"); await emit("picker:shown"); });
  expect(region.textContent).toBe("");
  await act(async () => { save.resolve(null); });
  expect(screen.getByRole("status", { name: "View changes" })).toBe(region);
  expect(region.textContent).toBe("");
  expect(updates).toEqual(["Rows: Spacious"]);
  expect(document.documentElement.dataset.density).toBe("spacious");
});

test("the same Rows label announces again after a silent background view change", async () => {
  await open();
  const { region, updates } = feedback();
  await chooseFromMore("Rows: Comfortable");
  await waitFor(() => expect(document.documentElement.dataset.density).toBe("spacious"));
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  expect(updates).toEqual(["Rows: Spacious"]);
  // A fresh read applies the saved external view; it is not an old reply.
  await act(async () => { await emit("preferences:changed"); });
  expect(document.documentElement.dataset.density).toBe("comfortable");
  expect(updates).toEqual(["Rows: Spacious"]);
  await chooseFromMore("Rows: Comfortable");
  await waitFor(() => expect(document.documentElement.dataset.density).toBe("spacious"));
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  expect(region.textContent).toBe("Rows: Spacious");
  expect(updates).toEqual(["Rows: Spacious", "Rows: Spacious"]);
});
