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


2026-11-11, #1867 slice 1: desktop packages carry the same-commit command.
macOS and Linux deb/rpm offer one Settings row for a non-elevated `~/.local/bin`
symlink and copy the exact PATH profile line when needed; Ibis never edits profiles.
The explicit install creates the user link even when Finder's inherited PATH
omits it, so adding the profile line completes terminal setup without needing
to change launchd's environment.
AppImage uses a durable owned copy, with a receipt for version and removal.
Foreign commands are preserved and never executed by Settings. Windows
installers manage the per-user PATH and uninstall cleanup. The Linux
alternatives integration was cut on 2026-11-12 after repeated review findings:
deb/rpm keep the bundled CLI but register nothing on PATH as root. Stable and
Nightly use the same explicit per-user Settings action, preserving an existing
command from the other channel.
Agent setup will reuse `ibis_core::command_install` in slice 2.

Release builds apply `tauri.bundle.conf.json`, plus
`tauri.linux.bundle.conf.json` on Linux, after staging the CLI with
`scripts/build-cli.sh`. These packaging overlays keep native development/tests
independent of a release sidecar. CI must inspect the installed command in
.app/.dmg, NSIS, deb/rpm and AppImage; check versions against the app;
verify the nested macOS signature; and exercise per-user Settings install/removal
and Windows PATH install/upgrade/uninstall (including preexisting PATH entries,
Stable/Nightly coexistence and AppImage exit). No installer builds are needed
for the config and outcome tests.

2026-11-14 (maintainer): Windows ships one installer, NSIS.
