import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, receipt, resetApp, result } from "./testing/app";

afterEach(resetApp);

const plain = (text: string | null | undefined) => (text ?? "").replace(/[⁨⁩]/g, "");

test("a waiting editor is named with its format, and its end asks for a fresh preview", async () => {
  let waiting: string | null = "Pandoc";
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

test("with no editor waiting nothing changes", async () => {
  backend({ editor_request: () => null, search: () => [result("taylor2026", "Synthetic signals")] });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "signals" } });
  await screen.findByText("Synthetic signals");
  expect(document.body.textContent).not.toContain("Citing for your editor");
});
