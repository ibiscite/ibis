import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { AgentApp } from "../bindings";
import { deferred } from "../testing/deferred";
import { AiApps } from "./AiApps";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const missing: AgentApp = { id: "cursor", name: "Cursor", found: true, revision: "invented-revision", state: "not-connected", detail: "", diff: null, needs_command: true, ibis_on_path: true };
const connected: AgentApp = { ...missing, state: "connected" };
const different: AgentApp = { ...missing, state: "different", diff: '- "ibis": {"command":"other"}\n+ "ibis": {"command":"ibis","args":["mcp","--app-libraries"]}' };

function page(apps: AgentApp[] = [missing], overrides = {}) {
  const api = {
    agentApps: vi.fn(async () => apps),
    connectAgentApp: vi.fn(async (_id: string, _replace: boolean) => [connected]),
    disconnectAgentApp: vi.fn(async (_id: string) => [missing]),
    ...overrides,
  };
  return { api, ...render(<AiApps api={api} highlighted={null} />) };
}

test("the list renders every app and gives each state its action or explanation", async () => {
  page([
    missing,
    { ...connected, id: "claude-code", name: "Claude Code" },
    { ...missing, id: "claude-desktop", name: "Claude Desktop", state: "stale" },
    { ...different, id: "codex", name: "Codex" },
    { ...missing, id: "vscode", name: "VS Code", state: "blocked", detail: "its settings file is a link" },
    { ...missing, id: "gemini", name: "Gemini CLI", state: "plugin" },
    { ...missing, id: "windsurf", name: "Windsurf", found: false },
    { ...missing, id: "lm-studio", name: "LM Studio", needs_command: false },
    { ...missing, id: "chatgpt", name: "ChatGPT desktop", state: "by-hand", detail: "Open Settings › MCP servers › Add server." },
  ]);
  for (const name of ["Cursor", "Claude Code", "Claude Desktop", "Codex", "VS Code", "Gemini CLI", "Windsurf", "LM Studio", "ChatGPT desktop"]) expect(await screen.findByText(name)).toBeTruthy();
  expect(screen.getByRole("button", { name: "Disconnect" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Repair" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Replace…" })).toBeTruthy();
  expect(screen.getByText("its settings file is a link")).toBeTruthy();
  expect(screen.getByText("Open Settings › MCP servers › Add server.")).toBeTruthy();
  expect(screen.getByText(/Other servers and settings are kept/)).toBeTruthy();
});

test("Connect and Disconnect use the shared commands and show their fresh list", async () => {
  const { api } = page();
  fireEvent.click(await screen.findByRole("button", { name: "Connect" }));
  await screen.findByText("Connected. Start a new session in the app to use Ibis.");
  expect(api.connectAgentApp).toHaveBeenCalledWith("cursor", false, missing.revision);
  fireEvent.click(screen.getByRole("button", { name: "Disconnect" }));
  await screen.findByRole("button", { name: "Connect" });
  expect(api.disconnectAgentApp).toHaveBeenCalledWith("cursor", connected.revision);
});

test("Replace shows the before and after and changes nothing until pressed", async () => {
  const { api } = page([different]);
  const replace = await screen.findByRole("button", { name: "Replace…" });
  expect(screen.getByText('- "ibis": {"command":"other"}')).toBeTruthy();
  expect(screen.getByText('+ "ibis": {"command":"ibis","args":["mcp","--app-libraries"]}')).toBeTruthy();
  expect(api.connectAgentApp).not.toHaveBeenCalled();
  fireEvent.click(replace);
  await screen.findByRole("button", { name: "Disconnect" });
  expect(api.connectAgentApp).toHaveBeenCalledWith("cursor", true, different.revision);
});

test("a failure says the shared sentence and keeps the old state", async () => {
  page([missing], { connectAgentApp: vi.fn(async () => { throw new Error("invented failure"); }) });
  fireEvent.click(await screen.findByRole("button", { name: "Connect" }));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Could not change the AI app connection. Check the app’s settings and folder permissions, then try again.");
  expect(screen.getByText("Not connected.")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Connect" })).toBeTruthy();
});

test("a late status reply cannot overwrite a newer change", async () => {
  const late = deferred<AgentApp[]>();
  const agentApps = vi.fn().mockResolvedValueOnce([missing]).mockImplementationOnce(() => late.promise);
  page([missing], { agentApps });
  const connect = await screen.findByRole("button", { name: "Connect" });
  fireEvent(window, new Event("focus"));
  fireEvent.click(connect);
  await screen.findByRole("button", { name: "Disconnect" });
  await act(async () => late.resolve([missing]));
  expect(screen.getByRole("button", { name: "Disconnect" })).toBeTruthy();
});

test("hidden windows do not refresh and closing invalidates outstanding requests", async () => {
  const late = deferred<AgentApp[]>();
  const { api, unmount } = page([missing], { agentApps: vi.fn().mockResolvedValueOnce([missing]).mockImplementationOnce(() => late.promise) });
  await screen.findByText("Cursor");
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
  fireEvent(window, new Event("focus"));
  expect(api.agentApps).toHaveBeenCalledTimes(1);
  vi.restoreAllMocks();
  fireEvent(window, new Event("focus"));
  unmount();
  page();
  await act(async () => late.resolve([connected]));
  expect(await screen.findByRole("button", { name: "Connect" })).toBeTruthy();
});

test("apps needing the bare command point to Command Line when it is missing", async () => {
  page([{ ...missing, ibis_on_path: false }, { ...missing, id: "lm-studio", name: "LM Studio", needs_command: false, ibis_on_path: false }]);
  const cursor = (await screen.findByText("Cursor")).closest(".ui-row")! as HTMLElement;
  expect(within(cursor).getByText(/Command Line above/)).toBeTruthy();
  const studio = screen.getByText("LM Studio").closest(".ui-row")! as HTMLElement;
  expect(within(studio).queryByText(/Command Line above/)).toBeNull();
});
