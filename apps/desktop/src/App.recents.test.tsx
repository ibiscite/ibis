import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, chooseFromMore, prepared, resetApp, result } from "./testing/app";
import { rememberCitation } from "./features/picker/recentHistory";

afterEach(resetApp);
const writer = "invented.writer";
const destination = () => ({ app: "Invented Writer", app_id: writer, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null });
const a = result("a", "Recent A"), b = result("b", "Recent B"), c = result("c", "New C");
const seed = () => rememberCitation([], writer, [a.id, b.id], localStorage);

test("recents are ordinary rows in one keyboard list; choosing one resolves it afresh without an old page", async () => {
  seed();
  const copies: unknown[] = [];
  backend({ picker_destination: destination,
    recent_references: () => [{ ...a, recent: "cited" }, { ...b, recent: "cited" }, { ...c, recent: "added" }],
    search: () => [result("d", "Typed result")],
    copy_file_citation: (args) => { const saved = prepared(args); copies.push(saved.request.items); return { text: saved.preview.text, warnings: [] }; },
  });
  render(<App />);
  await screen.findByRole("heading", { name: "Cited here recently" });
  expect(screen.getByRole("heading", { name: "Recently added" })).toBeTruthy();
  const search = screen.getByLabelText("Search references");
  fireEvent.keyDown(search, { key: "ArrowDown" });
  fireEvent.keyDown(search, { key: "ArrowDown" });
  expect(screen.getByRole("option", { selected: true }).textContent).toContain("New C");
  fireEvent.click(screen.getByText("New C"), { ctrlKey: true });
  fireEvent.change(search, { target: { value: "typed" } });
  await screen.findByText("Typed result");
  expect(screen.queryByRole("heading", { name: "Recently added" })).toBeNull();
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(copies[0]).toEqual([{ id: c.id, cite: c.cite, locator: null }]);
});

test("Compact requests and renders only Cited here recently", async () => {
  seed();
  const requests: unknown[] = [];
  backend({ picker_destination: destination,
    picker_preferences: () => ({ citation_style: null, density: "compact", larger_text: false, show_paste_preview: true }),
    recent_references: (args) => { requests.push(args.includeAdded); return [{ ...a, recent: "cited" }]; },
  });
  render(<App />);
  await screen.findByRole("heading", { name: "Cited here recently" });
  await waitFor(() => expect(document.documentElement.dataset.density).toBe("compact"));
  expect(requests.at(-1)).toBe(false);
  expect(screen.queryByRole("heading", { name: "Recently added" })).toBeNull();
});

test("Forget Recent Citations clears durable history and an old receipt cannot repopulate it on reopen", async () => {
  seed();
  backend({ picker_destination: destination,
    recent_references: (args) => Array.isArray(args.ids) && args.ids.length ? [{ ...a, recent: "cited" }] : [],
    search: () => [b],
  });
  render(<App />);
  await screen.findByText("Recent A");
  fireEvent.click(screen.getByRole("button", { name: "Keep Window Open" }));
  const search = screen.getByLabelText("Search references");
  fireEvent.change(search, { target: { value: "b" } });
  fireEvent.click(await screen.findByText("Recent B"), { detail: 2 });
  await screen.findByText("Copied");
  fireEvent.change(search, { target: { value: "" } });
  await screen.findByText("Recent A");
  await chooseFromMore("Forget Recent Citations");
  await waitFor(() => expect(localStorage.getItem("ibis-citation-history")).toBe("[]"));
  await act(async () => { await emit("picker:shown", null); });
  expect(localStorage.getItem("ibis-citation-history")).toBe("[]");
  expect(screen.queryByRole("heading", { name: "Cited here recently" })).toBeNull();
  expect(within(screen.getByRole("listbox", { name: "Results" })).queryAllByRole("option")).toHaveLength(0);
});

const saved = () => JSON.parse(localStorage.getItem("ibis-citation-history") ?? "[]") as { app: string; items: unknown[] }[];
const copyB = async () => {
  const search = screen.getByLabelText("Search references");
  fireEvent.change(search, { target: { value: "b" } });
  fireEvent.click(await screen.findByText("Recent B"), { detail: 2 });
  await screen.findByText("Copied");
  fireEvent.change(search, { target: { value: "" } });
};

test("Recent citations off forgets the history, records nothing and hides Forget; on again starts empty", async () => {
  seed();
  let on = false;
  backend({ picker_destination: destination,
    picker_preferences: () => ({ citation_style: null, density: "comfortable", larger_text: false, show_paste_preview: true, search_icon: "magnifier", paste: true, menu_bar: true, recent_citations: on }),
    recent_references: (args) => Array.isArray(args.ids) && args.ids.length ? [{ ...a, recent: "cited" }] : [],
    search: () => [b],
  });
  render(<App />);
  await waitFor(() => expect(saved()).toEqual([]));
  fireEvent.click(screen.getByRole("button", { name: "Keep Window Open" }));
  await copyB();
  expect(saved()).toEqual([]);
  expect(screen.queryByRole("heading", { name: "Cited here recently" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "More" }));
  expect(await screen.findByRole("menu")).toBeTruthy();
  expect(screen.queryByRole("menuitem", { name: /Forget Recent Citations/ })).toBeNull();
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

  // Turning it back on in Settings starts empty: the earlier receipt is not replayed.
  on = true;
  await act(async () => { await emit("preferences:changed", { revision: 1 }); });
  await act(async () => { await emit("picker:shown", null); });
  expect(saved()).toEqual([]);
  await copyB();
  await waitFor(() => expect(saved()).toEqual([{ app: writer, items: [b.id] }]));
});

test("after Forget, nothing is recorded until the next citation, which is recorded alone", async () => {
  seed();
  backend({ picker_destination: destination,
    recent_references: (args) => Array.isArray(args.ids) && args.ids.length ? [{ ...a, recent: "cited" }] : [],
    search: () => [b],
  });
  render(<App />);
  await screen.findByText("Recent A");
  fireEvent.click(screen.getByRole("button", { name: "Keep Window Open" }));
  await chooseFromMore("Forget Recent Citations");
  await waitFor(() => expect(saved()).toEqual([]));
  await act(async () => { await emit("preferences:changed", { revision: 1 }); });
  await act(async () => { await emit("picker:shown", null); });
  expect(saved()).toEqual([]);
  await copyB();
  await waitFor(() => expect(saved()).toEqual([{ app: writer, items: [b.id] }]));
});

test("Recent citations turned off and quickly on again still forgets, though the re-read sees on", async () => {
  seed();
  backend({ picker_destination: destination,
    recent_references: (args) => Array.isArray(args.ids) && args.ids.length ? [{ ...a, recent: "cited" }] : [],
    search: () => [b],
  });
  render(<App />);
  await screen.findByText("Recent A");
  fireEvent.click(screen.getByRole("button", { name: "Keep Window Open" }));
  await act(async () => {
    await emit("preferences:changed", { revision: 1, recent_citations: false });
    await emit("preferences:changed", { revision: 2, recent_citations: true });
  });
  await waitFor(() => expect(saved()).toEqual([]));
  await act(async () => { await emit("picker:shown", null); });
  expect(screen.queryByRole("heading", { name: "Cited here recently" })).toBeNull();
  await copyB();
  await waitFor(() => expect(saved()).toEqual([{ app: writer, items: [b.id] }]));
});
