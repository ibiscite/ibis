# 0033 The engine for agents and tools

The `ibis` command independently reads and cites references through the same
engine as the app. Its contract is [the CLI reference](../cli.md).

- Citations use library records or explicitly supplied records. Missing,
  changed and ambiguous identities refuse rather than produce guessed text.
- One-shot commands default to the app's chosen libraries. Explicit libraries
  replace that choice; `serve` and `mcp` require libraries named. Invalid or
  newer settings refuse.
- MCP supports its declared protocol revisions. Metadata sharing stays out of
  MCP. Lookup and write authority follow [0051](0051-references-in-and-out.md),
  with the network boundary in [0008](0008-offline-by-default.md).
- Notes, highlights and annotation comments require a separate per-library
  opt-in before agents can read them. Private links are always excluded.
- The command is `ibis`; published package names follow
  [0046](0046-official-names-and-listings.md).

Desktop packages carry the CLI from the same commit. macOS and Linux deb/rpm
Settings offer an explicit, non-elevated `~/.local/bin` symlink; Ibis copies a
PATH profile line when needed and never edits profiles. AppImage installs an
owned durable copy with a version/removal receipt. Foreign commands and the
other channel's installation are preserved and never executed by Settings.
Windows installers manage per-user PATH and uninstall cleanup.

Release builds stage the CLI with `scripts/build-cli.sh` and apply bundle
configuration overlays. Qualification checks the installed command and matching
version in every package, nested macOS signatures, Settings install/removal,
Windows PATH upgrades/uninstall, channel coexistence and AppImage exit.
