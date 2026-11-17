import { afterEach, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { CommandStatus } from "../bindings";
import { CliCommandRow } from "./CliCommand";

afterEach(cleanup);
const missing: CommandStatus = { state: "not-installed", version: null, used_path: null, profile_line: null, removable: false, installable: true };
const matching: CommandStatus = { ...missing, state: "matching", version: "1.2.3", used_path: "/invented/.local/bin/ibis", removable: true, installable: false };
function row(status: CommandStatus, extra = {}, onChanged?: () => void) {
  const api = { cliStatus: vi.fn(async () => status), setCliInstalled: vi.fn(async (on: boolean) => on ? matching : missing), copyCliProfile: vi.fn(async () => {}), ...extra };
  const view = render(<CliCommandRow api={api} highlighted={false} onChanged={onChanged} />);
  return { api, ...view };
}

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
