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
 * letters is a hard-coded word, or invented fixture data named here. */
function hardCoded(data: RegExp) {
  const said = [...document.body.querySelectorAll("*")].flatMap((node) => [
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
  { const left = hardCoded(/Invented reference|Smith|2020|synthetic1|refs\.bib|invented|Ibis|Pandoc|Zotero|⌘|⇧|Ctrl|Shift|Enter|Esc/g); if (left.length) console.log(JSON.stringify(left)); expect(left).toEqual([]); }
});

test("setup has no word outside the catalogue", async () => {
  const messages = setInterfaceLanguage("en-US", true);
  render(<InterfaceMessagesProvider messages={messages}><LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined, findSources: async () => ({ zotero: "absent", list: 1, files: [], cut_short: false }), useSampleLibrary: async () => undefined }}
    perform={async (_action, run) => { await run(); }} shortcut={{ shortcut: "Control+Shift+Y", registered: true, error: null }} /></InterfaceMessagesProvider>);
  await screen.findAllByRole("option");
  { const left = hardCoded(/Ibis|Zotero|Control|Ctrl|Shift|Y\b|⌃|⇧/g); if (left.length) console.log(JSON.stringify(left)); expect(left).toEqual([]); }
});

test.each(["general", "look", "citations", "ai-apps", "shortcuts", "about"])("Settings › %s has no word outside the catalogue", async (page) => {
  const messages = setInterfaceLanguage("en-US", true);
  render(<InterfaceMessagesProvider messages={messages}><SettingsApp api={demoApi()} initialPage={page} /></InterfaceMessagesProvider>);
  await screen.findAllByRole("heading");
  await act(async () => {});
  { const left = hardCoded(/Ibis|Zotero|Pandoc|LaTeX|Markdown|Typst|BibTeX|APA|MLA|Chicago|IEEE|Vancouver|Harvard|Claude|Codex|Cursor|ChatGPT|GitHub|macOS|Windows|Linux|0\.0\.0[^ ]*|⌘|⇧|⌃|⌥|Ctrl|Shift|Alt|Control|Option|Command|Enter|Esc|Tab/g); if (left.length) console.log(JSON.stringify(left)); expect(left).toEqual([]); }
});
