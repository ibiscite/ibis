import { fixtureDestination, formatAnswer } from "./testing/format";
import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { App } from "./App";
import type { PickerDestination } from "./api";
import { isStyle, listedOutput } from "./format/outputs";
import { backend, prepared, receipt, resetApp, result, type PreviewRequest, showPicker } from "./testing/app";

afterEach(resetApp);
const BROWSER: PickerDestination = fixtureDestination({ app: "Example Browser", app_id: "example.browser", app_identity: null, unreadable: false, can_paste: false, paste: null, paste_question: false, can_choose_paste: false, format: formatAnswer("pandoc", "yours") });
const formatButton = () => within(screen.getByLabelText("Search references").closest("header")!).getByRole("button", { name: /^Citation format / });
const formatShown = () => formatButton().getAttribute("aria-label")!.slice("Citation format ".length);
const openFormats = () => { if (formatButton().getAttribute("aria-expanded") !== "true") fireEvent.click(formatButton()); };
async function pick(name: string) {
  await waitFor(() => expect(formatButton().hasAttribute("disabled")).toBe(false));
  openFormats();
  fireEvent.click(screen.getByRole("option", { name }));
}
function opened(destination: () => PickerDestination, overrides: Parameters<typeof backend>[0] = {}) {
  const previews: PreviewRequest[] = [];
  const copies: { ticket: string; contentHash: string }[] = [];
  const remembered: (string | null)[] = [];
  const calls = backend({
    search: () => [result("a", "First")],
    "destination": () => destination(),
    preview_file_citation: (request) => {
      previews.push(request);
      const number = previews.length;
      const text = isStyle(listedOutput(request.output) ?? "pandoc") ? `(Smith, 2020) ${request.output}` : `synthetic ${request.output}`;
      return { ...receipt(text), ticket: `ticket-${number}`, content_hash: `hash-${number}` };
    },
    deliver_citation: (args) => {
      copies.push({ ticket: args.ticket, contentHash: args.contentHash });
      return { outcome: "retained", copied: { text: prepared(args).preview.text, warnings: [] }, reason: "pinned" };
    },
    remember_app_output: ({ output }) => { remembered.push(output as string | null); return null; },
    ...overrides,
  });
  render(<App />);
  return { calls, previews, copies, remembered };
}

test("a format pick is remembered for the browser, and a later pick replaces it", async () => {
  let destination = BROWSER;
  const { calls, remembered } = opened(() => destination);
  await screen.findByText("First");
  await pick("LaTeX \\cite");
  await waitFor(() => expect(remembered).toEqual(["cite"]));
  destination = fixtureDestination({ ...BROWSER, format: formatAnswer("cite", "app") });
  await act(async () => { await showPicker(); });
  await waitFor(() => expect(formatShown()).toBe("LaTeX \\cite · Example Browser"));
  await pick("Typst");
  await waitFor(() => expect(remembered).toEqual(["cite", "typst"]));
  await waitFor(() => expect(formatShown()).toBe("Typst · Example Browser"));
  expect(calls).not.toContain("picker_preferences_saved");
});

test("a missing DOI offers the format menu and BibTeX can recover the citation", async () => {
  const previews: string[] = [];
  backend({ search: () => [result("atlas", "Invented atlas")],
    preview_file_citation: ({ output }) => {
      previews.push(output);
      if (output === "doi-links") throw { code: "missing-doi", message: { id: "problem-missing-doi", args: {} },
        recovery: { action: "choose-output" }, focus: "format-picker" };
      return receipt(output === "bibtex" ? "@book{atlas}" : "[@atlas]");
    },
  });
  render(<App />);
  await pick("DOI links");
  const alert = await screen.findByRole("alert");
  expect(alert.textContent).toContain("Choose BibTeX in the citation format menu.");
  expect(formatButton().hasAttribute("disabled")).toBe(false);
  await pick("BibTeX");
  await waitFor(() => expect(screen.getByRole("region", { name: "Preview" }).textContent).toContain("@book{atlas}"));
  expect(screen.queryByRole("alert")).toBeNull();
  expect(previews).toContain("bibtex");
});
