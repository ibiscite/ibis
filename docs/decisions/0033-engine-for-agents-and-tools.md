# 0033 The engine for agents and tools

The `ibis` command is a complete, independent way into a person's references,
for scripts, other tools and AI agents, whatever manager or file holds them;
the app is one client of the same engine. Its public contract is
[docs/cli.md](../cli.md). Decided by the maintainer on 2026-11-01:

- Every citation comes from a real record: the person's library first, and an
  identifier lookup (DOI, arXiv and similar) only once that network use is
  approved. Agents pass the keys or ids Ibis returned; Ibis refuses anything
  changed, missing or ambiguous and never fills a gap with a guess.
- The command line, `ibis serve` and `ibis mcp` stay read-only until 1.0, and
  Ibis stays offline. Groundwork that needs no network comes first (offline
  identifier parsing, results that say where a record came from, one lookup point
  that is off by default); any internet use waits for its own approval, naming
  each service and what it learns.
- `ibis mcp` follows the current MCP revision (2026-07-28) and keeps answering
  older clients; Ibis follows each standard's latest official guidance.
- With no library named, one-shot commands read the libraries chosen in the app,
  read-only. Named libraries replace them; `serve` and `mcp` still need libraries
  named; unreadable or newer settings stop with that reason.
- The command is `ibis`; package names use `ibiscite` where `ibis` is taken. The
  app ships the matching `ibis` and offers to install it; the standalone download
  stays. Packages: a Homebrew tap, winget and `cargo binstall`; others on request.

2026-11-01: The maintainer authorized accepting Anthropic's Software Directory
Terms and Directory Policy and submitting the Ibis plugin with its two disclosed
policy holds (external command and name similarity). Submitted for review, with
scheduled checks and manual publication. Temporary client installations and
marketplaces used for validation are removed after testing.

2026-11-05 (maintainer): agents may read Zotero highlights, notes and annotation
comments through `ibis` and `ibis mcp` only after the person turns this on for a
library in the app. It is off by default, can be turned off at any time and
never includes private links. Without it, agents get references and citations
only. This replaces the earlier exclusion of notes and annotations from MCP.
