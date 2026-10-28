import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { AppOutput, PickerDestination } from "./api";
import { backend, chooseFromMore, prepared, resetApp, result, type PreviewRequest } from "./testing/app";

// The output follows the app the picker was opened from (#1202). Invented apps and records only.
afterEach(resetApp);

const WORD: PickerDestination = { app: "Example Word", automatic: { style: "apa" }, remembered: null, unreadable: false };
const formatButton = () => within(screen.getByLabelText("Search references").closest("header")!).getByRole("button", { name: /^Copy as / });
const formatShown = () => formatButton().getAttribute("aria-label")!.slice("Copy as ".length);
const openFormats = () => { if (formatButton().getAttribute("aria-expanded") !== "true") fireEvent.click(formatButton()); };

function opened(destination: () => PickerDestination, overrides: Parameters<typeof backend>[0] = {}) {
  const previews: PreviewRequest[] = [];
  const copies: { ticket: string; contentHash: string }[] = [];
  const remembered: (AppOutput | null)[] = [];
  const calls = backend({
    search: () => [result("a", "First")],
    picker_destination: () => destination(),
    preview_file_citation: (request) => {
      previews.push(request);
      const number = previews.length;
      return { ticket: `ticket-${number}`, content_hash: `hash-${number}`, text: request.style ? `(Smith, 2020) ${request.style}` : `[@a] ${request.format}`, html: null, warnings: [] };
    },
    copy_file_citation: (args) => {
      copies.push({ ticket: args.ticket, contentHash: args.contentHash });
      const { preview } = prepared(args);
      return { text: preview.text, warnings: [] };
    },
    remember_app_output: ({ output }) => { remembered.push(output as AppOutput | null); return null; },
    ...overrides,
  });
  render(<App />);
  return { calls, previews, copies, remembered };
}

test("opened from a word processor the preview is APA and the copy is exactly the previewed citation", async () => {
  const { previews, copies } = opened(() => WORD);
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("APA · Example Word"));
  await waitFor(() => expect(previews.at(-1)?.style).toBe("apa"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(1));
  // The copy names the last preview's receipt, so its content hash equals the preview's.
  expect(copies[0]).toEqual({ ticket: `ticket-${previews.length}`, contentHash: `hash-${previews.length}` });
});

test("a pick applies just this once and the next opening is Automatic again", async () => {
  const { calls, previews } = opened(() => WORD);
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("APA · Example Word"));
  openFormats();
  fireEvent.click(screen.getByRole("menuitemradio", { name: /^Typst/ }));
  expect(formatShown()).toBe("Typst · Example Word");
  await waitFor(() => expect(previews.at(-1)).toMatchObject({ style: null, format: "typst" }));
  // A one-off pick for a known app never changes the default format.
  expect(calls).not.toContain("set_picker_preferences");
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatShown()).toBe("APA · Example Word"));
});

test("Always use for the app keeps the pick, and unchecking it returns to Automatic", async () => {
  let destination = WORD;
  const { remembered } = opened(() => destination);
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("APA · Example Word"));
  openFormats();
  fireEvent.click(screen.getByRole("menuitemradio", { name: /^Pandoc/ }));
  openFormats();
  const always = screen.getByRole("menuitemcheckbox", { name: "Always use for Example Word" });
  expect(always.getAttribute("aria-checked")).toBe("false");
  fireEvent.click(always);
  await waitFor(() => expect(remembered).toEqual([{ key: "pandoc" }]));
  destination = { ...WORD, remembered: { key: "pandoc" } };
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatShown()).toBe("Pandoc · Example Word"));
  // While it is checked, a new pick is what the app always uses.
  openFormats();
  fireEvent.click(screen.getByRole("menuitemradio", { name: /^Typst/ }));
  await waitFor(() => expect(remembered).toEqual([{ key: "pandoc" }, { key: "typst" }]));
  openFormats();
  const checked = screen.getByRole("menuitemcheckbox", { name: "Always use for Example Word" });
  expect(checked.getAttribute("aria-checked")).toBe("true");
  fireEvent.click(checked);
  await waitFor(() => expect(remembered).toEqual([{ key: "pandoc" }, { key: "typst" }, null]));
  expect(formatShown()).toBe("APA · Example Word");
});

test("a remembered choice read at start is used before anything is copied", async () => {
  const { previews } = opened(() => ({ ...WORD, remembered: { style: "modern-language-association" } }));
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("MLA · Example Word"));
  await waitFor(() => expect(previews.at(-1)?.style).toBe("modern-language-association"));
});

test("an app Ibis has no output for keeps the default, and a pick there changes the default", async () => {
  const saved: unknown[] = [];
  opened(() => ({ app: "Example Notes", automatic: null, remembered: null, unreadable: false }), {
    set_picker_preferences: ({ copyFormat }) => { saved.push(copyFormat); return null; },
  });
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("Pandoc · Example Notes"));
  openFormats();
  fireEvent.click(screen.getByRole("menuitemradio", { name: /^Typst/ }));
  await waitFor(() => expect(saved).toEqual(["typst"]));
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatShown()).toBe("Typst · Example Notes"));
});

test("an unreadable per-app file says defaults apply, and More forgets every app", async () => {
  const { calls } = opened(() => ({ ...WORD, unreadable: true }));
  await screen.findByText("First");
  expect((await screen.findByText(/could not read the formats it remembers for apps/)).getAttribute("role")).toBe("alert");
  await chooseFromMore("Forget formats for apps");
  await waitFor(() => expect(calls).toContain("forget_app_outputs"));
});

test("no app named without a captured app", async () => {
  opened(() => ({ app: null, automatic: null, remembered: null, unreadable: false }));
  await screen.findByText("First");
  expect(formatShown()).toBe("Pandoc");
  openFormats();
  expect(screen.queryByRole("menuitemcheckbox")).toBeNull();
});
