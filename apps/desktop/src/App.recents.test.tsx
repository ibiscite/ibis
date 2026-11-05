import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, chooseFromMore, prepared, resetApp, result } from "./testing/app";

afterEach(resetApp);
const writer = "invented.writer";
const destination = () => ({ app: "Invented Writer", app_id: writer, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null });
const a = result("a", "Recent A"), b = result("b", "Recent B"), c = result("c", "New C");

test("a failed Forget retry keeps the privacy warning visible until legacy history is deleted", async () => {
  const key = "ibis-citation-history";
  const warning = "Recent citations could not be forgotten. Try again.";
  localStorage.setItem(key, "invented private citation history");
  const remove = vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => { throw new Error("synthetic storage refusal"); });
  let clears = 0;
  backend({ recent_references: (args) => {
    if (args.forget) clears++;
    return [{ revision: "1", results: [] }, false, false];
  } });
  try {
    render(<App />);
    expect((await screen.findByRole("alert")).textContent).toBe(warning);
    await chooseFromMore("Forget Recent Citations");
    await waitFor(() => expect(clears).toBe(1));
    expect(localStorage.getItem(key)).toBe("invented private citation history");
    expect(screen.getByRole("alert").textContent).toBe(warning);
    remove.mockRestore();
    await chooseFromMore("Forget Recent Citations");
    await waitFor(() => expect(clears).toBe(2));
    await waitFor(() => expect(screen.queryByText(warning)).toBeNull());
    expect(localStorage.getItem(key)).toBeNull();
  } finally {
    remove.mockRestore();
    localStorage.removeItem(key);
  }
});

test("recents are ordinary rows in one keyboard list; choosing one resolves it afresh without an old page", async () => {
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

test.each(["compact", "comfortable"])("%s requests and renders the same recent groups", async (density) => {
  const requests: unknown[] = [];
  backend({ picker_destination: destination,
    picker_preferences: () => ({ density, larger_text: false }),
    recent_references: (args) => { requests.push(args.forget); return [{ ...a, recent: "cited" }, { ...b, recent: "added" }]; },
  });
  render(<App />);
  await screen.findByRole("heading", { name: "Cited here recently" });
  await screen.findByRole("heading", { name: "Recently added" });
  await waitFor(() => expect(document.documentElement.dataset.density).toBe(density));
  expect(requests.at(-1)).toBe(false);
  expect(screen.getAllByRole("option").map((row) => row.textContent?.slice(0, 8))).toEqual([expect.stringContaining("Recent A"), expect.stringContaining("Recent B")]);
});


// Persistence and privacy races are exercised against the engine in
// recents/tests.rs. These scenarios assert the ordinary picker outcomes.
function nativeHistory() {
  let cited = true;
  const requests: unknown[] = [];
  return {
    requests,
    recent_references: (args: Record<string, unknown>) => {
      requests.push(args);
      if (args.forget) cited = false;
      return [{ revision: "1", results: cited ? [{ ...a, recent: "cited" }] : [] }, cited, false];
    },
    copy_file_citation: (args: Record<string, unknown>) => {
      cited = true;
      const saved = prepared(args);
      return { text: saved.preview.text, warnings: [] };
    },
  };
}
const copyB = async () => {
  const search = screen.getByLabelText("Search references");
  fireEvent.change(search, { target: { value: "b" } });
  fireEvent.click(await screen.findByText("Recent B"), { detail: 2 });
  await screen.findByText("Copied");
  fireEvent.change(search, { target: { value: "" } });
};

test("Forget clears native history and an old receipt cannot repopulate it on reopen", async () => {
  const native = nativeHistory();
  backend({ picker_destination: destination, recent_references: native.recent_references, copy_file_citation: native.copy_file_citation, search: () => [b] });
  render(<App />);
  await screen.findByText("Recent A");
  fireEvent.click(screen.getByRole("button", { name: "Keep Window Open" }));
  await copyB();
  await screen.findByText("Recent A");
  await chooseFromMore("Forget Recent Citations");
  await waitFor(() => expect(screen.queryByText("Recent A")).toBeNull());
  await act(async () => { await emit("picker:shown", null); });
  expect(screen.queryByRole("heading", { name: "Cited here recently" })).toBeNull();
  expect(within(screen.getByRole("listbox", { name: "Results" })).queryAllByRole("option")).toHaveLength(0);
  expect(native.requests.filter((args) => (args as { forget: boolean }).forget)).toEqual([{ forget: true }]);
  expect(localStorage.getItem("ibis-citation-history")).toBeNull();
});

test("after Forget, a fresh native citation can be shown without webview recording", async () => {
  const native = nativeHistory();
  backend({ picker_destination: destination, recent_references: native.recent_references, copy_file_citation: native.copy_file_citation, search: () => [b] });
  render(<App />);
  await screen.findByText("Recent A");
  fireEvent.click(screen.getByRole("button", { name: "Keep Window Open" }));
  await chooseFromMore("Forget Recent Citations");
  await waitFor(() => expect(screen.queryByText("Recent A")).toBeNull());
  await copyB();
  await screen.findByText("Recent A");
  expect(native.requests.every((args) => Object.keys(args as object).join() === "forget")).toBe(true);
  expect(localStorage.getItem("ibis-citation-history")).toBeNull();
});

test("a saved off followed by on refreshes native rows and clears Change Last Citation once", async () => {
  const native = nativeHistory();
  let off = false;
  backend({ picker_destination: destination,
    recent_references: (args) => off ? [{ revision: "1", results: [] }, false, false] : native.recent_references(args),
    search: () => [b],
  });
  render(<App />);
  await screen.findByText("Recent A");
  off = true;
  await act(async () => {
    await emit("preferences:changed", { revision: 1, recent_citations: false });
    await emit("preferences:changed", { revision: 2, recent_citations: true });
  });
  await waitFor(() => expect(screen.queryByRole("heading", { name: "Cited here recently" })).toBeNull());
  fireEvent.click(screen.getByRole("button", { name: "More" }));
  expect(await screen.findByRole("menu")).toBeTruthy();
  expect(screen.queryByRole("menuitem", { name: /Forget Recent Citations/ })).toBeNull();
});
