# 0045 Ibis's MCP server stays local

2026-11-11 (maintainer). OpenAI's plugin directory accepts local, command-line MCP servers only from allowlisted partners, arranged through an OpenAI representative. Individual developers have no self-service route (support case 18881579). Ibis won't add a hosted MCP endpoint to qualify: it would break the promise that Ibis reads your library on your computer and makes no network calls (decision 0008), and it would add a service to run and pay for.

Instead:
- The documentation shows how to add `ibis mcp` by hand in Codex, the ChatGPT desktop app and other MCP clients.
- Ibis is listed in the open MCP Registry and in Claude's extension directory.
- The OpenAI draft stays on file, in case local plugins open to individual open-source developers.
