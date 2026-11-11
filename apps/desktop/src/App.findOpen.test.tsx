import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "./App";
import { backend, resetApp, result } from "./testing/app";
import { fixtureDestination } from "./testing/format";
import { isMac } from "./platform";

afterEach(resetApp);

test("inside Zotero Enter opens the PDF or item, Command-O shows it, and Alt-Enter copies", async () => {
  const open = vi.fn(async (_args: Record<string, unknown>) => null);
  const calls = backend({ library_status: () => ({ state: "loaded", has_snapshot: true, zotero: true, snapshot_zotero: true, zotero_libraries: ["user"], name: "Zotero", references: 1, warnings: 0, error: null, problem: null, note: null }), search: () => [result("a", "Synthetic reference")],
    picker_destination: () => fixtureDestination({ app: "Zotero", app_id: "org.zotero.zotero", find_open: true }),
    open_library_source: open, zotero_groups: () => [],
  });
  render(<App />);
  await screen.findByText("Synthetic reference");
  const search = screen.getByLabelText("Search references");
  await screen.findByText("Open PDF, or show item in Zotero");
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(open).toHaveBeenCalledOnce());
  expect(open.mock.calls[0]?.[0]).toMatchObject({ reveal: false });
  expect(calls).not.toContain("copy_file_citation");
  fireEvent.keyDown(search, { key: "o", ctrlKey: !isMac, metaKey: isMac });
  await waitFor(() => expect(open).toHaveBeenCalledTimes(2));
  expect(open.mock.calls[1]?.[0]).toMatchObject({ reveal: true });
  fireEvent.keyDown(search, { key: "Enter", altKey: true });
  await waitFor(() => expect(calls).toContain("copy_file_citation"));
});
