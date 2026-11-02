import { afterEach, expect, test } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { App } from "../../App";
import { backend, resetApp } from "../../testing/app";

afterEach(resetApp);
const frame = () => act(async () => { await new Promise(requestAnimationFrame); });

test("after a blank click, the next key goes to Search", async () => {
  backend();
  render(<App />);
  const search = await screen.findByLabelText("Search references");
  act(() => search.focus());
  act(() => search.blur()); // what a click on a blank, unfocusable area does
  await frame();
  expect(document.activeElement).toBe(document.body);
  fireEvent.keyDown(document.body, { key: "s" });
  expect(document.activeElement).toBe(search);
});

test("Escape with focus on the page itself still hides the picker", async () => {
  const calls = backend();
  render(<App />);
  const search = await screen.findByLabelText("Search references");
  act(() => search.blur());
  fireEvent.keyDown(document.body, { key: "Escape" });
  expect(document.activeElement).toBe(search);
  await waitFor(() => expect(calls).toContain("hide_picker"));
});

test("a paste or delete with focus on the page keeps its native edit and reaches Search", async () => {
  backend();
  render(<App />);
  const search = await screen.findByLabelText("Search references");
  for (const key of [{ key: "v", ctrlKey: true }, { key: "v", metaKey: true }, { key: "Backspace" }, { key: "å", altKey: true }]) {
    act(() => search.blur());
    expect(fireEvent.keyDown(document.body, key)).toBe(true); // not cancelled
    expect(document.activeElement).toBe(search);
  }
});
