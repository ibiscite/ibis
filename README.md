<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="brand/exports/ibis-logo-on-dark.svg">
    <img src="brand/exports/ibis-logo.svg" alt="Ibis" width="240">
  </picture>
</h1>

<p align="center"><strong>Your next citation, a shortcut away.</strong><br><a href="https://ibiscite.org">ibiscite.org</a></p>

Ibis is a free app that searches your reference libraries and gives you a
citation where you write. No account, no uploads, no telemetry; sources read without changes.
[Privacy](PRIVACY.md) explains local data and optional updates.

## Install

Download the numbered [release](https://github.com/ibiscite/ibis/releases/latest) or
[Ibis Nightly](https://github.com/ibiscite/ibis/releases/tag/nightly).
They install as separate apps. Both are early previews.
See [how to recognise official Ibis](TRADEMARKS.md#how-to-recognise-official-ibis).

| System | Ibis download |
| --- | --- |
| macOS Apple silicon | [Disk image](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-macOS-Apple-silicon.dmg) |
| macOS Intel | [Disk image](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-macOS-Intel.dmg) |
| Windows x64 | [Installer](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-Windows-setup.exe) |
| Linux x64 | [AppImage](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-Linux.AppImage); `.deb` on the release page |

On macOS, drag Ibis to Applications. On Windows, run the installer. On Linux,
make the AppImage executable (`chmod +x Ibis-Linux.AppImage`) and open it.
See [installation help and tested systems](docs/compatibility.md) for security
warnings, WebView2, FUSE and Wayland shortcuts.

[Install with Nix](docs/nix.md): `nix profile add github:ibiscite/ibis` installs
the CLI on Linux and macOS (x86_64 and aarch64). Add `#ibis-desktop` for the
Linux desktop app.

## First use

1. Open Ibis with Command+Shift+Y on macOS or Ctrl+Shift+Y on Windows/X11.
   Wayland uses the desktop shortcut portal or a
   [system binding](docs/compatibility.md#bind-a-system-shortcut).
2. Choose Zotero or a [library file](docs/compatibility.md#library-files).
   Sample references let you try Ibis without a library.
3. Search by author, title, year, key or identifier. Pick with the arrow keys.
   Shift+Enter adds references to a citation.
4. Choose the output beside search, then press Enter. macOS and Windows
   attempt to paste; when unavailable, Ibis copies and explains why. Linux
   copies. The macOS permission guide is in Settings › General › Pasting.

The format menu offers key syntaxes and formatted citations. A remembered
format belongs to the app you came from; an unidentified app uses the default.
An optional writing project adds its output and bibliography check. The
Inspector edits page, form, prefix and suffix. Command+/ (Ctrl+/ elsewhere)
shows the keys.

Formatted citations are static text. Keys need a matching bibliography in
your document's tools. [Writing-app limits](docs/compatibility.md#writing-apps)
and [citation features](docs/parity.md) explain the differences.

## Use with Zotero

Ibis reads Zotero 7–10, open or closed, including selected group libraries.
For a running Zotero, enable Settings › Advanced › Allow other applications
on this computer to communicate with Zotero.

Keys come from Zotero's Citation Key field, Extra or Better BibTeX. Conflicts
block key output until resolved in Zotero. You can also choose a `.bib` export;
Ibis reads local changes automatically where file notifications work.

## Command line

The separate `ibis` program works without the desktop app:

```sh
ibis search lantern --file refs.bib
ibis cite vale2024lanterns --file refs.bib --style apa
```

See the [CLI reference](docs/cli.md) for installation, JSON, projects,
JSON-RPC and the [agent plugin](docs/cli.md#ai-agents-mcp).
Connect your AI assistant with the [setup guide](https://ibiscite.org/setup),
or paste this prompt:

```text
Set up Ibis for me: https://ibiscite.org/setup
```

## Help and feedback

Use [GitHub issues](https://github.com/ibiscite/ibis/issues/new/choose),
[Discussions](https://github.com/ibiscite/ibis/discussions) or
[diyanko@ibiscite.org](mailto:diyanko@ibiscite.org). Public reports need
invented examples; omit private library records and document text.

[Contributing](CONTRIBUTING.md) explains development and AI assistance.
Future work lives in [ROADMAP.md](ROADMAP.md).

## License

[AGPL-3.0-or-later](LICENSE). See [notices](NOTICE.md),
[name and logo policy](TRADEMARKS.md), [security](SECURITY.md) and
[plugin notice](docs/plugin-terms.md).
