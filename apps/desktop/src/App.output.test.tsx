import { fixtureDestination, formatAnswer } from "./testing/format";
import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { PickerDestination } from "./api";
import { isStyle, listedOutput } from "./format/outputs";
import { OUTPUTS } from "./bindings";
import { backend, prepared, receipt, resetApp, result, type PreviewRequest, toggleKeepOpen } from "./testing/app";

afterEach(resetApp);
const WORD: PickerDestination = fixtureDestination({ app: "Example Word", app_id: "example.word", app_identity: null, unreadable: false, can_paste: false, paste: null, paste_question: false, can_choose_paste: false, format: formatAnswer("apa", "suggestion") });
const BROWSER: PickerDestination = fixtureDestination({ app: "Example Browser", app_id: "example.browser", app_identity: null, unreadable: false, can_paste: false, paste: null, paste_question: false, can_choose_paste: false, format: formatAnswer("pandoc", "yours") });
const formatButton = () => within(screen.getByLabelText("Search references").closest("header")!).getByRole("button", { name: /^Citation format / });
const formatShown = () => formatButton().getAttribute("aria-label")!.slice("Citation format ".length);
const openFormats = () => { if (formatButton().getAttribute("aria-expanded") !== "true") fireEvent.click(formatButton()); };
async function pick(name: string) {
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  openFormats();
  fireEvent.click(screen.getByRole("option", { name }));
}
const refuseSave = () => { throw new Error("synthetic save failure"); };
function opened(destination: () => PickerDestination, overrides: Parameters<typeof backend>[0] = {}) {
  const previews: PreviewRequest[] = [];
  const copies: { ticket: string; contentHash: string }[] = [];
  const remembered: (string | null)[] = [];
  const calls = backend({
    search: () => [result("a", "First")],
    picker_destination: () => destination(),
    preview_file_citation: (request) => {
      previews.push(request);
      const number = previews.length;
      const text = isStyle(listedOutput(request.output) ?? "pandoc") ? `(Smith, 2020) ${request.output}` : `synthetic ${request.output}`;
      return { ...receipt(text), ticket: `ticket-${number}`, content_hash: `hash-${number}` };
    },
    copy_file_citation: (args) => {
      copies.push({ ticket: args.ticket, contentHash: args.contentHash });
      return { text: prepared(args).preview.text, warnings: [] };
    },
    remember_app_output: ({ output }) => { remembered.push(output as string | null); return null; },
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
  await waitFor(() => expect(previews.at(-1)?.output).toBe("apa"));
  await copy(previews, copies);
});

test("a format pick is remembered for the browser, and a later pick replaces it", async () => {
  let destination = BROWSER;
  const { calls, remembered } = opened(() => destination);
  await screen.findByText("First");
  await pick("LaTeX \\cite");
  await waitFor(() => expect(remembered).toEqual(["cite"]));
  destination = fixtureDestination({ ...BROWSER, format: formatAnswer("cite", "app") });
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatShown()).toBe("LaTeX \\cite · Example Browser"));
  await pick("Typst");
  await waitFor(() => expect(remembered).toEqual(["cite", "typst"]));
  await waitFor(() => expect(formatShown()).toBe("Typst · Example Browser"));
  expect(calls).not.toContain("picker_preferences_saved");
});

test("a pick that cannot be saved says it is for this paste, then the saved format returns", async () => {
  const { previews, copies, calls } = opened(() => (fixtureDestination({ ...BROWSER, format: formatAnswer("cite", "app") })), { remember_app_output: refuseSave });
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("LaTeX \\cite · Example Browser"));
  await pick("Typst");
  await waitFor(() => expect(formatShown()).toBe("Typst · This paste"));
  expect((await screen.findByText(/could not save the choice for this app/)).getAttribute("role")).toBe("alert");
  openFormats();
  expect(screen.getByText("Typst applies to this paste only. Pick a format to remember it for this app.")).toBeTruthy();
  fireEvent.keyDown(await screen.findByRole("combobox", { name: /^Search (formats|actions)$/ }), { key: "Escape" });
  await waitFor(() => expect(previews.at(-1)).toMatchObject({ output: "typst" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await copy(previews, copies);
  await waitFor(() => expect(formatShown()).toBe("LaTeX \\cite · Example Browser"));
  expect(calls).not.toContain("picker_preferences_saved");
});

test("a pasted list copies every result in order in the app's format, then hides", async () => {
  const { previews, copies, calls, remembered } = opened(() => (fixtureDestination({ ...BROWSER, format: formatAnswer("cite", "app") })), {
    search: () => ({ revision: "1", results: [result("b", "Second"), result("a", "First")], list: { missing: [] } }),
  });
  await screen.findByText("Pasted list");
  await pick("Typst");
  await waitFor(() => expect(previews.at(-1)).toMatchObject({ output: "typst", items: [{ id: { key: "b" } }, { id: { key: "a" } }] }));
  await copy(previews, copies);
  await waitFor(() => expect(calls).toContain("hide_picker"));
  expect(remembered).toEqual(["typst"]);
});

test("a new default reaches apps with no remembered format and leaves remembered ones alone", async () => {
  let destination = fixtureDestination({ ...BROWSER, app: null, app_id: null, format: formatAnswer("pandoc", "yours") }) as PickerDestination;
  const saved: unknown[] = [];
  const { remembered } = opened(() => destination, { picker_preferences_saved: ({ preferences }) => { saved.push((preferences as { key_output?: string }).key_output ?? (preferences as { output?: string }).output ?? null); destination = { ...destination, format: formatAnswer("typst", "yours") }; return null; } });
  await screen.findByText("First");
  await pick("Typst");
  await waitFor(() => expect(saved).toEqual(["typst"]));
  destination = fixtureDestination({ ...BROWSER, app: "Example Editor", app_id: "example.editor", format: formatAnswer("typst", "yours") });
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatShown()).toBe("Typst"));
  destination = fixtureDestination({ ...BROWSER, format: formatAnswer("cite", "app") });
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatShown()).toBe("LaTeX \\cite · Example Browser"));
  expect(remembered).toEqual([]);
});

test("the menu says why the next paste uses its format and where a pick goes", async () => {
  let destination: PickerDestination = WORD;
  opened(() => destination);
  await screen.findByText("First");
  const says = async (shown: string, note: string) => {
    await act(async () => { await emit("picker:shown"); });
    await waitFor(() => expect(formatShown()).toBe(shown));
    await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
    openFormats();
    const menu = await screen.findByRole("dialog");
    expect(screen.getByText(note).id).toBe(menu.getAttribute("aria-describedby"));
    fireEvent.keyDown(await screen.findByRole("combobox", { name: /^Search (formats|actions)$/ }), { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  };
  await says("APA · Example Word", "Suggested for Example Word. Your pick is remembered for this app.");
  destination = fixtureDestination({ ...WORD, format: formatAnswer("typst", "app") });
  await says("Typst · Example Word", "Remembered for Example Word. Your pick replaces it.");
  destination = BROWSER;
  await says("Pandoc", "Example Browser has no remembered format, so your default applies. Your pick is remembered for this app.");
  destination = fixtureDestination({ ...BROWSER, app: null, app_id: null, format: formatAnswer("pandoc", "yours") });
  await says("Pandoc", "Ibis can’t tell which app this is for, so your default applies. Your pick becomes your default.");
});

test("an unreadable per-app file says defaults apply and offers the app's own paste answer", async () => {
  const { calls } = opened(() => ({ ...WORD, unreadable: true, can_choose_paste: true }));
  await screen.findByText("First");
  expect((await screen.findByText(/App choices were damaged/)).textContent).toContain("Choose Paste or Copy Only again");
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  openFormats();
  expect(await screen.findByRole("option", { name: "Copy Only" })).toBeTruthy();
  expect(screen.getByRole("option", { name: "Paste into Example Word from Now On" })).toBeTruthy();
  fireEvent.keyDown(await screen.findByRole("combobox", { name: /^Search (formats|actions)$/ }), { key: "Escape" });
  expect(calls).not.toContain("remember_app_output");
});

test("an unread destination says what applies and never points at paste choices it cannot offer", async () => {
  const saved: unknown[] = [];
  const { remembered } = opened(() => { throw new Error("synthetic read failure"); }, { picker_preferences_saved: ({ preferences }) => { saved.push((preferences as { key_output?: string }).key_output ?? (preferences as { output?: string }).output ?? null); return null; } });
  await screen.findByText("First");
  expect((await screen.findByText(/could not tell which app this is\./)).textContent).toContain("Your default applies");
  expect(screen.queryByText(/Choose Paste or Copy Only/)).toBeNull();
  await pick("Typst");
  await waitFor(() => expect(formatShown()).toBe("Typst · This paste"));
  openFormats();
  expect(screen.getByText("Typst applies to this paste only.")).toBeTruthy();
  expect(saved).toEqual([]);
  expect(remembered).toEqual([]);
});

test("without a destination app format picks change the default", async () => {
  const saved: unknown[] = [];
  const { remembered } = opened(() => (fixtureDestination({ app: null, app_id: null, app_identity: null, unreadable: false, can_paste: false, paste: null, paste_question: false, can_choose_paste: false, format: formatAnswer("pandoc", "yours") })), {
    picker_preferences_saved: ({ preferences }) => { saved.push((preferences as { key_output?: string }).key_output ?? (preferences as { output?: string }).output ?? null); return null; },
  });
  await screen.findByText("First");
  await pick("LaTeX \\cite");
  await waitFor(() => expect(saved).toContain("cite"));
  expect(remembered).toEqual([]);
});

test("a successful copy consumes a held format even after the highlighted row changed", async () => {
  let finish!: (value: unknown) => void;
  const first = new Promise((done) => { finish = done; });
  const formats: string[] = [];
  opened(() => (fixtureDestination({ ...BROWSER, format: formatAnswer("cite", "app") })), {
    remember_app_output: refuseSave,
    search: () => [result("a", "First"), result("b", "Second")],
    copy_file_citation: (args) => {
      const preparedCopy = prepared(args);
      formats.push(preparedCopy.request.output);
      return formats.length === 1 ? first : { text: preparedCopy.preview.text, warnings: [] };
    },
  });
  await screen.findByText("First");
  await toggleKeepOpen();
  await pick("Typst");
  await waitFor(() => expect(formatShown()).toBe("Typst · This paste"));
  const search = screen.getByLabelText("Search references");
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(formats).toEqual(["typst"]));
  fireEvent.keyDown(search, { key: "ArrowDown" });
  await act(async () => { finish({ text: "synthetic typst copy", warnings: [] }); });
  await waitFor(() => expect(formatShown()).toBe("LaTeX \\cite · Example Browser"));
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(formats).toEqual(["typst", "cite"]));
});

test("one format shortcut opens the formats without copying or changing any default", async () => {
  const { calls, remembered } = opened(() => BROWSER);
  await screen.findByText("First");
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  expect(formatButton().getAttribute("aria-keyshortcuts")).toBe("Control+Shift+F");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "F", ctrlKey: true, shiftKey: true });
  const menu = await screen.findByRole("dialog");
  expect(within(menu).queryByRole("menuitemcheckbox")).toBeNull();
  expect(within(menu).getAllByRole("option").filter((row) => row.closest('[aria-label="Recent"], [aria-label="Key formats"], [aria-label="Citation styles"]'))).toHaveLength(OUTPUTS.length);
  expect(calls).not.toContain("copy_file_citation");
  expect(calls).not.toContain("picker_preferences_saved");
  expect(remembered).toEqual([]);
});

test("an unrelated Settings refresh does not extend a held format", async () => {
  let finish!: (value: unknown) => void;
  const copying = new Promise((done) => { finish = done; });
  const { calls } = opened(() => (fixtureDestination({ ...BROWSER, format: formatAnswer("cite", "app") })), { copy_file_citation: () => copying, remember_app_output: refuseSave });
  await screen.findByText("First");
  await toggleKeepOpen();
  await pick("Typst");
  await waitFor(() => expect(formatShown()).toBe("Typst · This paste"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(calls).toContain("copy_file_citation"));
  await act(async () => { await emit("preferences:changed"); });
  await waitFor(() => expect(calls.filter((call) => call === "picker_destination")).toHaveLength(2));
  await act(async () => { finish({ text: "synthetic typst copy", warnings: [] }); });
  await waitFor(() => expect(formatShown()).toBe("LaTeX \\cite · Example Browser"));
});

test("a failed preference read keeps the native app's authoritative automatic style", async () => {
  const { previews, copies } = opened(() => (fixtureDestination({ ...WORD, format: formatAnswer("ieee", "suggestion") })), {
    picker_preferences: () => { throw new Error("synthetic preference read failure"); },
  });
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("IEEE · Example Word"));
  await waitFor(() => expect(previews.at(-1)?.output).toBe("ieee"));
  await copy(previews, copies);
});

test("a new pick cancels an Enter that was waiting for another output's preview", async () => {
  let finish!: (value: unknown) => void;
  const typst = new Promise((done) => { finish = done; });
  const { copies } = opened(() => (fixtureDestination({ ...BROWSER, format: formatAnswer("cite", "app") })), {
    preview_file_citation: ({ output }) => output === "typst" ? typst : {
      ticket: `ticket-${output}`, content_hash: `hash-${output}`, text: `synthetic ${output}`, display: receipt(`synthetic ${output}`).display, warnings: [],
    },
  });
  await screen.findByText("First");
  await pick("Typst");
  const search = screen.getByLabelText("Search references");
  await waitFor(() => expect(formatShown()).toBe("Typst · Example Browser"));
  fireEvent.keyDown(search, { key: "Enter" });
  await pick("Pandoc");
  await waitFor(() => expect(formatShown()).toBe("Pandoc · Example Browser"));
  await act(async () => { finish({ ticket: "old-typst", content_hash: "old-typst", text: "old Typst", display: receipt("old Typst").display, warnings: [] }); });
  expect(copies).toEqual([]);
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(copies).toEqual([{ ticket: "ticket-pandoc", contentHash: "hash-pandoc" }]));
});

// First citation accepts the native suggestion without an onboarding question.
test("the first citation copies its suggested preview without asking", async () => {
  const { previews, copies, remembered } = opened(() => fixtureDestination({ ...BROWSER, format: formatAnswer("pandoc", "yours") }));
  await screen.findByText("First");
  await copy(previews, copies);
  expect(screen.queryByRole("menu")).toBeNull();
  expect(remembered).toEqual([]);
});
