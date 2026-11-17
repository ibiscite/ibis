import { fixtureDestination, formatAnswer } from "./testing/format";
import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "./App";
import { backend, chooseFromMore, prepared, resetApp, result, toggleKeepOpen, showPicker } from "./testing/app";

afterEach(resetApp);
const writer = "invented.writer";
const destination = () => (fixtureDestination({ app: "Invented Writer", app_id: writer, unreadable: false, can_paste: false, format: formatAnswer("pandoc", "yours") }));
const a = result("a", "Recent A"), b = result("b", "Recent B"), c = result("c", "New C");

test("recents are ordinary rows in one keyboard list; choosing one resolves it afresh without an old page", async () => {
  const copies: unknown[] = [];
  backend({ "destination": destination,
    recent_references: () => [{ ...a, recent: "cited" }, { ...b, recent: "cited" }, { ...c, recent: "added" }],
    search: () => [result("d", "Typed result")],
    deliver_citation: (args) => { const saved = prepared(args); copies.push(saved.request.items); return { outcome: "retained", copied: { text: saved.preview.text, warnings: [] }, reason: "pinned" }; },
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

// Persistence and privacy races are exercised against the engine in
// recents/tests.rs. These scenarios assert the ordinary picker outcomes.
function nativeHistory() {
  let cited = true;
  const requests: unknown[] = [];
  return {
    requests,
    recent_references: (args: Record<string, unknown>) => {
      if (!args.snapshot) requests.push(args);
      if (args.forget) cited = false;
      return [{ revision: "1", results: cited ? [{ ...a, recent: "cited" }] : [] }, cited, false];
    },
    deliver_citation: (args: Record<string, unknown>) => {
      cited = true;
      const saved = prepared(args);
      return { outcome: "retained", copied: { text: saved.preview.text, warnings: [] }, reason: "pinned" };
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
  backend({ destination: destination, recent_references: native.recent_references, deliver_citation: native.deliver_citation, search: () => [b] });
  render(<App />);
  await screen.findByText("Recent A");
  await toggleKeepOpen();
  await copyB();
  await screen.findByText("Recent A");
  await chooseFromMore("Forget Recent Citations");
  await waitFor(() => expect(screen.queryByText("Recent A")).toBeNull());
  await act(async () => { await showPicker(null); });
  expect(screen.queryByRole("heading", { name: "Cited here recently" })).toBeNull();
  expect(within(screen.getByRole("listbox", { name: "Results" })).queryAllByRole("option")).toHaveLength(0);
  expect(native.requests.filter((args) => (args as { forget: boolean }).forget)).toEqual([{ forget: true }]);
  expect(localStorage.getItem("ibis-citation-history")).toBeNull();
});
