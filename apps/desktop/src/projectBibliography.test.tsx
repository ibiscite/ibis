import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { App } from "./App";
import type { EntryCopy, ProjectStatus } from "./api";
import { backend, chooseFromMore, resetApp, result } from "./testing/app";

afterEach(resetApp);

const file = (over: Partial<ProjectStatus> = {}): ProjectStatus => ({ generation: 1, name: "thesis.bib", state: "ready", last_good: false, ...over });
/** Names sit in invisible direction marks; a person reads the words between. */
const plain = (text: string | null | undefined) => (text ?? "").replace(/[\u2068\u2069]/g, "");
const shownRow = () => plain(screen.getByRole("option").textContent);
const row = (state: string, generation = 1) => ({ ...result("okafor2018", "Invented seas"), project: { generation, state } });

/** The picker on one searched row, with `status` as the native slot. */
function picker(state: string, status: () => ProjectStatus | null, handlers: Parameters<typeof backend>[0] = {}) {
  const calls = backend({ project_status: status, search: () => [row(state)], ...handlers });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "seas" } });
  return calls;
}

test.each([
  ["in", "In thesis.bib"], ["missing", "Missing from thesis.bib"], ["could-not-check", "Couldn't check thesis.bib"],
  ["repeated", "Key appears more than once"], ["may-name-another", "Key may name another reference"],
])("a row in the %s state says so in words", async (state, words) => {
  picker(state, () => file());
  await waitFor(() => expect(shownRow()).toContain(words));
});

test("a failed read keeps its last good copy, labelled", async () => {
  picker("in", () => file({ state: "failed", last_good: true }));
  await waitFor(() => expect(shownRow()).toContain("In thesis.bib (last good copy)"));
});

test("a formatted output shows no mark, because a style has no key to check", async () => {
  backend({
    project_status: () => file(), search: () => [row("missing")],
    picker_preferences: () => ({ citation_style: "apa", density: "comfortable", larger_text: false, show_paste_preview: true, search_icon: "magnifier", paste: true, menu_bar: true }),
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
    () => file(), () => new Promise((resolve) => { release = resolve; }), () => file({ generation: 3, name: "newest.bib" }),
  ];
  backend({ project_status: () => answers.shift()?.() ?? null, search: () => [row("in", 3)] });
  render(<App />);
  fireEvent.change(screen.getByLabelText("Search references"), { target: { value: "seas" } });
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
  ["copied", /Copied the entry for .*okafor2018.*Add it to .*thesis\.bib/],
  ["no-original", /Export it from your reference manager/],
  ["dependency", /can't come with it/],
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
  await waitFor(() => expect(shownRow()).toContain("Couldn't check thesis.bib"));
  fireEvent.click(screen.getByRole("button", { name: /^More/ }));
  expect(plain((await screen.findByRole("menu")).textContent)).toContain("Couldn't read all of thesis.bib");
  fireEvent.keyDown(await screen.findByRole("menu"), { key: "Escape" });
  status = null;
  await act(() => emit("project:changed"));
  await waitFor(() => expect(shownRow()).not.toContain("thesis.bib"));
  await chooseFromMore("Check Against a Project Bibliography…");
});

test("a choice that could not be saved says so", async () => {
  picker("in", () => file());
  await waitFor(() => expect(shownRow()).toContain("In thesis.bib"));
  await act(() => emit("project:not-saved"));
  expect(await screen.findByText("The project bibliography could not be changed. Try again.")).toBeTruthy();
});
