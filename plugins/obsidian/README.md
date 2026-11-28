# Ibis for Obsidian

One command, **Ibis: Insert citation**, searches your references with the
`ibis` command and inserts the citation at the cursor, in the format Ibis uses
for Obsidian: Pandoc keys such as `[@vale2024lanterns]` unless you chose
another in Ibis. Assign it a hotkey in Obsidian's settings.

It needs Obsidian 1.5 or later on a desktop and the `ibis` command
([install](../../docs/cli.md#install)). It does not run on Obsidian's phone
and tablet apps.

## Install

Until the plugin is in Obsidian's community directory, copy `main.js` and
`manifest.json` into `.obsidian/plugins/ibis/` in your vault, then turn on
**Ibis** under Settings › Community plugins.

## Settings

- **Library files:** a `.bib`, CSL-JSON or RIS file per line. Empty means the
  libraries chosen in the Ibis app.
- **Zotero:** also search the local Zotero library, open or closed.
- **ibis command:** the command's full path. Empty finds it on PATH or where
  Ibis (`~/.local/bin`) and Homebrew put it, since Obsidian opened from the
  Dock does not see a terminal's PATH.

To turn the inserted keys into a document, export with Pandoc and the
[Ibis filter](../pandoc/README.md), for example through the Pandoc plugin for
Obsidian.

The plugin is plain JavaScript with no build step and no dependencies, and it
only starts `ibis search` and `ibis cite`. It reads no file itself and opens
no network connection.
