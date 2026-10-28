import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { AppOutput, PickerDestination } from "./api";
import { backend, chooseFromMore, prepared, resetApp, result, type PreviewRequest } from "./testing/app";

afterEach(resetApp);
const WORD: PickerDestination = { app: "Example Word", app_id: "example.word", automatic: { style: "apa" }, remembered: null, unreadable: false };
const BROWSER: PickerDestination = { app: "Example Browser", app_id: "example.browser", automatic: null, remembered: null, unreadable: false };
const formatButton = () => within(screen.getByLabelText("Search references").closest("header")!).getByRole("button", { name: /^Copy as / });
const formatShown = () => formatButton().getAttribute("aria-label")!.slice("Copy as ".length);
const openFormats = () => { if (formatButton().getAttribute("aria-expanded") !== "true") fireEvent.click(formatButton()); };
async function pick(name: string, role: "menuitemradio" | "menuitemcheckbox" | "menuitem" = "menuitemradio") {
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  openFormats();
  fireEvent.click(screen.getByRole(role, { name }));
}
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
      return { ticket: `ticket-${number}`, content_hash: `hash-${number}`, text: request.style ? `(Smith, 2020) ${request.style}` : `synthetic ${request.format}`, html: null, warnings: [] };
    },
    copy_file_citation: (args) => {
      copies.push({ ticket: args.ticket, contentHash: args.contentHash });
      return { text: prepared(args).preview.text, warnings: [] };
    },
    remember_app_output: ({ output }) => { remembered.push(output as AppOutput | null); return null; },
    ...overrides,
  });
  render(<App />);
  return { calls, previews, copies, remembered };
}
async function copy(previews: PreviewRequest[], copies: { ticket: string; contentHash: string }[]) {
  const preview = previews.length;
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(copies[0]).toEqual({ ticket: `ticket-${preview}`, contentHash: `hash-${preview}` });
}

test("opened from a word processor the preview is APA and copy uses its exact receipt", async () => {
  const { previews, copies } = opened(() => WORD);
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("APA · Example Word"));
  await waitFor(() => expect(previews.at(-1)?.style).toBe("apa"));
  await copy(previews, copies);
});

test.each([
  ["cite", "\\cite"], ["pandoc", "Pandoc"], ["typst", "Typst"],
] as const)("a browser-hosted key workflow uses the chosen global %s default and exact preview", async (format, name) => {
  const { previews, copies } = opened(() => BROWSER, { copy_format: () => format });
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe(`${name} · Example Browser`));
  await waitFor(() => expect(previews.at(-1)).toMatchObject({ style: null, format }));
  await copy(previews, copies);
});

test("a format pick is remembered for the browser, then Automatic restores its global fallback", async () => {
  let destination = BROWSER;
  const { calls, remembered } = opened(() => destination);
  await screen.findByText("First");
  await pick("\\cite");
  await waitFor(() => expect(remembered).toEqual([{ key: "cite" }]));
  destination = { ...BROWSER, remembered: { key: "cite" } };
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatShown()).toBe("\\cite · Example Browser"));
  await pick("Automatic", "menuitem");
  await waitFor(() => expect(remembered).toEqual([{ key: "cite" }, null]));
  await waitFor(() => expect(formatShown()).toBe("Pandoc · Example Browser"));
  expect(calls).not.toContain("set_picker_preferences");
});

test("Just this once overrides a remembered app output for one copy without saving either default", async () => {
  const { previews, copies, calls, remembered } = opened(() => ({ ...BROWSER, remembered: { key: "cite" } }));
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("\\cite · Example Browser"));
  await pick("Just this once", "menuitemcheckbox");
  await pick("Typst");
  await waitFor(() => expect(previews.at(-1)).toMatchObject({ style: null, format: "typst" }));
  await copy(previews, copies);
  await waitFor(() => expect(formatShown()).toBe("\\cite · Example Browser"));
  expect(remembered).toEqual([]);
  expect(calls).not.toContain("set_picker_preferences");
});

test("deliberately setting a global default preserves another app's remembered choice", async () => {
  let destination = { ...BROWSER, remembered: { key: "cite" } } as PickerDestination;
  const saved: unknown[] = [];
  const { remembered } = opened(() => destination, { set_picker_preferences: ({ copyFormat }) => { saved.push(copyFormat); return null; } });
  await screen.findByText("First");
  await pick("Just this once", "menuitemcheckbox");
  await pick("Typst");
  await pick("Use as global default", "menuitem");
  await waitFor(() => expect(saved).toEqual(["typst"]));
  destination = { ...BROWSER, app: "Example Editor", app_id: "example.editor" };
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatShown()).toBe("Typst · Example Editor"));
  destination = { ...BROWSER, remembered: { key: "cite" } };
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatShown()).toBe("\\cite · Example Browser"));
  expect(remembered).toEqual([]);
});

test("an unreadable per-app file says defaults apply, and More forgets every app", async () => {
  const { calls } = opened(() => ({ ...WORD, unreadable: true }));
  await screen.findByText("First");
  expect((await screen.findByText(/could not read the formats it remembers for apps/)).getAttribute("role")).toBe("alert");
  await chooseFromMore("Forget formats for apps");
  await waitFor(() => expect(calls).toContain("forget_app_outputs"));
});

test("without a captured app no app name or Automatic action is offered", async () => {
  opened(() => ({ app: null, app_id: null, automatic: null, remembered: null, unreadable: false }));
  await screen.findByText("First");
  expect(formatShown()).toBe("Pandoc");
  await pick("Just this once", "menuitemcheckbox");
  openFormats();
  expect(screen.queryByRole("menuitem", { name: "Automatic" })).toBeNull();
});

test("Automatic in a formatted app uses the global style just chosen in this picker", async () => {
  const { remembered } = opened(() => WORD);
  await screen.findByText("First");
  await pick("IEEE");
  await waitFor(() => expect(remembered).toEqual([{ style: "ieee" }]));
  await pick("Use as global default", "menuitem");
  await pick("Automatic", "menuitem");
  await waitFor(() => expect(remembered).toEqual([{ style: "ieee" }, null]));
  await waitFor(() => expect(formatShown()).toBe("IEEE · Example Word"));
});

test("a successful once-only copy consumes its override even after the highlighted row changed", async () => {
  let finish!: (value: unknown) => void;
  const first = new Promise((done) => { finish = done; });
  const formats: string[] = [];
  opened(() => ({ ...BROWSER, remembered: { key: "cite" } }), {
    search: () => [result("a", "First"), result("b", "Second")],
    copy_file_citation: (args) => {
      const preparedCopy = prepared(args);
      formats.push(preparedCopy.request.format);
      return formats.length === 1 ? first : { text: preparedCopy.preview.text, warnings: [] };
    },
  });
  await screen.findByText("First");
  fireEvent.click(screen.getByRole("button", { name: "Keep Window Open" }));
  await pick("Just this once", "menuitemcheckbox");
  await pick("Typst");
  const search = screen.getByLabelText("Search references");
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(formats).toEqual(["typst"]));
  fireEvent.keyDown(search, { key: "ArrowDown" });
  await act(async () => { finish({ text: "synthetic typst copy", warnings: [] }); });
  await waitFor(() => expect(formatShown()).toBe("\\cite · Example Browser"));
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(formats).toEqual(["typst", "cite"]));
});

test("one format shortcut opens the scope choices without copying or changing any default", async () => {
  const { calls, remembered } = opened(() => BROWSER);
  await screen.findByText("First");
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  expect(formatButton().getAttribute("aria-keyshortcuts")).toBe("Control+Shift+F");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "F", ctrlKey: true, shiftKey: true });
  expect(await screen.findByRole("menuitemcheckbox", { name: "Just this once" })).toBeTruthy();
  expect(screen.getByRole("menuitem", { name: "Automatic" })).toBeTruthy();
  expect(screen.getByRole("menuitem", { name: "Use as global default" })).toBeTruthy();
  expect(calls).not.toContain("copy_file_citation");
  expect(calls).not.toContain("set_picker_preferences");
  expect(remembered).toEqual([]);
});

test("an unrelated Settings refresh does not extend a one-time copy override", async () => {
  let finish!: (value: unknown) => void;
  const copying = new Promise((done) => { finish = done; });
  const { calls } = opened(() => ({ ...BROWSER, remembered: { key: "cite" } }), { copy_file_citation: () => copying });
  await screen.findByText("First");
  fireEvent.click(screen.getByRole("button", { name: "Keep Window Open" }));
  await pick("Just this once", "menuitemcheckbox");
  await pick("Typst");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(calls).toContain("copy_file_citation"));
  await act(async () => { await emit("preferences:changed"); });
  await waitFor(() => expect(calls.filter((call) => call === "picker_destination")).toHaveLength(2));
  await act(async () => { finish({ text: "synthetic typst copy", warnings: [] }); });
  await waitFor(() => expect(formatShown()).toBe("\\cite · Example Browser"));
});

test("a failed preference read keeps the native app's authoritative automatic style", async () => {
  const { previews, copies } = opened(() => ({ ...WORD, automatic: { style: "ieee" } }), {
    picker_preferences: () => { throw new Error("synthetic preference read failure"); },
  });
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("IEEE · Example Word"));
  await waitFor(() => expect(previews.at(-1)?.style).toBe("ieee"));
  await copy(previews, copies);
});

test("Automatic cancels an Enter that was waiting for another output's preview", async () => {
  let finish!: (value: unknown) => void;
  const typst = new Promise((done) => { finish = done; });
  const { copies } = opened(() => ({ ...BROWSER, remembered: { key: "cite" } }), {
    preview_file_citation: ({ format }) => format === "typst" ? typst : {
      ticket: `ticket-${format}`, content_hash: `hash-${format}`, text: `synthetic ${format}`, html: null, warnings: [],
    },
  });
  await screen.findByText("First");
  await pick("Just this once", "menuitemcheckbox");
  await pick("Typst");
  const search = screen.getByLabelText("Search references");
  fireEvent.keyDown(search, { key: "Enter" });
  await pick("Automatic", "menuitem");
  await waitFor(() => expect(formatShown()).toBe("Pandoc · Example Browser"));
  await act(async () => { finish({ ticket: "old-typst", content_hash: "old-typst", text: "old Typst", html: null, warnings: [] }); });
  expect(copies).toEqual([]);
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(copies).toEqual([{ ticket: "ticket-pandoc", contentHash: "hash-pandoc" }]));
});
