# Ibis

**Cite anything, anywhere.** Press a shortcut in any app, find a reference from your libraries in any language, and get a correct citation into whatever you are writing. If Ibis cannot insert it safely, it copies it and tells you why. It never fails silently.

Ibis is free, open source and private by design: everything works offline, nothing is tracked, and your libraries are never changed.

> **Status: early preview (0.1).** A global shortcut (Ctrl+Shift+Y, or Control+Command+R on macOS) opens the picker. Search a `.bib` library or, while Zotero is open, your Zotero library and any group libraries you choose (More menu), and copy a citation key (Pandoc, BibLaTeX, LaTeX, Typst) or a formatted citation in APA, Chicago, Harvard, IEEE or MLA. Ibis can start when you log in, so the shortcut is always ready: tick **Start Ibis when you log in** during first-run setup, in the More menu or in the menu bar or tray menu (off until you choose it). Builds are unsigned. More styles, reading Zotero while it is closed and direct insertion follow. See the [roadmap](ROADMAP.md).

## Planned for 1.0

- Works with **Zotero** (open *or* closed), **BibTeX/BibLaTeX**, **RIS** and **CSL-JSON**, with more managers to follow.
- Every **official citation style** (10,000+, including IEEE and APA), kept current, plus citation keys for **LaTeX**, **Pandoc/Markdown** and **Typst**.
- Page numbers per reference, several references at once, and a preview before inserting.
- Fast search by words, in any language and script.
- Inserts into Word, Google Docs, Overleaf, Obsidian, Notion, email, chat and anything else, with clear fallbacks.
- Gentle library health checks: missing data, odd names, duplicate keys.
- Windows, macOS and Linux. Keyboard, mouse and screen-reader friendly.

## Download

There are two builds. Both are free and run on Windows, macOS and Linux.

| | **Ibis** (releases) | **Ibis Nightly** |
|---|---|---|
| What | Numbered previews, tested before release | The newest work from `main`, rebuilt after every change |
| Version | `0.1.0`, `0.1.1`, `0.2.0`, ... | `0.1.1-nightly.20261020123456` (next version, then build time) |
| Get it | [Releases](https://github.com/ibiscite/ibis/releases) (newest numbered release) | [Releases → Ibis Nightly](https://github.com/ibiscite/ibis/releases/tag/nightly) |
| Direct downloads | [Mac (Apple silicon)](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-macOS-Apple-silicon.dmg) · [Mac (Intel)](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-macOS-Intel.dmg) · [Windows](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-Windows-setup.exe) · [Linux AppImage](https://github.com/ibiscite/ibis/releases/download/stable/Ibis-Linux.AppImage) | [Mac (Apple silicon)](https://github.com/ibiscite/ibis/releases/download/nightly/Ibis-macOS-Apple-silicon.dmg) · [Mac (Intel)](https://github.com/ibiscite/ibis/releases/download/nightly/Ibis-macOS-Intel.dmg) · [Windows](https://github.com/ibiscite/ibis/releases/download/nightly/Ibis-Windows-setup.exe) · [Linux AppImage](https://github.com/ibiscite/ibis/releases/download/nightly/Ibis-Linux.AppImage) |

The two install as separate apps, so you can keep both. Each can update itself if you allow update checks when it first starts; updates are signed and checked before they install. The apps themselves are not signed by Apple or Microsoft yet, so your system may warn before opening them (the release page says what to click).

**Version numbers** follow [semantic versioning](https://semver.org). Until 1.0 every release is a preview: `0.N.0` ships a milestone from the [roadmap](ROADMAP.md) and `0.N.1`, `0.N.2`, ... add fixes and small features. Tell us what works and what you miss through [a problem report or an idea](https://github.com/ibiscite/ibis/issues/new/choose).

The menu bar or tray menu shows which build and version you are running. Send questions and feedback to [GitHub Discussions](https://github.com/ibiscite/ibis/discussions), and problems to [the issues](https://github.com/ibiscite/ibis/issues/new/choose).

The first time the picker opens, it offers Zotero if Zotero is open and lists `.bib` files it finds in your Documents, Desktop and Downloads folders, by name only, so you can pick one; **Choose library…** opens any other file. It also asks where you write (Markdown, LaTeX, Typst, or Word and Google Docs) and sets **Copy as** to match.

## Use with Zotero

With Zotero open, choose **Use Zotero** in the picker (or **Use Zotero Library** in the menu bar or tray menu). Ibis reads your personal library through Zotero's local API, on your computer only, and checks it for changes each time the picker opens. It never changes anything in Zotero.

The first time, turn on Zotero's local API: in Zotero, open **Settings > Advanced** and tick **Allow other applications on this computer to communicate with Zotero**. Citation keys come from Zotero's own Citation Key field, or from Better BibTeX if you use it.

If you prefer a file, or Zotero is usually closed, let Zotero keep a `.bib` file up to date instead:

1. Install the [Better BibTeX](https://retorque.re/zotero-better-bibtex/installation/) plugin for Zotero.
2. In Zotero, right-click **My Library** (or a collection) and choose **Export Library…** (or **Export Collection…**).
3. Choose the **Better BibLaTeX** format, tick **Keep updated**, and save the file somewhere that stays put.
4. In Ibis, choose that file as your library.

Better BibTeX rewrites the file whenever your library changes, and Ibis reloads it automatically. Ibis only reads the file; it never changes Zotero or the export.

## Try the command line tool (developers)

```sh
cargo run -p ibis -- search "garcia sleep" --bib fixtures/bibtex/real-world.bib
cargo run -p ibis -- cite mueller2021 vanderberg2019 --page mueller2021=12-14 --format autocite
cargo run -p ibis -- check fixtures/bibtex/damaged.bib
```

## Contributing

You do not need to be a programmer to help. Reporting an app where pasting fails, adding a test example, or improving documentation are all valuable. AI-assisted contributions are welcome under clear rules. Start with [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Ibis is licensed under the [GNU Affero General Public License v3.0 or later](LICENSE). Contributions use the [Developer Certificate of Origin](https://developercertificate.org/). See also [PRIVACY.md](PRIVACY.md), [SECURITY.md](SECURITY.md), [GOVERNANCE.md](GOVERNANCE.md), [TRADEMARKS.md](TRADEMARKS.md) and the [code signing policy](docs/code-signing.md).

Citation styles come from the [Citation Style Language](https://citationstyles.org/) project (CC BY-SA 3.0).
