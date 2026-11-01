import { act, render, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { afterEach, expect, test } from "vitest";
import { App } from "../App";
import { backend, chooseRows, resetApp } from "../testing/app";
import * as nativeApi from "../api";
import type { PickerPreferences, PickerSave } from "../bindings";

afterEach(resetApp);
const saved: PickerPreferences = { citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true, search_icon: "ibis", paste: true, appearance: "dark", accent: "purple" };

test("an open picker applies saved appearance and accent, then a Settings change, using its existing reads", async () => {
  let preferences = saved;
  const calls = backend({ picker_preferences: () => preferences });
  const { unmount } = render(<App />);
  await waitFor(() => expect(document.documentElement.dataset.appearance).toBe("dark"));
  expect(document.documentElement.dataset.accent).toBe("purple");
  expect(calls.filter((call) => call === "picker_preferences")).toHaveLength(1);
  preferences = { ...saved, appearance: "light", accent: "green" };
  await act(async () => { await emit("preferences:changed", preferences); });
  await waitFor(() => expect(document.documentElement.dataset.accent).toBe("green"));
  expect(document.documentElement.dataset.appearance).toBe("light");
  expect(calls.filter((call) => call === "picker_preferences")).toHaveLength(2);
  unmount();
  expect(document.documentElement.hasAttribute("data-accent")).toBe(false);
});

test("a delayed older picker read cannot restore the theme after a newer Settings event", async () => {
  let resolve!: (value: PickerPreferences) => void;
  const older = new Promise<PickerPreferences>((yes) => { resolve = yes; });
  let reads = 0;
  backend({ picker_preferences: () => ++reads === 1 ? older : { ...saved, appearance: "light", accent: "green" } });
  render(<App />);
  await waitFor(() => expect(reads).toBe(1));
  await act(async () => { await emit("preferences:changed", {}); });
  await waitFor(() => expect(document.documentElement.dataset.accent).toBe("green"));
  await act(async () => resolve(saved));
  expect(document.documentElement.dataset.appearance).toBe("light");
  expect(document.documentElement.dataset.accent).toBe("green");
});


test("a failed refresh keeps the last saved picker appearance usable", async () => {
  let fail = false;
  backend({ picker_preferences: () => fail ? Promise.reject(new Error("synthetic read failure")) : saved });
  render(<App />);
  await waitFor(() => expect(document.documentElement.dataset.accent).toBe("purple"));
  fail = true;
  await act(async () => { await emit("preferences:changed", {}); });
  expect(document.documentElement.dataset.accent).toBe("purple");
  expect(document.documentElement.dataset.appearance).toBe("dark");
});


test("a Settings notification invalidates a queued old save even when its newer refresh fails", async () => {
  let reads = 0, resolve!: (value: PickerSave) => void;
  const saving = new Promise<PickerSave>((yes) => { resolve = yes; });
  backend({ picker_preferences: () => ++reads === 1 ? saved : Promise.reject(new Error("synthetic newer read failure")) });
  const api = { ...nativeApi, setPickerPreferences: async () => saving };
  render(<App api={api} />);
  await waitFor(() => expect(document.documentElement.dataset.accent).toBe("purple"));
  await chooseRows("spacious");
  await act(async () => { await emit("preferences:changed", {}); });
  await act(async () => resolve({ preferences: { ...saved, accent: "blue" }, copy_format: "pandoc" }));
  await waitFor(() => expect(reads).toBe(2));
  expect(document.documentElement.dataset.accent).toBe("purple");
});
