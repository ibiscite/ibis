import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, chooseFromMore, receipt, resetApp, result } from "./testing/app";

afterEach(resetApp);

const plain = (text: string | null | undefined) => (text ?? "").replace(/[⁨⁩]/g, "");

test("a waiting editor is named with its format, and its end asks for a fresh preview", async () => {
  let waiting: { request: number; format: string } | null = { request: 1, format: "Pandoc" };
  let previews = 0;
  backend({
    editor_request: () => waiting,
    search: () => [result("taylor2026", "Synthetic signals")],
    preview_file_citation: () => { previews += 1; return receipt(waiting ? "@taylor2026" : "[@taylor2026]"); },
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "signals" } });
  await waitFor(() => expect(plain(document.body.textContent)).toContain("Citing for your editor in Pandoc."));
  await waitFor(() => expect(document.body.textContent).toContain("@taylor2026"));
  const before = previews;
  waiting = null;
  await act(() => emit("editor:request"));
  await waitFor(() => expect(plain(document.body.textContent)).not.toContain("Citing for your editor"));
  await waitFor(() => expect(previews).toBeGreaterThan(before));
  await waitFor(() => expect(document.body.textContent).toContain("[@taylor2026]"));
});

test("with no editor waiting nothing changes and nothing is searched twice", async () => {
  const calls = backend({ editor_request: () => null, search: () => [result("taylor2026", "Synthetic signals")] });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "signals" } });
  await screen.findByText("Synthetic signals");
  await act(() => emit("editor:request"));
  expect(document.body.textContent).not.toContain("Citing for your editor");
  expect(calls.filter((command) => command === "search")).toHaveLength(1);
});

test("a second request in the same format is still a new request", async () => {
  let waiting: { request: number; format: string } | null = { request: 1, format: "Pandoc" };
  let previews = 0;
  backend({
    editor_request: () => waiting,
    search: () => [result("taylor2026", "Synthetic signals")],
    preview_file_citation: () => { previews += 1; return receipt("@taylor2026"); },
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "signals" } });
  await waitFor(() => expect(previews).toBeGreaterThan(0));
  const before = previews;
  waiting = { request: 2, format: "Pandoc" };
  await act(() => emit("editor:request"));
  await waitFor(() => expect(previews).toBeGreaterThan(before));
});

test("Keep My Order follows the waiting editor's output, not the picker's format", async () => {
  let waiting: { request: number; format: string; style: boolean } | null = { request: 1, format: "APA", style: true };
  const orders: boolean[] = [];
  backend({
    editor_request: () => waiting,
    search: () => [result("taylor2026", "Synthetic signals")],
    preview_file_citation: ({ intent }) => { orders.push(Boolean(intent.keep_order)); return receipt("(Taylor, 2026)"); },
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "signals" } });
  await waitFor(() => expect(plain(document.body.textContent)).toContain("Citing for your editor in APA."));
  await chooseFromMore("Keep My Order");
  await waitFor(() => expect(orders.at(-1)).toBe(true));
  // The picker's own Pandoc always keeps the order, so it does not offer the choice.
  waiting = null;
  await act(() => emit("editor:request"));
  await waitFor(() => expect(plain(document.body.textContent)).not.toContain("Citing for your editor"));
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  await screen.findByRole("dialog", { name: "Actions" });
  expect(screen.queryByRole("option", { name: "Keep My Order" })).toBeNull();
});
