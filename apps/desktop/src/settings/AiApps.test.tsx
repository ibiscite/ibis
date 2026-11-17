import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { AgentApp } from "../bindings";
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
  expect(screen.getByRole("button", { name: "Disconnect Claude Code" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Repair Claude Desktop" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Replace… Codex" })).toBeTruthy();
  expect(screen.getByText("its settings file is a link")).toBeTruthy();
  expect(screen.getByText("Open Settings › MCP servers › Add server.")).toBeTruthy();
  expect(screen.getByText(/Other servers and settings are kept/)).toBeTruthy();
});

test("Connect and Disconnect use the shared commands and show their fresh list", async () => {
  const { api } = page();
  fireEvent.click(await screen.findByRole("button", { name: "Connect Cursor" }));
  await screen.findByText("Connected. Start a new session in the app to use Ibis.");
  expect(api.connectAgentApp).toHaveBeenCalledWith("cursor", false, missing.revision);
  fireEvent.click(screen.getByRole("button", { name: "Disconnect Cursor" }));
  await screen.findByRole("button", { name: "Connect Cursor" });
  expect(api.disconnectAgentApp).toHaveBeenCalledWith("cursor", connected.revision);
});

test("a failure says the shared sentence and keeps the old state", async () => {
  page([missing], { connectAgentApp: vi.fn(async () => { throw new Error("invented failure"); }) });
  fireEvent.click(await screen.findByRole("button", { name: "Connect Cursor" }));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Could not change the AI app connection. Check the app’s settings and folder permissions, then try again.");
  expect(screen.getByText("Not connected.")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Connect Cursor" })).toBeTruthy();
});
