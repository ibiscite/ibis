import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { App } from "../../App";
import { say } from "../../i18n";
import { backend, resetApp, result } from "../../testing/app";

afterEach(async () => { await resetApp(); vi.restoreAllMocks(); });

test("returning to a failed search replaces the picker alert with the retry's results", async () => {
  let attempts = 0;
  let finishB!: (rows: ReturnType<typeof result>[]) => void;
  let finishRetry!: (rows: ReturnType<typeof result>[]) => void;
  backend({ search: ({ query }) => {
    if (query.text === "a") {
      if (++attempts === 1) throw new Error("invented search failure");
      return new Promise((resolve) => { finishRetry = resolve; });
    }
    if (query.text === "ab") return new Promise((resolve) => { finishB = resolve; });
    return [];
  } });
  render(<App />);
  await screen.findByText("Type to find a reference");
  const search = screen.getByLabelText("Search references");
  fireEvent.change(search, { target: { value: "a" } });
  expect((await screen.findByText(say("picker-search-error"))).getAttribute("role")).toBe("alert");
  fireEvent.change(search, { target: { value: "ab" } });
  await waitFor(() => expect(finishB).toBeTypeOf("function"));
  fireEvent.change(search, { target: { value: "a" } });
  await waitFor(() => expect(attempts).toBe(2));
  await act(async () => { finishRetry([result("recovered", "Invented recovered reference")]); });
  await screen.findByRole("option", { name: /Invented recovered reference/ });
  expect(screen.queryByText(say("picker-search-error"))).toBeNull();
  await act(async () => { finishB([result("stale", "Invented stale reference")]); });
  expect(screen.queryByRole("option", { name: /Invented stale reference/ })).toBeNull();
  expect(screen.getByRole("option", { name: /Invented recovered reference/ })).toBeTruthy();
});
