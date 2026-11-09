<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="brand/exports/ibis-logo-on-dark.svg">
    <img src="brand/exports/ibis-logo.svg" alt="Ibis" width="240">
  </picture>
</h1>

<p align="center"><strong>Your next citation, a shortcut away.</strong><br>Search your libraries and cite where you write.<br><a href="https://ibiscite.org">ibiscite.org</a></p>

Ibis is a small, free app for researchers, students and anyone who writes with references. Press one shortcut in any app, type a few words, choose a reference from your Zotero library or a BibTeX file, and Ibis gives you the citation for your document.

**Download:** [Mac (Apple silicon)](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-macOS-Apple-silicon.dmg) · [Mac (Intel)](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-macOS-Intel.dmg) · [Windows](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-Windows-setup.exe) · [Linux](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-Linux.AppImage). Ibis is an early preview, so expect rough edges and a warning from your computer [the first time you open it](#install).

No account, no uploads, no telemetry; sources read without changes. [Privacy](PRIVACY.md) covers runtime data and optional update checks.

## What it does

- **Opens from any app.** Press **Ctrl+Shift+Y** on Windows or Linux, or **Command+Shift+Y** on a Mac. If another app already uses that shortcut, Ibis tells you, and a shortcut you have set yourself is kept. Linux desktops that use Wayland cannot use this shortcut yet; Ibis Nightly lets you [set one in your desktop's settings](docs/compatibility.md#bind-a-system-shortcut).
- **Finds the reference.** Search your Zotero library and any group libraries you choose (Zotero 7–10, open or closed), or a reference file: BibTeX or BibLaTeX (`.bib`, `.bibtex`) or CSL-JSON (`.json`, `.csljson`).
- **Gives you the citation you need.** A citation key for Pandoc, BibLaTeX, LaTeX or Typst; a formatted citation in APA, Chicago, Harvard or MLA; or an IEEE reference entry. A citation key is a short label for a reference, such as `vale2024lanterns`; those tools use it to find the full reference. Ibis remembers a format per app, and you change it in the picker. When Ibis can't tell the app, a format you pick becomes your default.
- **Gets it into your document.** Press **Enter**. On a Mac, Ibis pastes for you once you allow it, and the guide is in **Settings > General > Pasting**. On Windows and Linux, or when pasting is not possible, Ibis copies the citation and you paste it yourself.
- **Leaves your library alone.** Ibis works offline after setup and never changes your library.
- **Nightly only, until the next release:** start Ibis when you log in, and paste a DOI, ISBN, PubMed ID (`PMID:…`) or arXiv ID to find that record in your library (Ibis does not fetch it from the internet).

Ibis does not build the reference list for your whole document yet. See the [roadmap](ROADMAP.md) for what comes next, including pasting on Windows, any citation style, exporting a document's bibliography and checking a manuscript's citations.

## Install

There are two builds. Both are free, run on Windows, macOS and Linux, and install as separate apps, so you can keep both.

- **Ibis** is the numbered release, and the one to start with: [Mac (Apple silicon)](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-macOS-Apple-silicon.dmg) · [Mac (Intel)](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-macOS-Intel.dmg) · [Windows](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-Windows-setup.exe) · [Linux AppImage](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-Linux.AppImage) ([all releases](https://github.com/ibiscite/ibis/releases)).
- **Ibis Nightly** is where new work lands first. It is rebuilt often and may be unfinished: [Mac (Apple silicon)](https://github.com/ibiscite/ibis/releases/download/nightly/Ibis-macOS-Apple-silicon.dmg) · [Mac (Intel)](https://github.com/ibiscite/ibis/releases/download/nightly/Ibis-macOS-Intel.dmg) · [Windows](https://github.com/ibiscite/ibis/releases/download/nightly/Ibis-Windows-setup.exe) · [Linux AppImage](https://github.com/ibiscite/ibis/releases/download/nightly/Ibis-Linux.AppImage) ([release page](https://github.com/ibiscite/ibis/releases/tag/nightly)).

On Debian or Ubuntu you can also use the `.deb` on either release page. See [compatibility](docs/compatibility.md) for tested systems and help with Linux.

**The first time you open it.** The apps are not signed by Apple or Microsoft yet, so your system warns before opening them:

- **macOS:** open the `.dmg`, drag Ibis to Applications and open it. If macOS says it cannot check the app, open **System Settings > Privacy & Security**, scroll down and choose **Open Anyway**.
- **Windows:** run the installer. For a SmartScreen warning, check that the file came from the release page above; if you trust it and **More info > Run anyway** is available, you can continue. Smart App Control and work or school policies may block it without that option. For Smart App Control, [tell us](https://github.com/ibiscite/ibis/issues/new/choose) and wait for a trusted, signed build; there is no per-app exception. For a managed device, ask IT to review and approve Ibis. Keep protection enabled. If WebView2 Runtime is missing, the installer downloads it once, which needs an internet connection.
- **Linux:** make the AppImage executable (`chmod +x Ibis-Linux.AppImage`, or tick "Allow executing" in its file properties) and open it.

On first start, Ibis asks whether to check for updates. If you say yes, it can update itself; updates are signed and checked before they install. The menu bar or tray menu shows which build and version you are running.

Some Linux desktops, including GNOME without a tray extension, show no tray icon. Use the launcher to reopen Ibis and the picker's **More** menu for library choices and Quit. Nightly also has its start-at-login switch there. Builds with updates enabled expose update controls in **More** too.

## First use

1. Press the shortcut and choose your library: **Zotero**, or a **BibTeX, BibLaTeX, CSL-JSON, RIS or CSV file** ([which files](docs/compatibility.md#library-files)). **Look for libraries on this computer** checks, only when you ask, for a running Zotero and for `.bib` files in your Documents, Desktop and Downloads folders.
2. Search by author, title, year, journal, citation key or keywords. You can combine words from different fields. Use the arrow keys to pick a reference.
3. Press **Enter**. On a Mac this pastes into your document; if permission is needed, follow the guide in **Settings > General > Pasting**. On Windows and Linux, or when pasting is unavailable, Ibis copies the citation and you paste it yourself.

**Shift+Enter** adds several references to one citation. Press **Ctrl+/** (**⌘/** on a Mac) to see every key.

To change the format, use the control beside search. The format belongs to the app you came from, and choosing another one remembers it for that app. When Ibis can't tell the app, a format you pick becomes your default. **Settings › Citations › Default format** applies to apps with no remembered format.

**Nightly only, until the next release:** add a locator at the end of a search,
such as `memory p 12–14`, `memory ch. 3` or `memory S. 12`. Ibis shows how it interpreted the locator; **Search as typed**
searches the complete text instead. Each reference you add keeps its own locator
through later searches, preview and copy. To change an already added reference,
highlight it and choose **Apply locator**; its locator's remove button clears
only the locator. Pages, chapters, sections, paragraphs, figures, lines, notes,
volumes and verses are supported. Times such as `01:23` stay literal search text.

## Use with Zotero

Choose **Zotero** during setup, or **Use Zotero** in the picker's More menu (or **Use Zotero Library** in the menu bar or tray menu). Ibis reads Zotero 7–10, whether Zotero is open or closed. It reads your personal library and any group libraries you choose, on your computer only, and checks for changes each time the picker opens. It never changes anything in Zotero.

If any selected library cannot be read, Ibis keeps searching its last good copy of all of them and tells you it could not update. Libraries that did respond wait until all of them can be read again.

To read while Zotero is open, Ibis needs Zotero's permission: in Zotero, open **Settings > Advanced** and tick **Allow other applications on this computer to communicate with Zotero**. Ibis reads Zotero's Citation Key field, a `Citation Key:` line in Extra and Better BibTeX keys. If the keys disagree, Ibis shows the conflict and refuses key output until you resolve it in Zotero.

If you prefer a file, let Zotero keep a `.bib` file up to date instead:

1. Install the [Better BibTeX](https://retorque.re/zotero-better-bibtex/installation/) plugin for Zotero.
2. In Zotero, right-click **My Library** (or a collection) and choose **Export Library…** (or **Export Collection…**).
3. Choose the **Better BibLaTeX** format, tick **Keep updated**, and save the file somewhere that stays put.
4. In Ibis, choose that file as your library.

Better BibTeX rewrites the file whenever your library changes, and Ibis reloads it automatically. Ibis only reads the file; it never changes Zotero or the export.

## Help and feedback

Ask questions and suggest ideas in [GitHub Discussions](https://github.com/ibiscite/ibis/discussions). Report a problem through [the issue form](https://github.com/ibiscite/ibis/issues/new/choose). The app's menu has **Report a Problem** and **Suggest an Idea** too. No GitHub account? Email [diyanko@ibiscite.org](mailto:diyanko@ibiscite.org).

Please use invented examples: what you post on GitHub is public. Ibis has one maintainer, so replies can take a while.

## How Ibis is built

Ibis has one maintainer, who decides what it does and what ships. AI agents do most of the engineering under that direction, and people test the real app. [CONTRIBUTING.md](CONTRIBUTING.md#how-ibis-is-built) says what that means for you. You do not need to be a programmer to help: reporting an app where pasting fails, adding an invented test example or improving the documentation all count.

## Command line

You do not need the command line to use the app. `ibis` is the engine itself, for people, scripts and AI agents. On macOS and Linux, install it with `brew install ibiscite/tap/ibis`, or download it from the [nightly](https://github.com/ibiscite/ibis/releases/tag/nightly) or a [release](https://github.com/ibiscite/ibis/releases/latest). The [command line reference](docs/cli.md) has the details and everything else it can do.

```sh
ibis search "lantern" --file refs.bib
ibis cite vale2024lanterns --file refs.bib --style apa
ibis cite vale2024lanterns fen2023clouds --file refs.bib --format autocite
```

`ibis --help` lists every command. The [terminal kit](crates/ibis-cli/README.md) adds an fzf picker recipe, shell completions and diagnostics. The [Ibis plugin for Claude Code and Codex](docs/cli.md#ai-agents-mcp) lets an AI agent cite only what your library holds.

## License

Ibis is licensed under the [GNU Affero General Public License v3.0 or later](LICENSE). Contributions use the [Developer Certificate of Origin](https://developercertificate.org/). See also [PRIVACY.md](PRIVACY.md), [SECURITY.md](SECURITY.md), [CONTRIBUTING.md](CONTRIBUTING.md), [TRADEMARKS.md](TRADEMARKS.md) and the [code signing policy](docs/code-signing.md).

Citation styles come from the [Citation Style Language](https://citationstyles.org/) project (CC BY-SA 3.0).
