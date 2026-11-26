import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { App } from "../App";
import { LibrarySetup } from "../LibrarySetup";
import { demoApi } from "../settings/demo";
import { SettingsApp } from "../settings/SettingsApp";
import { backend, resetApp, result } from "../testing/app";
import { InterfaceMessagesProvider, setInterfaceLanguage } from ".";

afterEach(async () => { await resetApp(); setInterfaceLanguage("en-US"); });

/** Words the window shows that did not come from the catalogue: in the
 * pseudolocale every catalogue word is inside ⟦ ⟧, so anything else with
 * letters is a hard-coded word, or invented data named here. Code, such as a
 * diff or a BibTeX entry, is data. Words Rust says (a pane’s name)
 * come from the demo here and from Rust's catalogue in the app. */
function hardCoded(data: RegExp) {
  const said = [...document.body.querySelectorAll(":not(code, code *, pre, pre *)")].flatMap((node) => [
    ...[...node.childNodes].filter((child) => child.nodeType === Node.TEXT_NODE).map((child) => child.textContent ?? ""),
    ...["aria-label", "aria-description", "title", "placeholder", "alt"].map((name) => node.getAttribute(name) ?? ""),
  ]);
  return [...new Set(said.map((text) => text.replace(/⟦[^⟧]*⟧/g, "").replace(data, "").trim()).filter((text) => /\p{L}{2}/u.test(text)))];
}

test("the picker has no word outside the catalogue", async () => {
  const messages = setInterfaceLanguage("en-US", true);
  backend({ search: () => [result("synthetic1", "Invented reference")] });
  await act(async () => { render(<InterfaceMessagesProvider messages={messages}><App /></InterfaceMessagesProvider>); });
  await act(async () => { fireEvent.change(screen.getByRole("combobox"), { target: { value: "invented" } }); });
  await screen.findByText("Invented reference");
  expect(hardCoded(new RegExp(`Invented reference|Smith|2020|synthetic1|refs\\.bib|invented|${names.source}`, "g"))).toEqual([]);
});

test("setup has no word outside the catalogue", async () => {
  const messages = setInterfaceLanguage("en-US", true);
  render(<InterfaceMessagesProvider messages={messages}><LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined, findSources: async () => ({ zotero: "absent", list: 1, files: [], cut_short: false }), useSampleLibrary: async () => undefined }}
    perform={async (_action, run) => { await run(); }} shortcut={{ shortcut: "Control+Shift+Y", registered: true, error: null }} /></InterfaceMessagesProvider>);
  await screen.findAllByRole("option");
  expect(hardCoded(new RegExp(`\\bY\\b|${names.source}`, "g"))).toEqual([]);
});

/** Names that are the same in every language: apps, formats, systems, symbols. */
const names = /Ibis|Zotero|Pandoc|LaTeX|Markdown|Typst|BibTeX|APA|MLA|Chicago|IEEE|Vancouver|Harvard|Claude|Codex|Cursor|ChatGPT|GitHub|macOS|Windows|Linux|0\.0\.0[^ ]*|⌘|⇧|⌃|⌥/;

test.each(["general", "look", "citations", "ai-apps", "shortcuts", "about"])("Settings › %s has no word outside the catalogue", async (page) => {
  const messages = setInterfaceLanguage("en-US", true);
  render(<InterfaceMessagesProvider messages={messages}><SettingsApp api={demoApi()} initialPage={page} /></InterfaceMessagesProvider>);
  await screen.findAllByRole("heading");
  await act(async () => {});
  const demo = /Accessibility|In [^.]+ desktop: .*|title|River signals|A map of the reeds|lin2026map|\d+\/\d+\/\d+, [\d:]+ [AP]M|Sample references|signals\.bib|Desktop|desktop|Code/g;
  expect(hardCoded(new RegExp(`${demo.source}|${names.source}`, "g"))).toEqual([]);
});
