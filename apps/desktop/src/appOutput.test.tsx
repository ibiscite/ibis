import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { AppOutput, PickerDestination } from "./api";
import { backend, chooseFromMore, prepared, receipt, resetApp, result, type PreviewRequest } from "./testing/app";

afterEach(resetApp);
const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const WORD: PickerDestination = { app: "Example Word", app_id: "example.word", automatic: { style: "apa" }, remembered: null, unreadable: false, can_paste: false, output_question: null };
const BROWSER: PickerDestination = { app: "Example Browser", app_id: "example.browser", automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: "browser" };
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

test.each(["browser", "editor"] as const)("the first %s paste asks, remembers the pick, and copies its exact preview once", async (kind) => {
  let destination: PickerDestination = { ...BROWSER, output_question: kind };
  const { previews, copies, remembered } = opened(() => destination);
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByRole("menuitemradio", { name: "APA · text" });
  expect(copies).toEqual([]);
  fireEvent.click(screen.getByRole("menuitemradio", { name: "APA · text" }));
  await waitFor(() => expect(remembered).toEqual([{ style: "apa" }]));
  await waitFor(() => expect(formatShown()).toBe(`APA remembered for this ${kind}`));
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(previews.at(-1)?.style).toBe("apa");
  expect(copies[0]).toEqual({ ticket: `ticket-${previews.length}`, contentHash: `hash-${previews.length}` });
  destination = { ...BROWSER, remembered: { style: "apa" } };
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(copies).toHaveLength(2));
  expect(screen.queryByRole("menu")).toBeNull();
});

test.each(["Escape", "Tab"])("%s uses the global default for this paste and asks again next time", async (key) => {
  const { copies, remembered } = opened(() => BROWSER);
  await screen.findByText("First");
  fireEvent.click(screen.getByRole("button", { name: "Keep Window Open" }));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByRole("menu");
  expect(copies).toEqual([]);
  fireEvent.keyDown(screen.getByRole("searchbox"), { key });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(prepared(copies[0]).request.format).toBe("pandoc");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByRole("menu");
  expect(copies).toHaveLength(1);
  expect(remembered).toEqual([]);
});

test("Just this once keeps the first question open for a pick without saving it", async () => {
  const { copies, remembered } = opened(() => BROWSER);
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "Just this once" }));
  expect(await screen.findByRole("menuitemcheckbox", { name: "Just this once", checked: true })).toBeTruthy();
  fireEvent.click(screen.getByRole("menuitemradio", { name: "APA · text" }));
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(prepared(copies[0]).request.style).toBe("apa");
  expect(remembered).toEqual([]);
  await act(async () => { await emit("picker:shown"); });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByRole("menu");
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
    const item = screen.queryByRole("menuitemradio", { name: "APA · text" });
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
  fireEvent.click(await screen.findByRole("menuitemradio", { name: "APA · text" }));
  expect(copies).toEqual([]);
  expect(formatShown()).toBe("APA for this citation");
  await act(async () => fail());
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(prepared(copies[0]).request.style).toBe("apa");
  await act(async () => { await emit("picker:shown"); });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByRole("menu");
});

test.each(["footer", "double-click"])("%s asks through the same first-paste chooser", async (action) => {
  const { copies } = opened(() => BROWSER);
  await screen.findByText("First");
  if (action === "footer") fireEvent.click(within(screen.getByRole("group", { name: "Keys" })).getByRole("button", { name: "Copy as Pandoc" }));
  else fireEvent.click(screen.getByText("First"), { detail: 2 });
  await screen.findByRole("menuitemradio", { name: "Pandoc key" });
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
  await waitFor(() => expect(calls).toContain("search"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(screen.queryByRole("menu")).toBeNull();
  expect(copies).toEqual([]);
  await act(async () => finish([result("a", "First")]));
  fireEvent.click(await screen.findByRole("menuitemradio", { name: "APA · text" }));
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(prepared(copies[0]).request).toMatchObject({ style: "apa", items: [{ id: { key: "a" } }] });
  expect(remembered).toEqual([{ style: "apa" }]);
  expect(calls.filter((call) => call === "show_copied_notice")).toHaveLength(1);
});

test("the question appears even when the global key preview fails, then waits for the chosen style's preview", async () => {
  let finish!: (preview: unknown) => void;
  const rendering = new Promise((resolve) => { finish = resolve; });
  const { copies } = opened(() => BROWSER, { preview_file_citation: ({ style }) => {
    if (!style) throw new Error("synthetic key unavailable");
    return rendering;
  } });
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  fireEvent.click(await screen.findByRole("menuitemradio", { name: "APA · text" }));
  await waitFor(() => expect(formatShown()).toBe("APA remembered for this browser"));
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
  fireEvent.click(await screen.findByRole("menuitemradio", { name: "APA · text" }));
  await act(async () => { await emit("picker:hidden"); finish(); });
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  expect(copies).toEqual([]);
});

test.each(["More", "Keyboard shortcuts"])("opening %s cancels a dismissed question waiting for its preview", async (overlay) => {
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

test.each(["More", "Keyboard shortcuts"])("opening %s cancels a first-paste pick waiting for its save", async (overlay) => {
  let finish!: () => void;
  const saving = new Promise<null>((resolve) => { finish = () => resolve(null); });
  const { copies, calls } = opened(() => BROWSER, { remember_app_output: () => saving });
  await screen.findByText("First");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  fireEvent.click(await screen.findByRole("menuitemradio", { name: "APA · text" }));
  await waitFor(() => expect(calls).toContain("remember_app_output"));
  const button = formatButton();
  if (overlay === "More") fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  else fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "/", ctrlKey: true });
  const role = overlay === "More" ? "menu" : "dialog";
  await screen.findByRole(role, { name: new RegExp(`^${overlay}`) });
  await act(async () => finish());
  await waitFor(() => expect(button.getAttribute("aria-label")).toBe("Copy as APA remembered for this browser"));
  expect(copies).toEqual([]);
  expect(calls).not.toContain("hide_picker");
  fireEvent.keyDown(screen.getByRole(role), { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole(role)).toBeNull());
  expect(copies).toEqual([]);
});

test("opened from a word processor the preview is APA and copy uses its exact receipt", async () => {
  const { previews, copies } = opened(() => WORD);
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("APA · text · Example Word"));
  await waitFor(() => expect(previews.at(-1)?.style).toBe("apa"));
  await copy(previews, copies);
});

test.each([
  ["cite", "\\cite"], ["pandoc", "Pandoc key"], ["typst", "Typst key"],
] as const)("a browser-hosted key workflow uses the chosen global %s default and exact preview", async (format, name) => {
  const { previews, copies } = opened(() => BROWSER, { copy_format: () => format });
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe(`${name} · Example Browser`));
  await waitFor(() => expect(previews.at(-1)).toMatchObject({ style: null, format }));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await screen.findByRole("menuitemradio", { name: "APA · text" });
  expect(copies).toEqual([]);
  fireEvent.keyDown(screen.getByRole("searchbox"), { key: "Escape" });
  await waitFor(() => expect(copies).toHaveLength(1));
  expect(prepared(copies[0]).request).toMatchObject({ format, style: null });
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
  await waitFor(() => expect(formatShown()).toBe("Pandoc key · Example Browser"));
  expect(calls).not.toContain("set_picker_preferences");
});

test("Just this once overrides a remembered app output for one copy without saving either default", async () => {
  const { previews, copies, calls, remembered } = opened(() => ({ ...BROWSER, remembered: { key: "cite" } }));
  await screen.findByText("First");
  await waitFor(() => expect(formatShown()).toBe("\\cite · Example Browser"));
  await pick("Just this once", "menuitemcheckbox");
  await pick("Typst key");
  await waitFor(() => expect(previews.at(-1)).toMatchObject({ style: null, format: "typst" }));
  await copy(previews, copies);
  await waitFor(() => expect(formatShown()).toBe("\\cite · Example Browser"));
  expect(remembered).toEqual([]);
  expect(calls).not.toContain("set_picker_preferences");
});

test("a pasted list consumes a one-time format and hides after copying every result in order", async () => {
  const { previews, copies, calls, remembered } = opened(() => ({ ...BROWSER, remembered: { key: "cite" } }), {
    search: () => ({ revision: "1", results: [result("b", "Second"), result("a", "First")], list: { missing: [] } }),
  });
  await screen.findByText("Pasted list");
  await pick("Just this once", "menuitemcheckbox");
  await pick("Typst key");
  await waitFor(() => expect(previews.at(-1)).toMatchObject({ format: "typst", items: [{ id: { key: "b" } }, { id: { key: "a" } }] }));
  await copy(previews, copies);
  await waitFor(() => expect(formatShown()).toBe("\\cite · Example Browser"));
  await waitFor(() => expect(calls).toContain("hide_picker"));
  expect(remembered).toEqual([]);
});

test("deliberately setting a global default preserves another app's remembered choice", async () => {
  let destination = { ...BROWSER, remembered: { key: "cite" } } as PickerDestination;
  const saved: unknown[] = [];
  const { remembered } = opened(() => destination, { set_picker_preferences: ({ copyFormat }) => { saved.push(copyFormat); return null; } });
  await screen.findByText("First");
  await pick("Just this once", "menuitemcheckbox");
  await pick("Typst key");
  await pick("Use as global default", "menuitem");
  await waitFor(() => expect(saved).toEqual(["typst"]));
  destination = { ...BROWSER, app: "Example Editor", app_id: "example.editor" };
  await act(async () => { await emit("picker:shown"); });
  await waitFor(() => expect(formatShown()).toBe("Typst key · Example Editor"));
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
  opened(() => ({ app: null, app_id: null, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null }));
  await screen.findByText("First");
  expect(formatShown()).toBe("Pandoc key");
  await pick("Just this once", "menuitemcheckbox");
  openFormats();
  expect(screen.queryByRole("menuitem", { name: "Automatic" })).toBeNull();
});

test("without a destination app format picks change the global default, as both README claims explain", async () => {
  const saved: unknown[] = [];
  const { remembered } = opened(() => ({ app: null, app_id: null, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null }), {
    set_picker_preferences: ({ copyFormat }) => { saved.push(copyFormat); return null; },
  });
  await screen.findByText("First");
  await pick("\\cite");
  await waitFor(() => expect(saved).toContain("cite"));
  expect(remembered).toEqual([]);
  const readme = readFileSync(new FileURL("../../../README.md", import.meta.url), "utf8");
  const claims = readme.split("\n").filter((line) => line.startsWith("- Cite with") || line.startsWith("1. Press the shortcut."));
  expect(claims).toHaveLength(2);
  for (const claim of claims) {
    expect(claim).toContain("On macOS,");
    expect(claim.toLowerCase()).toContain("on windows and linux, choosing a format changes the global default");
  }
});

test("Automatic in a formatted app uses the global style just chosen in this picker", async () => {
  const { remembered } = opened(() => WORD);
  await screen.findByText("First");
  await pick("IEEE · reference entry");
  await waitFor(() => expect(remembered).toEqual([{ style: "ieee" }]));
  await pick("Use as global default", "menuitem");
  await pick("Automatic", "menuitem");
  await waitFor(() => expect(remembered).toEqual([{ style: "ieee" }, null]));
  await waitFor(() => expect(formatShown()).toBe("IEEE · reference entry · Example Word"));
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
  await pick("Typst key");
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
  await pick("Typst key");
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
  await waitFor(() => expect(formatShown()).toBe("IEEE · reference entry · Example Word"));
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
  await pick("Typst key");
  const search = screen.getByLabelText("Search references");
  fireEvent.keyDown(search, { key: "Enter" });
  await pick("Automatic", "menuitem");
  await waitFor(() => expect(formatShown()).toBe("Pandoc key · Example Browser"));
  await act(async () => { finish({ ticket: "old-typst", content_hash: "old-typst", text: "old Typst", html: null, warnings: [] }); });
  expect(copies).toEqual([]);
  fireEvent.keyDown(search, { key: "Enter" });
  await screen.findByRole("menu");
  expect(copies).toEqual([]);
  fireEvent.keyDown(screen.getByRole("searchbox"), { key: "Escape" });
  await waitFor(() => expect(copies).toEqual([{ ticket: "ticket-pandoc", contentHash: "hash-pandoc" }]));
});
