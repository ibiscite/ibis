import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { LibrarySetup } from "./LibrarySetup";

afterEach(cleanup);
const perform = async (_action: string, run: () => Promise<unknown>) => { await run(); };

test.each(["available", "absent", "access_off"] as const)("first-run has one suggested library action when Zotero is %s", async (zotero) => {
  const choose = vi.fn(async () => undefined), useZotero = vi.fn(async () => undefined);
  const { container } = render(<LibrarySetup api={{ chooseLibrary: choose, useZotero,
    findSources: async () => ({ zotero, list: 1, files: [], cut_short: false }) }} perform={perform}
    writing={{ format: "pandoc", formatted: true, choose: vi.fn() }} />);
  await waitFor(() => {
    const primary = container.querySelectorAll<HTMLButtonElement>(".setup-choices > .suggested-action");
    expect(primary).toHaveLength(1);
    expect(primary.item(0).textContent).toBe(zotero === "available" ? "Use Zotero" : "Choose library…");
  });
  fireEvent.click(container.querySelector<HTMLButtonElement>(".suggested-action")!);
  expect(zotero === "available" ? useZotero : choose).toHaveBeenCalledOnce();
  expect(zotero === "available" ? choose : useZotero).not.toHaveBeenCalled();
});
