import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { EndpointStatus } from "../bindings";
import { EditorEndpointRow } from "./EditorEndpoint";

afterEach(cleanup);

const off: EndpointStatus = { on: false, url: null, problem: null };
const on: EndpointStatus = { on: true, url: "http://127.0.0.1:23124/0123456789abcdef0123456789abcdef/better-bibtex/cayw?format=pandoc", problem: null };

function row(api: Partial<Parameters<typeof EditorEndpointRow>[0]["api"]> = {}) {
  const full = { editorEndpoint: vi.fn(async () => off), setEditorEndpoint: vi.fn(async (next: boolean) => next ? on : off), copyEditorEndpoint: vi.fn(async () => null), ...api };
  render(<EditorEndpointRow api={full} highlighted={false} />);
  return full;
}

test("it is off until switched on, then shows the address to paste and what it lets in", async () => {
  const api = row();
  const toggle = await screen.findByRole("switch", { name: "Editor endpoint" });
  await waitFor(() => expect((toggle as HTMLInputElement).disabled).toBe(false));
  expect((toggle as HTMLInputElement).checked).toBe(false);
  expect(screen.queryByText(/127\.0\.0\.1/)).toBeNull();
  expect(document.body.textContent).toContain("Web pages can't use it, and nothing leaves this computer.");
  fireEvent.click(toggle);
  await screen.findByText(on.url!);
  expect(api.setEditorEndpoint).toHaveBeenCalledWith(true);
  fireEvent.click(screen.getByRole("button", { name: "Copy Address" }));
  await screen.findByRole("button", { name: "Copied" });
  expect(api.copyEditorEndpoint).toHaveBeenCalledTimes(1);
});

test("a port another program holds is said, and the switch stays off", async () => {
  row({ setEditorEndpoint: vi.fn(async () => ({ on: false, url: null, problem: "port-in-use" as const })) });
  const toggle = await screen.findByRole("switch", { name: "Editor endpoint" });
  await waitFor(() => expect((toggle as HTMLInputElement).disabled).toBe(false));
  fireEvent.click(toggle);
  await screen.findByText("Another program is using the endpoint's port, so it is off.");
  expect((toggle as HTMLInputElement).checked).toBe(false);
});

test("a slow first read cannot undo a newer switch", async () => {
  let release: (status: EndpointStatus) => void = () => {};
  const api = row({ editorEndpoint: vi.fn(() => new Promise<EndpointStatus>((resolve) => { release = resolve; })) });
  const toggle = await screen.findByRole("switch", { name: "Editor endpoint" });
  // A failed first read leaves it usable; here the read is still out.
  expect((toggle as HTMLInputElement).disabled).toBe(true);
  await act(async () => { await api.setEditorEndpoint(true); });
  release(off);
  await act(async () => {});
  expect(screen.queryByText(on.url!)).toBeNull();
});

test("a failure to change it says so", async () => {
  row({ setEditorEndpoint: vi.fn(async () => { throw new Error("no"); }) });
  const toggle = await screen.findByRole("switch", { name: "Editor endpoint" });
  await waitFor(() => expect((toggle as HTMLInputElement).disabled).toBe(false));
  fireEvent.click(toggle);
  await screen.findByText("The endpoint could not be changed. Try again.");
});
