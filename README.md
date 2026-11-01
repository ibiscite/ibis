<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="brand/exports/ibis-logo-on-dark.svg">
    <img src="brand/exports/ibis-logo.svg" alt="Ibis" width="240">
  </picture>
</h1>

<p align="center"><strong>Your next citation, a shortcut away.</strong><br>Search your libraries and cite where you write.<br><a href="https://ibiscite.org">ibiscite.org</a></p>

Ibis is free and open source. Searching and copying work offline after setup. Ibis adds no analytics and never changes your libraries. See [privacy and local storage](PRIVACY.md) for runtime data and optional update checks.

## What it does today

Ibis is an early preview.

- Open the picker with **Ctrl+Shift+Y** on Windows or Linux/X11, or **Command+Shift+Y** on macOS, if the shortcut is available. Existing custom shortcuts are kept. **Nightly, until the next release:** on Wayland, [bind `ibis toggle` in your desktop](docs/compatibility.md#bind-a-system-shortcut); it starts Ibis or shows/hides the running picker. Use `ibis toggle --app "Ibis Nightly"` for macOS Nightly, or `--app <executable-path>` on Linux/Windows. Portal shortcuts ([#782](https://github.com/ibiscite/ibis/issues/782)) have not shipped.
- Search a BibTeX/BibLaTeX (`.bib`, `.bibtex`) or CSL-JSON (`.json`, `.csljson`) file, or your Zotero library and any group libraries you choose (Zotero 7–10, open or closed).
- Cite with a key for Pandoc, BibLaTeX, LaTeX or Typst, a formatted citation in APA, Chicago, Harvard or MLA, or an IEEE reference entry. On macOS, output is chosen per app; on Windows and Linux, choosing a format changes the global default. Change the output in the picker.
- Press **Enter** to paste on macOS. Elsewhere, or when pasting is unavailable, Ibis copies. The paste permission guide is in **Settings > General > Pasting**.
- Works offline and never changes your library.
- **Nightly only, until the next release:** start Ibis when you log in, and paste a DOI, ISBN, PubMed ID (`PMID:…`) or arXiv ID to find that record in your library (Ibis does not fetch it from the internet).

See the [roadmap](ROADMAP.md) for what comes next, including pasting on Windows, any citation style, exporting a document's bibliography and checking a manuscript's citations.

## Install

There are two builds. Both are free, run on Windows, macOS and Linux, and install as separate apps, so you can keep both.

- **Ibis** is the numbered preview release: [Mac (Apple silicon)](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-macOS-Apple-silicon.dmg) · [Mac (Intel)](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-macOS-Intel.dmg) · [Windows](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-Windows-setup.exe) · [Linux AppImage](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-Linux.AppImage) ([all releases](https://github.com/ibiscite/ibis/releases)).
- **Ibis Nightly** has the newest work and is rebuilt regularly from main, so it may be unfinished: [Mac (Apple silicon)](https://github.com/ibiscite/ibis/releases/download/nightly/Ibis-macOS-Apple-silicon.dmg) · [Mac (Intel)](https://github.com/ibiscite/ibis/releases/download/nightly/Ibis-macOS-Intel.dmg) · [Windows](https://github.com/ibiscite/ibis/releases/download/nightly/Ibis-Windows-setup.exe) · [Linux AppImage](https://github.com/ibiscite/ibis/releases/download/nightly/Ibis-Linux.AppImage) ([release page](https://github.com/ibiscite/ibis/releases/tag/nightly)).

On Linux you can also use the `.deb` on either release page. See [compatibility](docs/compatibility.md) for tested systems, runtime requirements and AppImage FUSE help.

**The first time you open it.** The apps are not signed by Apple or Microsoft yet, so your system warns before opening them:

- **macOS:** open the `.dmg`, drag Ibis to Applications and open it. If macOS says it cannot check the app, open **System Settings > Privacy & Security**, scroll down and choose **Open Anyway**.
- **Windows:** run the installer. For a SmartScreen warning, check that the file came from the release page above; if you trust it and **More info > Run anyway** is available, you can continue. Smart App Control and work or school policies may block it without that option. For Smart App Control, [tell us](https://github.com/ibiscite/ibis/issues/new/choose) and wait for a trusted, signed build; there is no per-app exception. For a managed device, ask IT to review and approve Ibis. Keep protection enabled. If WebView2 Runtime is missing, the installer downloads it once, which needs an internet connection.
- **Linux:** make the AppImage executable (`chmod +x Ibis-Linux.AppImage`, or tick "Allow executing" in its file properties) and open it.

On first start, Ibis asks whether to check for updates. If you say yes, it can update itself; updates are signed and checked before they install. The menu bar or tray menu shows which build and version you are running.

Some Linux desktops, including GNOME without a tray extension, show no tray icon. Use the launcher to reopen Ibis and the picker's **More** menu for library choices and Quit. Nightly also has its start-at-login switch there. Builds with updates enabled expose update controls in **More** too.

## First use

1. Press the shortcut. Choose **Zotero** or a **BibTeX, BibLaTeX or CSL-JSON file**. **Look for libraries on this computer** checks for running Zotero and `.bib` files in your Documents, Desktop and Downloads folders when asked. On macOS, the format beside search follows the app you came from; choose another format there to remember it for that app. On Windows and Linux, choosing a format changes the global default. Use **Just this once** for one citation.
2. Search by author, title, year, journal, citation key or keywords. You can combine words from different fields. Use the arrow keys to pick a reference.
3. Press **Enter** to paste into your document on macOS. If permission is needed, follow the guide in **Settings > General > Pasting**. On Windows and Linux, or when pasting is unavailable, Ibis copies the citation; paste it into your document yourself. **Shift+Enter** adds several references to one citation. Press **Ctrl+/** (**⌘/** on macOS) to see every key.

Hide or show the citation output with **Show what will be pasted** in **More**, or **Ctrl+I** (**⌘I** on macOS). Ibis remembers this choice; the references you added stay selected.

**Nightly only, until the next release:** add a locator at the end of a search,
such as `memory p 12–14`, `memory ch. 3` or `memory S. 12`. Ibis shows how it interpreted the locator; **Search as typed**
searches the complete text instead. Each reference you add keeps its own locator
through later searches, preview and copy. To change an already added reference,
highlight it and choose **Apply locator**; its locator's remove button clears
only the locator. Pages, chapters, sections, paragraphs, figures, lines, notes,
volumes and verses are supported. Times such as `01:23` stay literal search text.

## Use with Zotero

Choose **Zotero** during setup, or **Use Zotero** in the picker's More menu (or **Use Zotero Library** in the menu bar or tray menu). Ibis reads Zotero 7–10: through its local API while Zotero runs, and from a private copy of its database while it is closed. It reads your personal library and any group libraries you choose, on your computer only, and checks for changes each time the picker opens. It never changes anything in Zotero.

If any selected library fails during a refresh, Ibis keeps the whole previous
snapshot searchable and reports that it could not be updated. Even libraries
that did respond wait until all selected libraries can be read again.

To read while Zotero runs, turn on its local API: in Zotero, open **Settings > Advanced** and tick **Allow other applications on this computer to communicate with Zotero**. Ibis reads Zotero's Citation Key field, a `Citation Key:` line in Extra and Better BibTeX keys. If the keys disagree, Ibis shows the conflict and refuses key output until you resolve it in Zotero.

If you prefer a file, let Zotero keep a `.bib` file up to date instead:

1. Install the [Better BibTeX](https://retorque.re/zotero-better-bibtex/installation/) plugin for Zotero.
2. In Zotero, right-click **My Library** (or a collection) and choose **Export Library…** (or **Export Collection…**).
3. Choose the **Better BibLaTeX** format, tick **Keep updated**, and save the file somewhere that stays put.
4. In Ibis, choose that file as your library.

Better BibTeX rewrites the file whenever your library changes, and Ibis reloads it automatically. Ibis only reads the file; it never changes Zotero or the export.

## Help and feedback

Ask questions and suggest ideas in [GitHub Discussions](https://github.com/ibiscite/ibis/discussions). Report a problem through [the issue forms](https://github.com/ibiscite/ibis/issues/new/choose). The app's menu has **Report a Problem** and **Suggest an Idea** too.

## Command line

`ibis` is the engine itself, for people, scripts and AI agents. Download it from the [nightly](https://github.com/ibiscite/ibis/releases/tag/nightly) or a [release](https://github.com/ibiscite/ibis/releases/latest): `ibis-macOS-Apple-silicon.tar.gz`, `ibis-macOS-Intel.tar.gz`, `ibis-Windows.zip` or `ibis-Linux.tar.gz`. Unpack it and put `ibis` somewhere on your PATH. The macOS binary is not signed yet; if macOS refuses to run a downloaded copy, run `xattr -d com.apple.quarantine ibis` once.

```sh
ibis search "garcia" --file refs.bib
ibis cite accents2021 --file refs.bib --style apa
ibis cite accents2021 protected2020 --file refs.bib --format autocite
```

`ibis --help` lists every command. From a checkout, `cargo run -p ibis -- <command>` does the same, for example with the synthetic `fixtures/bibtex/fidelity.bib`:

```sh
cargo run -p ibis -- libraries --file fixtures/bibtex/fidelity.bib --json
cargo run -p ibis -- schema cite --json
cargo run -p ibis -- call search --params '{"query":"garcia"}' --file fixtures/bibtex/fidelity.bib --json
cargo run -p ibis -- serve --stdio --file fixtures/bibtex/fidelity.bib
cargo run -p ibis -- mcp --file fixtures/bibtex/fidelity.bib
```

The [terminal kit](crates/ibis-cli/README.md) includes an fzf picker recipe,
`ibis completions bash|zsh|fish` and read-only `ibis doctor --json` diagnostics.

The command line calls the same typed engine methods as the app. Every command
accepts `--json` and then writes one versioned envelope to stdout, with stable error
codes and exit codes. The [command line reference](docs/cli.md) covers libraries,
commands, JSON output, exit codes, canonical citation requests, the JSON-RPC server
and `ibis mcp` for AI agents. In Claude Code, `/plugin install ibis --marketplace ibiscite/ibis`
adds the Ibis plugin: the MCP server and a skill that cites only what your library holds.

## Contributing

You do not need to be a programmer to help. Reporting an app where pasting fails, adding a test example, or improving documentation are all valuable. AI-assisted contributions are welcome under clear rules. Start with [CONTRIBUTING.md](CONTRIBUTING.md). See [testing](docs/testing.md) for what the checks prove and what comes next.

## License

Ibis is licensed under the [GNU Affero General Public License v3.0 or later](LICENSE). Contributions use the [Developer Certificate of Origin](https://developercertificate.org/). See also [PRIVACY.md](PRIVACY.md), [SECURITY.md](SECURITY.md), [CONTRIBUTING.md](CONTRIBUTING.md), [TRADEMARKS.md](TRADEMARKS.md) and the [code signing policy](docs/code-signing.md).

Citation styles come from the [Citation Style Language](https://citationstyles.org/) project (CC BY-SA 3.0).
