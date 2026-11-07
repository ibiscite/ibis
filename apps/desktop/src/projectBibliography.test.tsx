import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { EntryCopy, ProjectStatus } from "./api";
import { backend, chooseFromMore, receipt, resetApp, result } from "./testing/app";

afterEach(resetApp);

test("an unreadable bibliography is a warning with Retry: Enter copies, says why and stays open", async () => {
  const warning = "Couldn't check a against thesis.bib.";
  const called = backend({
    search: () => [result("a", "Invented work")],
    project_status: () => file({ state: "failed", reason: "It is not there, or not on this computer yet." }),
    preview_file_citation: () => ({ ...receipt("[@a]"), warnings: [warning] }),
    copy_file_citation: () => ({ text: "[@a]", warnings: [warning] }),
  });
  render(<App />);
  await screen.findByText("Invented work");
  const notice = await screen.findByText((text) => plain(text).startsWith("Couldn’t read thesis.bib."));
  expect(notice.closest('[role="status"]')).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(called).toContain("retry_project"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(called).toContain("copy_file_citation"));
  await waitFor(() => expect(screen.getAllByText(warning).length).toBeGreaterThan(0));
  expect(called).not.toContain("hide_picker");
});

test("a known wrong-work key is refused and never reaches native copy, by Enter or by dragging", async () => {
  const called = backend({ search: () => [result("a", "Invented work")],
    preview_file_citation: () => ({ ...receipt("[@a]"), project_refusal: "A key in thesis.bib identifies a different work." }),
  });
  render(<App />);
  await screen.findByText("A key in thesis.bib identifies a different work.");
  expect(document.querySelector<HTMLButtonElement>(".footer-action")?.disabled).toBe(true);
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  expect(called).not.toContain("copy_file_citation");
  // Dragging delivers what Enter pastes, so it delivers nothing here either.
  const beak = document.querySelector<HTMLElement>(".shelf-leading")!;
  expect(beak.getAttribute("draggable")).toBeNull();
  const data = new Map<string, string>();
  fireEvent.dragStart(beak, { dataTransfer: { setData: (type: string, value: string) => data.set(type, value), effectAllowed: "" } });
  expect([...data.keys()]).toEqual([]);
});

test("Retry checks again and keeps the citation, but does not replay the refused Enter", async () => {
  let refused = true;
  const called = backend({ search: () => [result("a", "Invented work")],
    project_status: () => file({ revision: refused ? 1 : 2 }),
    preview_file_citation: () => ({ ...receipt("[@a]"), project_refusal: refused ? "A key in thesis.bib identifies a different work." : null }),
    retry_project: async () => { refused = false; await emit("project:changed", [1, 2]); },
    copy_file_citation: () => ({ text: "[@a]", warnings: [] }),
  });
  render(<App />);
  await screen.findByText("A key in thesis.bib identifies a different work.");
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Retry" })).toBeNull());
  expect(called.filter((command) => command === "retry_project")).toHaveLength(1);
  expect(called).not.toContain("copy_file_citation");
  expect(screen.getByText("Invented work")).toBeTruthy();
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(called.filter((command) => command === "copy_file_citation")).toHaveLength(1));
});

const file = (over: Partial<ProjectStatus> = {}): ProjectStatus => ({ generation: 1, revision: 1, name: "thesis.bib", state: "ready", reason: null, damaged_lines: [], ...over });
/** Names sit in invisible direction marks; a person reads the words between. */
const plain = (text: string | null | undefined) => (text ?? "").replace(/[\u2068\u2069]/g, "");
const shownRow = () => plain(screen.getByRole("option").textContent);
const row = (state: string, generation = 1, lines: number[] = []) => ({ ...result("okafor2018", "Invented seas"), project: { generation, state, lines } });

/** The picker on one searched row, with `status` as the native slot. */
let called: string[] = [];
const calls = (command: string) => called.filter((name) => name === command).length;
function picker(state: string, status: () => ProjectStatus | null, handlers: Parameters<typeof backend>[0] = {}) {
  called = backend({ project_status: status, search: () => [row(state)], ...handlers });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "seas" } });
}

test.each([
  ["in", "In thesis.bib"], ["missing", "Missing from thesis.bib"], ["could-not-check", "Couldn’t check thesis.bib"],
  ["repeated", "Key appears more than once"], ["may-name-another", "Key may name another reference"],
])("a row in the %s state says so in words", async (state, words) => {
  picker(state, () => file());
  await waitFor(() => expect(shownRow()).toContain(words));
});

test("a repeated key says where to look in the file", async () => {
  backend({ project_status: () => file(), search: () => [row("repeated", 1, [12, 88])] });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "seas" } });
  await waitFor(() => expect(shownRow()).toContain("Key appears more than once (lines 12, 88)"));
});

test("a citation with a warning stays open and says why instead of pasting", async () => {
  const warning = "okafor2018 is missing from thesis.bib.";
  picker("missing", () => file(), {
    preview_file_citation: () => receipt("[@okafor2018]", null, [warning]),
    copy_file_citation: () => ({ text: "[@okafor2018]", warnings: [warning] }),
  });
  await waitFor(() => expect(shownRow()).toContain("Missing from thesis.bib"));
  await waitFor(() => expect(document.body.textContent).toContain("[@okafor2018]"));
  fireEvent.keyDown(screen.getByLabelText("Search references"), { key: "Enter" });
  await waitFor(() => expect(document.body.textContent).toContain(warning));
  expect(calls("hide_picker")).toBe(0);
});

test("with no bibliography chosen, the check asks no search twice", async () => {
  const called = backend({ project_status: () => null, search: () => [row("in")] });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "seas" } });
  await screen.findByText("Invented seas");
  await act(() => emit("project:changed"));
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  expect(called.filter((command) => command === "search")).toHaveLength(1);
});

test("new content in the same healthy file marks the rows again", async () => {
  let status = file(), mark = "missing";
  backend({ project_status: () => status, search: () => [row(mark)] });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "seas" } });
  await waitFor(() => expect(shownRow()).toContain("Missing from thesis.bib"));
  status = file({ revision: 2 });
  mark = "in";
  await act(() => emit("project:changed"));
  await waitFor(() => expect(shownRow()).toContain("In thesis.bib"));
});

test("a formatted output shows no mark, because a style has no key to check", async () => {
  backend({
    project_status: () => file(), search: () => [row("missing")],
    picker_preferences: () => ({ output: "apa", density: "comfortable", larger_text: false, show_paste_preview: true, search_icon: "magnifier", paste: true, menu_bar: true }),
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "seas" } });
  await screen.findByText("Invented seas");
  expect(shownRow()).not.toContain("thesis.bib");
});

test("a row answers only for the choice it was marked against, across a change of file", async () => {
  let status = file();
  picker("in", () => status);
  await waitFor(() => expect(shownRow()).toContain("In thesis.bib"));
  // B is chosen; A's row is still on screen and B has not been read yet.
  status = file({ generation: 2, name: "other.bib", state: "reading" });
  await act(() => emit("project:changed"));
  await waitFor(() => expect(shownRow()).not.toContain("thesis.bib"));
  expect(shownRow()).not.toContain("other.bib");
});

test("a status read that finishes after a newer one cannot replace it", async () => {
  let release: (status: ProjectStatus) => void = () => {};
  const answers: (() => Promise<ProjectStatus> | ProjectStatus)[] = [
    // The hook reads at mount and again once its listener exists.
    () => file(), () => file(), () => new Promise((resolve) => { release = resolve; }), () => file({ generation: 3, name: "newest.bib" }),
  ];
  backend({ project_status: () => answers.shift()?.() ?? null, search: () => [row("in", 3)] });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "seas" } });
  await waitFor(() => expect(answers).toHaveLength(2));
  await act(() => emit("project:changed"));
  await act(() => emit("project:changed"));
  await waitFor(() => expect(shownRow()).toContain("In newest.bib"));
  await act(async () => { release(file({ generation: 2, name: "stale.bib" })); });
  expect(shownRow()).toContain("In newest.bib");
});

test("the status survives the picker closing and opening again", async () => {
  picker("missing", () => file());
  await waitFor(() => expect(shownRow()).toContain("Missing from thesis.bib"));
  await act(() => emit("picker:hidden"));
  await act(() => emit("picker:shown", 1));
  await waitFor(() => expect(shownRow()).toContain("Missing from thesis.bib"));
});

test.each<[EntryCopy, RegExp]>([
  ["copied", /Copied the entry for okafor2018 as your file has it, including any file or note field\. Check it, then add it to thesis\.bib/],
  ["no-original", /Export it from your reference manager/],
  ["dependency", /can’t come with it/],
  ["changed", /Search again, then try again/],
])("copying a missing entry ending %s says what to do next", async (outcome, words) => {
  const sent: unknown[] = [];
  picker("missing", () => file(), { copy_project_entry: (args) => { sent.push(args); return outcome; } });
  await waitFor(() => expect(shownRow()).toContain("Missing from thesis.bib"));
  await chooseFromMore("Copy Entry for \u2068thesis.bib\u2069");
  await waitFor(() => expect(plain(document.querySelector(".notices")?.textContent)).toMatch(words));
  expect(sent).toEqual([{ id: row("missing").id, generation: 1 }]);
});

test("an entry is offered only for a missing key", async () => {
  picker("repeated", () => file());
  await waitFor(() => expect(shownRow()).toContain("Key appears more than once"));
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  const menu = await screen.findByRole("menu");
  expect(within(menu).queryByRole("menuitem", { name: /Copy Entry/ })).toBeNull();
  expect(within(menu).getByRole("menuitem", { name: /Stop Checking/ })).toBeTruthy();
});

test("More says what is wrong with the file, and offers the choice when none is made", async () => {
  let status: ProjectStatus | null = file({ state: "damaged" });
  picker("could-not-check", () => status);
  await waitFor(() => expect(shownRow()).toContain("Couldn’t check thesis.bib"));
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  expect(plain((await screen.findByRole("menu")).textContent)).toContain("Couldn’t read all of thesis.bib, so nothing is called missing.");
  fireEvent.keyDown(await screen.findByRole("menu"), { key: "Escape" });
  status = null;
  await act(() => emit("project:changed"));
  await waitFor(() => expect(shownRow()).not.toContain("thesis.bib"));
  await chooseFromMore("Check Against a .bib File…");
});

test("More names the failed read and why", async () => {
  picker("could-not-check", () => file({ state: "failed", reason: "It is still changing." }));
  await waitFor(() => expect(shownRow()).toContain("Couldn’t check thesis.bib"));
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  expect(plain((await screen.findByRole("menu")).textContent)).toContain("Couldn’t read thesis.bib. It is still changing.");
});

test("a choice that could not be saved says so", async () => {
  picker("in", () => file());
  await waitFor(() => expect(shownRow()).toContain("In thesis.bib"));
  await act(() => emit("project:not-saved"));
  expect(await screen.findByText("The .bib file to check against could not be changed. Try again.")).toBeTruthy();
});

test("Enter while the project bibliography is still reading copies once the read ends, without a failure", async () => {
  let reading = true;
  const calls = backend({
    project_status: () => file({ state: reading ? "reading" : "ready", revision: reading ? 1 : 2 }),
    picker_destination: () => ({ app: null, app_id: null, app_identity: null, automatic: null, remembered: null, unreadable: false, can_paste: false, output_question: null, paste: null, paste_question: false, can_choose_paste: false, context_version: reading ? [1, 1] : [1, 2] }),
    search: () => [result("a", "First")],
    // The engine refuses a key output whose project read has not finished.
    preview_file_citation: () => reading ? Promise.reject({ code: "cancelled" }) : receipt("[@a]"),
    copy_file_citation: () => ({ outcome: "copied", warnings: [] }),
  });
  render(<App />);
  const box = screen.getByLabelText("Search references");
  fireEvent.change(box, { target: { value: "first" } });
  await screen.findByText("First");
  await waitFor(() => expect(calls.filter((call) => call === "preview_file_citation").length).toBeGreaterThan(0));
  fireEvent.keyDown(box, { key: "Enter" });
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryAllByText("This citation could not be prepared. Close and reopen Ibis to try again.")).toHaveLength(0);
  reading = false;
  await act(() => emit("project:changed", [1, 2]));
  await waitFor(() => expect(calls.filter((call) => call === "copy_file_citation")).toHaveLength(1));
  expect(screen.queryAllByText("This citation could not be prepared. Close and reopen Ibis to try again.")).toHaveLength(0);
});

test("a status read that fails after a reading answer does not leave the preview waiting", async () => {
  let fail = false;
  backend({
    project_status: () => fail ? Promise.reject(new Error("unreadable")) : file({ state: "reading" }),
    search: () => [result("a", "First")],
    preview_file_citation: () => Promise.reject({ code: "cancelled" }),
  });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "first" } });
  await screen.findByText("First");
  fail = true;
  await act(() => emit("project:changed", [1, 2]));
  expect((await screen.findAllByText("This citation could not be prepared. Close and reopen Ibis to try again.")).length).toBeGreaterThan(0);
});
