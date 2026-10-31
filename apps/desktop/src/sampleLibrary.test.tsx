import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import { backend, prepared, receipt, resetApp, result } from "./testing/app";
import type { Density } from "./api";
import type { FilePreview } from "./bindings";
import samples from "../src-tauri/resources/samples.csljson?raw";

afterEach(resetApp);

const bundledSamples = JSON.parse(samples);
const lantern = bundledSamples[0];
const title = lantern.title;
const apa = "(Vale, 2024) [Sample]";
const sampleKeys = `[@${lantern["citation-key"]}] [Sample]`;
const apaWarnings = ["Year suffixes require the whole document; Ibis leaves them out."];

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
  const copied = vi.fn((args: unknown) => ({ text: prepared(args).preview.text, warnings: prepared(args).preview.warnings }));
  const notice = vi.fn(() => ({ outcome: "copied", reason: "paste-disabled", app: null }));
  const preview = vi.fn(async ({ items }: { items: { id: { library: string } }[] }) =>
    receipt(items[0]?.id.library === "samples" ? formatted ? apa : sampleKeys : "(Fen, 2023)",
      formatted ? items[0]?.id.library === "samples" ? "(Vale, 2024)<span> [Sample]</span>" : "(Fen, 2023)" : null,
      formatted ? apaWarnings : []));
  const calls = backend({
    library_status: status,
    use_sample_library: ({ enabled }) => switchTo(enabled ? "sample" : "none"),
    choose_library: () => null,
    picker_preferences: () => ({ citation_style: formatted ? "apa" : null, density, show_paste_preview: true }),
    search: ({ query }) => ({ revision: mode, results: mode === "none" || !query.text ? [] : [{
      ...result(mode === "sample" ? lantern["citation-key"] : "own", mode === "sample" ? title : "My invented reference", mode === "sample" ? "samples" : "real"),
      revision: mode, sample: mode === "sample",
    }] }),
    preview_file_citation: preview,
    copy_file_citation: copied,
    show_copied_notice: notice,
  });
  render(<App />);
  return { calls, copied, notice, preview, switchTo };
}

async function chooseSamples() {
  press(await screen.findByRole("option", { name: /^Try sample references/ }));
  await screen.findByRole("option", { name: /^Use your own library/ });
}

test.each([true, false])("first citation: choose samples, search, Enter and receipt (formatted: %s)", async (formatted) => {
  const { calls, copied, notice } = setup("comfortable", formatted);
  const text = formatted ? apa : sampleKeys;
  await screen.findByRole("option", { name: /^Try sample references/ });
  expect(calls).not.toContain("use_sample_library");
  await chooseSamples();
  expect(screen.getByText("Search for “lantern”, choose the lantern gardens reference, then press Enter.")).toBeTruthy();
  const input = screen.getByRole("combobox");
  expect(document.activeElement).toBe(input);
  fireEvent.change(input, { target: { value: "lantern" } });
  const row = await screen.findByRole("option", { name: new RegExp(`Sample · ${title}`) });
  expect(within(row).getByText("Sample")).toBeTruthy();
  expect(screen.getByRole("status", { name: "Library status" }).textContent).toBe("Sample references · 3 references");
  await waitFor(() => expect(document.querySelector(".shelf")?.textContent).toContain(text));
  fireEvent.keyDown(input, { key: "Enter" });
  if (formatted) {
    await waitFor(() => expect(document.querySelector(".copy-notice p")?.textContent).toBe("Copied"));
    expect(prepared(copied.mock.calls[0]?.[0]).preview.text).toBe(apa);
    expect(notice).not.toHaveBeenCalled();
    expect(screen.getByRole("list", { name: "Copy warnings" }).textContent).toContain(apaWarnings[0]);
  } else {
    await waitFor(() => expect(notice).toHaveBeenCalledWith({ text, delivery: null }));
    expect(calls).toContain("hide_picker");
  }
  expect(copied).toHaveBeenCalledOnce();
  expect(prepared(copied.mock.calls[0]?.[0]).preview.text).toContain("[Sample]");
  expect(calls).not.toContain("set_picker_preferences");
});

test.each<Density>(["compact", "comfortable", "spacious"])("samples stay labelled in %s rows", async (density) => {
  setup(density);
  await chooseSamples();
  fireEvent.change(screen.getByRole("combobox"), { target: { value: "lantern" } });
  const row = await screen.findByRole("option", { name: new RegExp(`Sample · ${title}`) });
  expect(row.querySelector(".result-title")?.textContent).toBe(`Sample · ${title}`);
});

test("Use your own library returns to normal choices, clears sample selection, and a cancelled file choice stays there", async () => {
  const { calls } = setup();
  await chooseSamples();
  const input = screen.getByRole("combobox");
  fireEvent.change(input, { target: { value: "lantern" } });
  await screen.findByRole("option", { name: new RegExp(title) });
  fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
  await waitFor(() => expect(document.querySelector("main")?.dataset.selection).toBe("true"));
  press(screen.getByRole("option", { name: /^Use your own library/ }));
  const file = await screen.findByRole("option", { name: /^BibTeX, BibLaTeX or CSL-JSON file/ });
  expect(screen.queryByRole("option", { name: new RegExp(title) })).toBeNull();
  expect(screen.getByRole("option", { name: /^Zotero/ })).toBeTruthy();
  expect(document.querySelector(".shelf")).toBeNull();
  expect(document.querySelector("main")?.dataset.selection).toBeUndefined();
  press(file);
  await waitFor(() => expect(calls).toContain("choose_library"));
  expect(document.activeElement).toBe(file);
  expect(calls).not.toContain("copy_file_citation");
});

test.each(["none", "More", "keys", "keys from More"] as const)("welcome Escape dismisses %s then hides without clearing the retained sample query", async (overlay) => {
  const { calls } = setup();
  await chooseSamples();
  const input = screen.getByRole<HTMLInputElement>("combobox");
  fireEvent.change(input, { target: { value: "lantern" } });
  await screen.findByRole("option", { name: new RegExp(title) });
  press(screen.getByRole("option", { name: /^Use your own library/ }));
  const zotero = await screen.findByRole("option", { name: /^Zotero/ });
  act(() => zotero.focus());
  expect(input.value).toBe("lantern");
  if (overlay !== "none") {
    const opener = overlay === "keys" ? zotero : screen.getByRole("button", { name: /^More/ });
    act(() => opener.focus());
    if (overlay === "keys") fireEvent.keyDown(opener, { key: "/", ctrlKey: true });
    else fireEvent.click(opener);
    if (overlay === "keys from More") {
      const item = await screen.findByRole("menuitem", { name: "Keyboard shortcuts" });
      act(() => item.focus());
      fireEvent.click(item);
    }
    const layer = await screen.findByRole(overlay === "More" ? "menu" : "dialog");
    fireEvent.keyDown(layer, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole(overlay === "More" ? "menu" : "dialog")).toBeNull());
    expect(calls).not.toContain("hide_picker");
    expect(input.value).toBe("lantern");
    if (overlay === "keys from More") await waitFor(() => expect(document.activeElement).toBe(opener));
  }
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  await waitFor(() => expect(calls.filter((call) => call === "hide_picker")).toHaveLength(1));
  expect(input.value).toBe("lantern");
  await act(async () => { await emit("picker:hidden"); await emit("picker:shown"); });
  fireEvent.keyDown(zotero, { key: "Escape" });
  await waitFor(() => expect(calls.filter((call) => call === "hide_picker")).toHaveLength(2));
});

test("welcome footer Escape offers Hide Ibis even while a sample query is retained", async () => {
  const { calls } = setup();
  await chooseSamples();
  fireEvent.change(screen.getByRole("combobox"), { target: { value: "lantern" } });
  await screen.findByRole("option", { name: new RegExp(title) });
  press(screen.getByRole("option", { name: /^Use your own library/ }));
  const zotero = await screen.findByRole("option", { name: /^Zotero/ });
  fireEvent.keyDown(zotero, { key: "Alt", altKey: true });
  const footer = within(document.querySelector("footer")!);
  expect(footer.queryByRole("button", { name: "Clear search" })).toBeNull();
  fireEvent.click(footer.getByRole("button", { name: "Hide Ibis" }));
  await waitFor(() => expect(calls).toContain("hide_picker"));
});

test("a real source replaces samples even when selected sample references were on the shelf", async () => {
  const { switchTo, copied, preview } = setup();
  await chooseSamples();
  const input = screen.getByRole("combobox");
  fireEvent.change(input, { target: { value: "lantern" } });
  await screen.findByRole("option", { name: new RegExp(title) });
  fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
  await waitFor(() => expect(document.querySelector("main")?.dataset.selection).toBe("true"));
  await act(() => switchTo("real"));
  const row = await screen.findByRole("option", { name: /My invented reference/ });
  expect(row.textContent).not.toContain("Sample");
  expect(screen.queryByRole("option", { name: /^Use your own library/ })).toBeNull();
  expect(document.querySelector("main")?.dataset.selection).toBeUndefined();
  await waitFor(() => expect(document.querySelector(".shelf")?.textContent).toContain("(Fen, 2023)"));
  fireEvent.keyDown(input, { key: "Enter" });
  await waitFor(() => expect(copied).toHaveBeenCalledOnce());
  expect(preview.mock.lastCall?.[0].items.map((item) => item.id.library)).toEqual(["real"]);
  expect(screen.getByRole("status", { name: "Library status" }).textContent).toBe("invented.bib · 1 reference");
});

test("leaving samples cancels an Enter waiting for its citation preview", async () => {
  const { copied, preview } = setup();
  await chooseSamples();
  let finish!: (preview: FilePreview) => void;
  preview.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  const input = screen.getByRole("combobox");
  fireEvent.change(input, { target: { value: "lantern" } });
  await screen.findByRole("option", { name: new RegExp(title) });
  await waitFor(() => expect(preview).toHaveBeenCalled());
  fireEvent.keyDown(input, { key: "Enter" });
  press(screen.getByRole("option", { name: /^Use your own library/ }));
  await screen.findByRole("option", { name: /^Zotero/ });
  await act(async () => { finish(receipt(apa)); });
  expect(copied).not.toHaveBeenCalled();
  expect(document.querySelector(".shelf")).toBeNull();
});

test.each(["setup", "real"] as const)("sample copy notices leave with the sample session (%s)", async (next) => {
  const { switchTo } = setup();
  await chooseSamples();
  const input = screen.getByRole("combobox");
  fireEvent.change(input, { target: { value: "lantern" } });
  await waitFor(() => expect(document.querySelector(".shelf")?.textContent).toContain(apa));
  fireEvent.keyDown(input, { key: "Enter" });
  await waitFor(() => expect(document.querySelector(".copy-notice p")?.textContent).toBe("Copied"));
  if (next === "real") await act(() => switchTo("real"));
  else press(screen.getByRole("option", { name: /^Use your own library/ }));
  await waitFor(() => expect(document.querySelector(".copy-notice")).toBeNull());
});

test("a sample-command failure reports one visible error and keeps setup available", async () => {
  backend({
    library_status: () => ({ state: "none", has_snapshot: false, name: null, references: 0 }),
    use_sample_library: () => { throw new Error("invented failure"); },
  });
  render(<App />);
  press(await screen.findByRole("option", { name: /^Try sample references/ }));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Ibis could not change the sample library. Try again.");
  expect(screen.getByRole("option", { name: /^Zotero/ })).toBeTruthy();
});
