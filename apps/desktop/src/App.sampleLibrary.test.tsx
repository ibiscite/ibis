import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, prepared, receipt, resetApp, result } from "./testing/app";
import type { Density } from "./api";
import { warningText } from "./i18n";
import samples from "../src-tauri/resources/samples.csljson?raw";

afterEach(resetApp);

const bundledSamples = JSON.parse(samples);
const lantern = bundledSamples[0];
const title = lantern.title;
const apa = "(Vale, 2024) [Sample]";
const sampleKeys = `[@${lantern["citation-key"]}] [Sample]`;
const apaWarnings = [{ item: null, message: { id: "notice-year-suffix-withheld", args: {} } }];

function press(row: HTMLElement) {
  act(() => row.focus());
  fireEvent.keyDown(row, { key: "Enter", code: "Enter" });
  fireEvent.keyUp(row, { key: "Enter", code: "Enter" });
}

function setup(density: Density = "comfortable", formatted = true) {
  let mode: "none" | "sample" | "real" = "none";
  const status = () => ({
    revision: mode, state: mode === "none" ? "none" : "loaded", has_snapshot: mode !== "none",
    name: mode === "sample" ? "Sample references" : mode === "real" ? "invented.bib" : null,
    snapshot_name: mode === "sample" ? "Sample references" : mode === "real" ? "invented.bib" : null,
    references: mode === "sample" ? bundledSamples.length : mode === "real" ? 1 : 0, sample: mode === "sample",
    zotero: false, warnings: 0, error: null, problem: null, note: null,
  });
  const switchTo = async (next: typeof mode) => { mode = next; await emit("library:changed", status()); };
  const copied = vi.fn((args: unknown) => ({ outcome: "retained", reason: "warnings", copied: { text: prepared(args).preview.text, warnings: prepared(args).preview.warnings } }));
  const preview = vi.fn(async ({ items }: { items: { id: { library: string } }[] }) =>
    receipt(items[0]?.id.library === "samples" ? formatted ? apa : sampleKeys : "(Fen, 2023)",
      null,
      formatted ? apaWarnings : []));
  const calls = backend({
    library_status: status,
    use_sample_library: ({ enabled }) => switchTo(enabled ? "sample" : "none"),
    choose_library: () => null,
    picker_preferences: () => ({ output: formatted ? "apa" : "pandoc", density }),
    search: ({ query }) => ({ revision: mode, results: mode === "none" || !query.text ? [] : [{
      ...result(mode === "sample" ? lantern["citation-key"] : "own", mode === "sample" ? title : "My invented reference", mode === "sample" ? "samples" : "real"),
      revision: mode, sample: mode === "sample",
    }] }),
    preview_file_citation: preview,
    deliver_citation: copied,
  });
  render(<App />);
  return { calls, copied, preview, switchTo };
}

async function chooseSamples() {
  press(await screen.findByRole("option", { name: /^Try Sample References/ }));
  await screen.findByRole("option", { name: /^Use Your Own Library/ });
}

test.each([true, false])("first citation: choose samples, search, Enter and receipt (formatted: %s)", async (formatted) => {
  const { calls, copied } = setup("comfortable", formatted);
  const text = formatted ? apa : sampleKeys;
  await screen.findByRole("option", { name: /^Try Sample References/ });
  expect(calls).not.toContain("use_sample_library");
  await chooseSamples();
  expect(screen.getByText("Search for “lantern”, choose the lantern gardens reference, then press Enter.")).toBeTruthy();
  const input = screen.getByRole("combobox");
  expect(document.activeElement).toBe(input);
  fireEvent.change(input, { target: { value: "lantern" } });
  const row = await screen.findByRole("option", { name: new RegExp(`Sample · ${title}`) });
  expect(within(row).getByText("Sample")).toBeTruthy();
  expect(screen.getByRole("status", { name: "Library status" }).textContent).toBe("Sample references");
  // With results the way out steps aside: one row after them, nothing centred, its title unseen.
  const exit = screen.getByRole("listbox", { name: "Sample references" });
  expect(exit.closest(".results-area")?.hasAttribute("data-centered")).toBe(false);
  expect(screen.getByRole("heading", { name: "Sample references" }).className).toBe("visually-hidden");
  await waitFor(() => expect(document.querySelector(".shelf")?.textContent).toContain(text));
  fireEvent.keyDown(input, { key: "Enter" });
  if (formatted) {
    expect(await screen.findByText("Copied", { selector: ".shelf-copied, .copy-notice strong" })).toBeTruthy();
    expect(prepared(copied.mock.calls[0]?.[0]).preview.text).toBe(apa);
    expect(screen.getByRole("list", { name: "Copy warnings" }).textContent).toContain(warningText(apaWarnings[0]!));
  } else {
    await waitFor(() => expect(copied).toHaveBeenCalledOnce());
  }
  expect(copied).toHaveBeenCalledOnce();
  expect(prepared(copied.mock.calls[0]?.[0]).preview.text).toContain("[Sample]");
  expect(calls).not.toContain("picker_preferences_saved");
});
