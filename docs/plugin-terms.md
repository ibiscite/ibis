# Ibis software and plugin notice

Ibis is free, open-source software licensed under the
[GNU Affero General Public License, version 3 or later](../LICENSE).
This notice describes Ibis and its plugin; it does not replace the licence or
add conditions to it. The licence includes its warranty and liability terms.

## Local software

The Ibis plugin runs the `ibis` command on your computer. It needs a connected
Ibis MCP server and reads the reference libraries you choose. It searches and
formats citations without changing those libraries. It is not a hosted citation
service operated by the Ibis contributors.

`ibis agents setup` generates local adapters and registers them in the selected
AI apps' settings. Generated adapters launch the selected Ibis executable by
its absolute path. The setup command uses no network. Claude Desktop asks for
its own extension installation confirmation; some clients finish installation
in their Plugins screen. Ownership records contain Ibis's own entries, and
one-time `*.ibis-backup` files preserve existing client settings. Those backups
can contain the original settings' private values and remain on this computer.

## Data and other services

The MCP server passes bibliographic data and citations to the AI client that
started it. It excludes notes, annotations, source paths and local links. The
AI client may send what it receives to its own service, under that provider's
terms and privacy policy. The plugin includes no Ibis project API credentials
and does not provide an AI subscription or API credits.

See [Privacy and data flows](../PRIVACY.md) for Ibis's local storage, diagnostics,
Zotero access and optional app update checks. The existing
[name and logo policy](../TRADEMARKS.md) also remains available.

## Help

Report problems or ask questions through
[Ibis's GitHub issues](https://github.com/ibiscite/ibis/issues).
Omit private library contents and credentials from public reports.
