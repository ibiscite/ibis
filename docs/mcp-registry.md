# Listing `ibis mcp` in the MCP Registry

The [MCP Registry](https://registry.modelcontextprotocol.io) lists MCP servers for
clients to find and install. Ibis is listed as `io.github.ibiscite/ibis`. Nothing is
published yet: publishing needs the maintainer's account, and each release needs its
bundles first.

## What is listed

The registry lists packages, not source. Ibis isn't on npm, PyPI or crates.io, so
its package is an [MCP Bundle](https://github.com/modelcontextprotocol/mcpb)
(`.mcpb`) attached to each GitHub release. The registry accepts bundles only from
GitHub or GitLab releases.

- **One bundle per system:** `ibis-mcp-macOS.mcpb`, `ibis-mcp-Windows.mcpb` and
  `ibis-mcp-Linux.mcpb`. Each is a zip holding the release's `ibis` command
  (`ibis.exe` on Windows), `icon.png` and a `manifest.json`.
- **macOS needs one universal binary.** MCPB chooses by system, not by processor, so
  the macOS bundle holds a binary made with `lipo` from the two macOS builds.
- **The manifest:** `node scripts/mcp-registry.mjs manifest SYSTEM` prints it, from
  [plugins/ibis/mcpb/manifest.json](../plugins/ibis/mcpb/manifest.json) limited to
  that system. It runs `ibis mcp --app-libraries`, so the server reads the libraries
  chosen in the Ibis app. Release Please keeps its version.
- **The registry entry:** `node scripts/mcp-registry.mjs server DIR` prints the
  entry, `server.json`, for the bundles in `DIR`, with each one's SHA-256. The
  registry requires the hash, and clients check it before installing. The entry is
  made at publish time, not committed, because the hashes exist only once the
  bundles are built.

## Still to do

1. **Build the bundles in the release workflow,** beside the `ibis-<system>` archives.
   Zip the signed binary, `brand/` icon and manifest, then attach the three `.mcpb`
   files to the release. This touches release publishing, so it needs one
   independent review (AGENTS.md › Merging).
2. **Publish, with the maintainer's account.** The namespace `io.github.ibiscite`
   belongs to the `ibiscite` organization, and only an owner of that organization can
   publish to it. `mcp-publisher login github` can't yet publish to an organization's
   namespace, so use either a token or the release workflow's GitHub OIDC login:

   ```sh
   node scripts/mcp-registry.mjs server dist > server.json
   mcp-publisher validate
   mcp-publisher login github --token "$TOKEN"   # needs read:org
   mcp-publisher publish
   ```

   Publishing again for each release lists its version. A reverse-DNS name such as
   `org.ibiscite/ibis` would instead need a DNS record or a file on `ibiscite.org`.

The registry is in preview and its format may change, so check `mcp-publisher
validate` before each publish. The schema date is in `scripts/mcp-registry.mjs`.
