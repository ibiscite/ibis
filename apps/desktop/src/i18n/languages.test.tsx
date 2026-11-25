import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { parse } from "@fluent/syntax";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { App } from "../App";
import { demoApi } from "../settings/demo";
import { SettingsApp } from "../settings/SettingsApp";
import { backend, resetApp, result } from "../testing/app";
import { deliveryHarness } from "../testing/delivery";
import { OUTPUTS } from "../bindings";
import { InterfaceMessagesProvider, LANGUAGES, setInterfaceLanguage, type InterfaceLanguage } from ".";
import { OUTPUT_DESCRIPTIONS, OUTPUT_NAMES } from "./outputs";

const TRANSLATED: InterfaceLanguage[] = ["de", "ar"];
afterEach(async () => { await resetApp(); setInterfaceLanguage("en-US"); });

/** Every English sentence without placeables, by id, to catch one left untranslated. */
const english = new Map<string, string>();
for (const entry of parse(LANGUAGES["en-US"].source, { withSpans: false }).body) {
  if (entry.type !== "Message" || !entry.value || entry.value.elements.length !== 1) continue;
  const [element] = entry.value.elements;
  if (element?.type === "TextElement") english.set(entry.id.name, element.value.trim());
}

/** Visible words and accessible names in the window that are still English. */
function untranslated(language: InterfaceLanguage) {
  const translated = new Map<string, string>();
  for (const entry of parse(LANGUAGES[language].source, { withSpans: false }).body) {
    if (entry.type === "Message" && entry.value?.elements.length === 1 && entry.value.elements[0]!.type === "TextElement") {
      translated.set(entry.id.name, (entry.value.elements[0] as { value: string }).value.trim());
    }
  }
  // Words the same in both (names, "Ibis", "OK") are not English left behind.
  const left = new Set([...english].filter(([id, text]) => translated.get(id) !== text && /[a-z]{3}/.test(text)).map(([, text]) => text));
  const said = [...document.body.querySelectorAll("*")].flatMap((node) => [
    ...[...node.childNodes].filter((child) => child.nodeType === Node.TEXT_NODE).map((child) => child.textContent ?? ""),
    ...["aria-label", "title", "placeholder"].map((name) => node.getAttribute(name) ?? ""),
  ]).map((text) => text.replace(/[⁨⁩]/g, "").trim()).filter(Boolean);
  return [...new Set(said.filter((text) => left.has(text)))];
}

test.each(TRANSLATED)("Settings › General speaks %s and takes its direction", async (language) => {
  const messages = setInterfaceLanguage(language);
  render(<InterfaceMessagesProvider messages={messages}><SettingsApp api={demoApi({ speaking: language })} initialPage="general" /></InterfaceMessagesProvider>);
  expect(await screen.findByRole("heading", { name: messages.text("settings-general") })).toBeTruthy();
  expect(await screen.findByText(messages.text("general-language"))).toBeTruthy();
  expect(document.documentElement).toMatchObject({ lang: language, dir: LANGUAGES[language].direction });
  expect(untranslated(language)).toEqual([]);
});

test.each(TRANSLATED)("the picker speaks %s, its results and footer included", async (language) => {
  const messages = setInterfaceLanguage(language);
  backend({ search: () => [result("a", "First invented reference")] });
  await act(async () => { render(<InterfaceMessagesProvider messages={messages}><App /></InterfaceMessagesProvider>); });
  const search = screen.getByRole("combobox", { name: messages.text("picker-search-label") });
  await act(async () => { fireEvent.change(search, { target: { value: "first" } }); });
  // A Latin title keeps its own direction inside the Arabic window.
  const title = await screen.findByText("First invented reference");
  expect(title.closest("[dir='auto'], bdi")).toBeTruthy();
  expect(untranslated(language)).toEqual([]);
});

test("what is copied and pasted is the same in every interface language", async () => {
  vi.useFakeTimers();
  const copies = [];
  for (const language of ["en-US", ...TRANSLATED] as InterfaceLanguage[]) {
    const messages = setInterfaceLanguage(language);
    const harness = deliveryHarness();
    await act(async () => { render(<InterfaceMessagesProvider messages={messages}><App /></InterfaceMessagesProvider>); });
    await act(async () => { harness.search.pending[0]!.complete({ revision: "1", results: [] }); });
    const search = screen.getByRole("combobox", { name: messages.text("picker-search-label") });
    await act(async () => { fireEvent.change(search, { target: { value: "first" } }); });
    const job = harness.search.pending.find((pending) => pending.args.text === "first")!;
    await act(async () => { job.complete({ revision: "1", results: [result("a", "First invented reference")] }); });
    await act(async () => { fireEvent.keyDown(search, { key: "Enter" }); });
    const preview = harness.preview.pending.at(-1)!;
    // What Rust is asked to format never carries the interface language.
    const { items, output } = preview.args;
    const asked = JSON.stringify({ items, output });
    await act(async () => { preview.complete({ text: "(Aster, 2026, p. 12)", clipboardHtml: "<i>Aster</i> 2026" }); });
    await act(async () => { harness.copy.pending[0]!.complete(); });
    await act(async () => { harness.delivery.pending[0]!.complete(); });
    await act(async () => { harness.notice.pending[0]!.complete(); });
    copies.push({ asked, clipboard: harness.clipboard, pasted: harness.document.text });
    expect(within(harness.receipt).getByRole("status").querySelector("code")?.textContent).toBe("(Aster, 2026, p. 12)");
    await resetApp();
  }
  expect(copies[0]!.pasted).toBe("Draft: (Aster, 2026, p. 12).");
  for (const copy of copies.slice(1)) expect(copy).toEqual(copies[0]);
  vi.useRealTimers();
});

beforeEach(() => { localStorage.clear(); });

test.each(TRANSLATED)("Settings › Citations names its key formats in %s", async (language) => {
  // Sol's probe: the key-format menu said "Keys only" in German and Arabic.
  const messages = setInterfaceLanguage(language);
  render(<InterfaceMessagesProvider messages={messages}><SettingsApp api={demoApi({ speaking: language })} initialPage="citations" /></InterfaceMessagesProvider>);
  fireEvent.click(await screen.findByRole("button", { name: new RegExp(`^${messages.text("citations-keys")}`) }));
  const menu = await screen.findByRole("menu");
  const strip = (text: string) => text.replace(/[⁨⁩]/g, "");
  expect(within(menu).getByRole("menuitemradio", { name: (name) => strip(name).startsWith(strip(messages.text("output-name-keys"))) })).toBeTruthy();
  expect(within(menu).queryByRole("menuitemradio", { name: /Keys only|Pandoc without brackets/ })).toBeNull();
  expect(untranslated(language)).toEqual([]);
});

test("the English output words are the engine's own", () => {
  const messages = setInterfaceLanguage("en-US");
  for (const output of OUTPUTS) {
    const name = OUTPUT_NAMES[output.id as keyof typeof OUTPUT_NAMES], description = OUTPUT_DESCRIPTIONS[output.id as keyof typeof OUTPUT_DESCRIPTIONS];
    if (name) expect(messages.text(name)).toBe(output.name);
    if (description) expect(messages.text(description)).toBe(output.description);
  }
});
