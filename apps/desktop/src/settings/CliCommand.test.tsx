import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { CommandStatus } from "../bindings";
import { CliCommandRow } from "./CliCommand";

afterEach(cleanup);
const missing: CommandStatus = { state: "not-installed", version: null, used_path: null, profile_line: null, removable: false, installable: true };
const matching: CommandStatus = { ...missing, state: "matching", version: "1.2.3", used_path: "/invented/.local/bin/ibis", removable: true, installable: false };
function row(status: CommandStatus, extra = {}) {
  const api = { cliStatus: vi.fn(async () => status), setCliInstalled: vi.fn(async (on: boolean) => on ? matching : missing), copyCliProfile: vi.fn(async () => {}), ...extra };
  const view = render(<CliCommandRow api={api} highlighted={false} />);
  return { api, ...view };
}

test.each([
  [missing, "Not installed"],
  [matching, "ibis 1.2.3 — matches this app"],
  [{ ...matching, state: "different", version: "1.1.0" }, "ibis 1.1.0 — a different version from this app"],
  [{ ...missing, state: "foreign", used_path: "/invented/homebrew/bin/ibis", installable: false }, "Another ibis is used: /invented/homebrew/bin/ibis. Version unavailable."],
] as const)("Settings shows command state $state", async (status, label) => {
  row(status as CommandStatus);
  expect(await screen.findByText(label)).toBeTruthy();
});

test("install and remove update the row, and a fresh visit reads the saved result", async () => {
  const first = row(missing);
  fireEvent.click(await screen.findByRole("button", { name: "Install the ibis command" }));
  await screen.findByText("ibis 1.2.3 — matches this app");
  expect(first.api.setCliInstalled).toHaveBeenCalledWith(true);
  first.unmount();
  row(matching);
  fireEvent.click(await screen.findByRole("button", { name: "Remove Command" }));
  await screen.findByText("Not installed");
});

test("PATH guidance copies the exact native profile line", async () => {
  const profile = 'export PATH="$HOME/.local/bin:$PATH"';
  const { api } = row({ ...missing, profile_line: profile });
  await screen.findByText(profile);
  fireEvent.click(screen.getByRole("button", { name: "Copy" }));
  await screen.findByRole("button", { name: "Copied" });
  expect(api.copyCliProfile).toHaveBeenCalledOnce();
});

test("failed I/O says so and a completion after closing cannot change a reopened row", async () => {
  let resolve!: (status: CommandStatus) => void;
  const first = row(missing, { setCliInstalled: vi.fn(() => new Promise<CommandStatus>((done) => { resolve = done; })) });
  fireEvent.click(await screen.findByRole("button", { name: "Install the ibis command" }));
  first.unmount();
  row(missing, { setCliInstalled: vi.fn(async () => { throw new Error("denied"); }) });
  await act(async () => resolve(matching));
  await screen.findByText("Not installed");
  fireEvent.click(screen.getByRole("button", { name: "Install the ibis command" }));
  await screen.findByRole("alert");
  expect(screen.queryByText("ibis 1.2.3 — matches this app")).toBeNull();
});


test("a read delayed beyond an install cannot undo its result", async () => {
  let release!: (status: CommandStatus) => void;
  const cliStatus = vi.fn().mockResolvedValueOnce(missing).mockImplementationOnce(() => new Promise<CommandStatus>((done) => { release = done; }));
  row(missing, { cliStatus });
  const install = await screen.findByRole("button", { name: "Install the ibis command" });
  fireEvent(window, new Event("focus"));
  fireEvent.click(install);
  await screen.findByText("ibis 1.2.3 — matches this app");
  await act(async () => release(missing));
  expect(screen.getByText("ibis 1.2.3 — matches this app")).toBeTruthy();
});
