import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { PickerDestination } from "./api";
import { isStyle, listedOutput } from "./format/outputs";
import { OUTPUTS } from "./bindings";
import { backend, prepared, receipt, resetApp, result, type PreviewRequest } from "./testing/app";

afterEach(resetApp);
const WORD: PickerDestination = { app: "Example Word", app_id: "example.word", app_identity: null, automatic: "apa", remembered: null, unreadable: false, can_paste: false, output_question: null, paste: null, paste_question: false, can_choose_paste: false };
const BROWSER: PickerDestination = { app: "Example Browser", app_id: "example.browser", app_identity: null, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: "browser", paste: null, paste_question: false, can_choose_paste: false };
const formatButton = () => within(screen.getByLabelText("Search references").closest("header")!).getByRole("button", { name: /^Citation format / });
const formatShown = () => formatButton().getAttribute("aria-label")!.slice("Citation format ".length);
const openFormats = () => { if (formatButton().getAttribute("aria-expanded") !== "true") fireEvent.click(formatButton()); };
async function pick(name: string) {
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  openFormats();
  fireEvent.click(screen.getByRole("menuitemradio", { name }));
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
      return { ticket: `ticket-${number}`, content_hash: `hash-${number}`, text: isStyle(listedOutput(request.output) ?? "pandoc") ? `(Smith, 2020) ${request.output}` : `synthetic ${request.output}`, html: null, warnings: [] };
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

test.each(["browser", "editor"] as const)("the first %s paste asks, remembers the pick, and copies its exact preview once", async (kind) => {
  let destination: PickerDestination = { ...BROWSER, output_question: kind };
  const { previews, copies, remembered } = opened(() => destination);
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByRole("menuitemradio", { name: "APA" });
  expect(copies).toEqual([]);
  fireEvent.click(screen.getByRole("menuitemradio", { name: "APA" }));
  await waitFor(() => expect(remembered).toEqual(["apa"]));
  await waitFor(() => expect(formatShown()).toBe("APA · Example Browser"));
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(previews.at(-1)?.output).toBe("apa");
  expect(copies[0]).toEqual({ ticket: `ticket-${previews.length}`, contentHash: `hash-${previews.length}` });
  destination = { ...BROWSER, remembered: "apa" };
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(2));
  expect(screen.queryByRole("menu")).toBeNull();
});

test.each(["Escape", "Tab"])("%s uses the default for this paste and asks again next time", async (key) => {
  const { copies, remembered } = opened(() => BROWSER);
  await screen.findByText("First");
  fireEvent.click(screen.getByRole("button", { name: "Keep Window Open" }));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByRole("menu");
  expect(copies).toEqual([]);
  fireEvent.keyDown(screen.getByRole("searchbox"), { key });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(prepared(copies[0]).request.output).toBe("pandoc");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByRole("menu");
  expect(copies).toHaveLength(1);
  expect(remembered).toEqual([]);
});

test.each(["query", "hidden", "reopened", "command"])("a %s change cancels the citation held by the question", async (change) => {
  const { copies } = opened(() => BROWSER);
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByRole("menu");
  if (change === "query") fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "another" } });
  else if (change === "command") fireEvent.keyDown(screen.getByRole("searchbox"), { key: "k", ctrlKey: true });
  else await act(async () => { await emit(change === "hidden" ? "picker:hidden" : "picker:shown"); });
  if (change !== "command") {
    const item = screen.queryByRole("menuitemradio", { name: "APA" });
    if (item) fireEvent.click(item);
  } else {
    const more = await screen.findByRole("menu", { name: /^More/ });
    fireEvent.keyDown(more, { key: "Escape" });
  }
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  await act(async () => { await new Promise(requestAnimationFrame); });
  expect(copies).toEqual([]);
});

test("a first-paste pick waits for its save; failure copies the choice once and asks again", async () => {
  let fail!: () => void;
  const saving = new Promise<null>((_, reject) => { fail = () => reject(new Error("synthetic save failure")); });
  const { copies } = opened(() => BROWSER, { remember_app_output: () => saving });
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  fireEvent.click(await screen.findByRole("menuitemradio", { name: "APA" }));
  expect(copies).toEqual([]);
  expect(formatShown()).toBe("APA · Example Browser");
  await act(async () => fail());
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(prepared(copies[0]).request.output).toBe("apa");
  await act(async () => { await emit("picker:shown"); });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByRole("menu");
});

test.each(["footer", "double-click"])("%s asks through the same first-paste chooser", async (action) => {
  const { copies } = opened(() => BROWSER);
  await screen.findByText("First");
  if (action === "footer") fireEvent.click(within(screen.getByRole("group", { name: "Keys" })).getByRole("button", { name: "Copy as Pandoc" }));
  else fireEvent.click(screen.getByText("First"), { detail: 2 });
  await screen.findByRole("menuitemradio", { name: "Pandoc" });
  expect(copies).toEqual([]);
});

test("an Enter during a slow destination read waits and then asks", async () => {
  let finish!: (destination: PickerDestination) => void;
  const reading = new Promise<PickerDestination>((resolve) => { finish = resolve; });
  const { copies } = opened(() => BROWSER, { picker_destination: () => reading });
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(copies).toEqual([]);
  expect(screen.queryByRole("menu")).toBeNull();
  await act(async () => finish(BROWSER));
  await screen.findByRole("menu");
  expect(copies).toEqual([]);
});

test("Enter during a browser search waits, asks, then copies the chosen format once", async () => {
  let finish!: (results: ReturnType<typeof result>[]) => void;
  const searching = new Promise<ReturnType<typeof result>[]>((resolve) => { finish = resolve; });
  const { calls, copies, remembered } = opened(() => BROWSER, { search: () => searching });
  // Blank queries load recents; type a query to exercise an actual search.
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "First" } });
  await waitFor(() => expect(calls).toContain("search"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(screen.queryByRole("menu")).toBeNull();
  expect(copies).toEqual([]);
  await act(async () => finish([result("a", "First")]));
  fireEvent.click(await screen.findByRole("menuitemradio", { name: "APA" }));
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(prepared(copies[0]).request).toMatchObject({ output: "apa", items: [{ id: { key: "a" } }] });
  expect(remembered).toEqual(["apa"]);
  expect(calls.filter((call) => call === "show_copied_notice")).toHaveLength(1);
});

test("the question appears even when the global key preview fails, then waits for the chosen style's preview", async () => {
  let finish!: (preview: unknown) => void;
  const rendering = new Promise((resolve) => { finish = resolve; });
  const { copies } = opened(() => BROWSER, { preview_file_citation: ({ output }) => {
    if (!isStyle(listedOutput(output as string) ?? "pandoc")) throw new Error("synthetic key unavailable");
    return rendering;
  } });
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  fireEvent.click(await screen.findByRole("menuitemradio", { name: "APA" }));
  await waitFor(() => expect(formatShown()).toBe("APA · Example Browser"));
  expect(copies).toEqual([]);
  await act(async () => finish({ ticket: "style-ticket", content_hash: "style-hash", text: "Synthetic APA", html: null, warnings: [] }));
  await waitFor(() => expect(copies).toEqual([{ ticket: "style-ticket", contentHash: "style-hash" }]));
});

test("hiding while a first choice saves cancels its pending copy", async () => {
  let finish!: () => void;
  const saving = new Promise<null>((resolve) => { finish = () => resolve(null); });
  const { copies } = opened(() => BROWSER, { remember_app_output: () => saving });
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  fireEvent.click(await screen.findByRole("menuitemradio", { name: "APA" }));
  await act(async () => { await emit("picker:hidden"); finish(); });
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  expect(copies).toEqual([]);
});

test.each(["More", "Keyboard Shortcuts"])("opening %s cancels a dismissed question waiting for its preview", async (overlay) => {
  let finish!: (preview: ReturnType<typeof receipt>) => void;
  const rendering = new Promise<ReturnType<typeof receipt>>((resolve) => { finish = resolve; });
  const { copies, calls } = opened(() => BROWSER, { preview_file_citation: () => rendering });
  await screen.findByText("First");
  await waitFor(() => expect(calls).toContain("preview_file_citation"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  fireEvent.keyDown(await screen.findByRole("searchbox"), { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  if (overlay === "More") fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  else fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "/", ctrlKey: true });
  const role = overlay === "More" ? "menu" : "dialog";
  await screen.findByRole(role, { name: new RegExp(`^${overlay}`) });
  await act(async () => finish(receipt("Synthetic delayed preview")));
  expect(copies).toEqual([]);
  expect(calls).not.toContain("hide_picker");
  fireEvent.keyDown(screen.getByRole(role), { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole(role)).toBeNull());
  expect(copies).toEqual([]);
});

test.each(["More", "Keyboard Shortcuts"])("opening %s cancels a first-paste pick waiting for its save", async (overlay) => {
  let finish!: () => void;
  const saving = new Promise<null>((resolve) => { finish = () => resolve(null); });
  const { copies, calls } = opened(() => BROWSER, { remember_app_output: () => saving });
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  fireEvent.click(await screen.findByRole("menuitemradio", { name: "APA" }));
  await waitFor(() => expect(calls).toContain("remember_app_output"));
  const button = formatButton();
  if (overlay === "More") fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  else fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "/", ctrlKey: true });
  const role = overlay === "More" ? "menu" : "dialog";
  await screen.findByRole(role, { name: new RegExp(`^${overlay}`) });
  await act(async () => finish());
  await waitFor(() => expect(button.getAttribute("aria-label")).toBe("Citation format APA · Example Browser"));
  expect(copies).toEqual([]);
  expect(calls).not.toContain("hide_picker");
  fireEvent.keyDown(screen.getByRole(role), { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole(role)).toBeNull());
  expect(copies).toEqual([]);
});

test("opened from a word processor the preview is APA and copy uses its exact receipt", async () => {
  const { previews, copies } = opened(() => WORD);
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("APA · Example Word"));
  await waitFor(() => expect(previews.at(-1)?.output).toBe("apa"));
  await copy(previews, copies);
});

test.each([
  ["cite", "LaTeX \\cite"], ["pandoc", "Pandoc"], ["typst", "Typst"],
] as const)("a browser-hosted key workflow uses the chosen global %s default and exact preview", async (format, name) => {
  const { previews, copies } = opened(() => BROWSER, { picker_preferences: () => ({ output: format }) });
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe(`${name}`));
  await waitFor(() => expect(previews.at(-1)).toMatchObject({ output: format }));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByRole("menuitemradio", { name: "APA" });
  expect(copies).toEqual([]);
  fireEvent.keyDown(screen.getByRole("searchbox"), { key: "Escape" });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(prepared(copies[0]).request).toMatchObject({ output: format });
});

test("a format pick is remembered for the browser, and a later pick replaces it", async () => {
  let destination = BROWSER;
  const { calls, remembered } = opened(() => destination);
  await screen.findByText("First");
  await pick("LaTeX \\cite");
  await waitFor(() => expect(remembered).toEqual(["cite"]));
  destination = { ...BROWSER, remembered: "cite" };
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatShown()).toBe("LaTeX \\cite · Example Browser"));
  await pick("Typst");
  await waitFor(() => expect(remembered).toEqual(["cite", "typst"]));
  await waitFor(() => expect(formatShown()).toBe("Typst · Example Browser"));
  expect(calls).not.toContain("picker_preferences_saved");
});

test("a pick that cannot be saved says it is for this paste, then the saved format returns", async () => {
  const { previews, copies, calls } = opened(() => ({ ...BROWSER, remembered: "cite" }), { remember_app_output: refuseSave });
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("LaTeX \\cite · Example Browser"));
  await pick("Typst");
  await waitFor(() => expect(formatShown()).toBe("Typst · This paste"));
  expect((await screen.findByText(/could not save the choice for this app/)).getAttribute("role")).toBe("alert");
  openFormats();
  expect(screen.getByText("Typst applies to this paste only. Pick a format to remember it for this app.")).toBeTruthy();
  fireEvent.keyDown(await screen.findByRole("searchbox"), { key: "Escape" });
  await waitFor(() => expect(previews.at(-1)).toMatchObject({ output: "typst" }));
  await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  await copy(previews, copies);
  await waitFor(() => expect(formatShown()).toBe("LaTeX \\cite · Example Browser"));
  expect(calls).not.toContain("picker_preferences_saved");
});

test("a pasted list copies every result in order in the app's format, then hides", async () => {
  const { previews, copies, calls, remembered } = opened(() => ({ ...BROWSER, remembered: "cite" }), {
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
  let destination = { ...BROWSER, app: null, app_id: null, output_question: null } as PickerDestination;
  const saved: unknown[] = [];
  const { remembered } = opened(() => destination, { picker_preferences_saved: ({ preferences }) => { saved.push((preferences as { output?: string }).output ?? null); return null; } });
  await screen.findByText("First");
  await pick("Typst");
  await waitFor(() => expect(saved).toEqual(["typst"]));
  destination = { ...BROWSER, app: "Example Editor", app_id: "example.editor" };
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatShown()).toBe("Typst"));
  destination = { ...BROWSER, remembered: "cite" };
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
    const menu = await screen.findByRole("menu");
    expect(screen.getByText(note).id).toBe(menu.getAttribute("aria-describedby"));
    fireEvent.keyDown(await screen.findByRole("searchbox"), { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  };
  await says("APA · Example Word", "Suggested for Example Word. Your pick is remembered for this app.");
  destination = { ...WORD, remembered: "typst" };
  await says("Typst · Example Word", "Remembered for Example Word. Your pick replaces it.");
  destination = BROWSER;
  await says("Pandoc", "Example Browser has no remembered format, so your default applies. Your pick is remembered for this app.");
  destination = { ...BROWSER, app: null, app_id: null, output_question: null };
  await says("Pandoc", "Ibis can’t tell which app this is for, so your default applies. Your pick becomes your default.");
});

test("an unreadable per-app file says defaults apply and offers the app's own paste answer", async () => {
  const { calls } = opened(() => ({ ...WORD, unreadable: true, can_choose_paste: true }));
  await screen.findByText("First");
  expect((await screen.findByText(/saved app choices were damaged/)).textContent).toContain("Choose Paste or Copy Only in the format menu");
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  openFormats();
  expect(await screen.findByRole("menuitem", { name: "Copy Only" })).toBeTruthy();
  expect(screen.getByRole("menuitem", { name: "Paste into Example Word from Now On" })).toBeTruthy();
  fireEvent.keyDown(await screen.findByRole("searchbox"), { key: "Escape" });
  expect(calls).not.toContain("remember_app_output");
});

test.each([["the open first-paste question", true], ["the format menu with nothing asked", false]])("answering Copy Only in %s saves a format only for the open question", async (_where, asked) => {
  const { remembered } = opened(() => ({ ...BROWSER, can_choose_paste: true }));
  await screen.findByText("First");
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  if (asked) fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  else openFormats();
  fireEvent.click(await screen.findByRole("menuitem", { name: "Copy Only" }));
  await waitFor(() => expect(remembered).toEqual([asked ? "pandoc" : null]));
});

test("a first-paste answer whose paste choice cannot be saved says the format was kept", async () => {
  opened(() => ({ ...BROWSER, can_choose_paste: true }), { remember_app_output: () => Promise.reject({ code: "delivery_not_saved" }) });
  await screen.findByText("First");
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  fireEvent.click(await screen.findByRole("menuitem", { name: "Copy Only" }));
  expect((await screen.findByText(/saved the format for this app, but not whether to paste into it/)).textContent).toContain("Choose Paste or Copy Only again");
});

test("an unread destination says what applies and never points at paste choices it cannot offer", async () => {
  const saved: unknown[] = [];
  const { remembered } = opened(() => { throw new Error("synthetic read failure"); }, { picker_preferences_saved: ({ preferences }) => { saved.push((preferences as { output?: string }).output ?? null); return null; } });
  await screen.findByText("First");
  expect((await screen.findByText(/could not tell which app this is for/)).textContent).toContain("Reopen Ibis to try again");
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
  const { remembered } = opened(() => ({ app: null, app_id: null, app_identity: null, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null, paste: null, paste_question: false, can_choose_paste: false }), {
    picker_preferences_saved: ({ preferences }) => { saved.push((preferences as { output?: string }).output ?? null); return null; },
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
  opened(() => ({ ...BROWSER, remembered: "cite" }), {
    remember_app_output: refuseSave,
    search: () => [result("a", "First"), result("b", "Second")],
    copy_file_citation: (args) => {
      const preparedCopy = prepared(args);
      formats.push(preparedCopy.request.output);
      return formats.length === 1 ? first : { text: preparedCopy.preview.text, warnings: [] };
    },
  });
  await screen.findByText("First");
  fireEvent.click(screen.getByRole("button", { name: "Keep Window Open" }));
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
  const menu = await screen.findByRole("menu");
  expect(within(menu).queryByRole("menuitemcheckbox")).toBeNull();
  expect(within(menu).getAllByRole("menuitemradio")).toHaveLength(OUTPUTS.length);
  expect(calls).not.toContain("copy_file_citation");
  expect(calls).not.toContain("picker_preferences_saved");
  expect(remembered).toEqual([]);
});

test("an unrelated Settings refresh does not extend a held format", async () => {
  let finish!: (value: unknown) => void;
  const copying = new Promise((done) => { finish = done; });
  const { calls } = opened(() => ({ ...BROWSER, remembered: "cite" }), { copy_file_citation: () => copying, remember_app_output: refuseSave });
  await screen.findByText("First");
  fireEvent.click(screen.getByRole("button", { name: "Keep Window Open" }));
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
  const { previews, copies } = opened(() => ({ ...WORD, automatic: "ieee" }), {
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
  const { copies } = opened(() => ({ ...BROWSER, remembered: "cite" }), {
    preview_file_citation: ({ output }) => output === "typst" ? typst : {
      ticket: `ticket-${output}`, content_hash: `hash-${output}`, text: `synthetic ${output}`, html: null, warnings: [],
    },
  });
  await screen.findByText("First");
  await pick("Typst");
  const search = screen.getByLabelText("Search references");
  await waitFor(() => expect(formatShown()).toBe("Typst · Example Browser"));
  fireEvent.keyDown(search, { key: "Enter" });
  await pick("Pandoc");
  await waitFor(() => expect(formatShown()).toBe("Pandoc · Example Browser"));
  await act(async () => { finish({ ticket: "old-typst", content_hash: "old-typst", text: "old Typst", html: null, warnings: [] }); });
  expect(copies).toEqual([]);
  fireEvent.keyDown(search, { key: "Enter" });
  await waitFor(() => expect(copies).toEqual([{ ticket: "ticket-pandoc", contentHash: "hash-pandoc" }]));
});
