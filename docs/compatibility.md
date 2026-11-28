# Compatibility and limits

On macOS and Windows, Enter attempts to paste into the previous app. If the
target or permission cannot be verified, Ibis copies and says why. Linux
copies; paste it yourself. Clipboard managers may retain copied text.

## Interface languages

The app speaks English, German and Arabic: the choice in Settings › General ›
Language, else the first of the system’s preferred languages it speaks. Arabic
mirrors the window. Citations keep their style’s language. The command line and
MCP answer in English.

## Platforms: tested and targeted

These are workflow targets, not certified minimum operating systems. A
passing build does not establish keyboard, display or accessibility behaviour
on another desktop.

| Platform | Packages | Automated native scope |
| --- | --- | --- |
| Windows | x64 | Windows Server 2025 hosted runner, installation and native journey. |
| macOS | Apple silicon and Intel | macOS 15 Apple silicon hosted runner. Intel is cross-built, without an installed-app test. Open at Login uses macOS 13+ APIs. |
| Linux | x64, built on Ubuntu 22.04 | Ubuntu 24.04, Xvfb/Openbox: `.deb` picker smoke and extracted AppImage startup. Native Wayland desktops are not qualified. |

Windows needs Microsoft Edge WebView2. The installer downloads it when
missing; that step needs internet access. Its Microsoft update service is
separate from Ibis's update consent. Install WebView2 in advance for offline
setup.

## Installation blocks

Use packages from the [official releases](https://github.com/ibiscite/ibis/releases)
or [build with Nix](nix.md) (CLI on Linux/macOS, desktop on Linux, both x86_64
and aarch64).
Operating-system signing and updater signatures are separate; see
[code signing](code-signing.md).

- **macOS:** after moving Ibis to Applications, use System Settings › Privacy
  & Security › Open Anyway if Gatekeeper blocks it.
- **Windows:** SmartScreen may offer More info › Run anyway. If system or
  organization policy blocks the unsigned installer, ask the administrator;
  keep security protections enabled.
- **AppImage/FUSE:** use the `.deb` on Debian/Ubuntu or run
  `APPIMAGE_EXTRACT_AND_RUN=1 ./Ibis-Linux.AppImage`. CI checks extracted
  startup, not the full picker through that fallback.
- **Missing `libGLESv2.so.2`:** AppImage needs system graphics libraries. On
  Debian/Ubuntu, `libgles2` and `libegl1` supply them. The `.deb` declares
  WebKitGTK dependencies.

## Linux shortcuts and tray

X11 uses a global shortcut through XKB. Wayland uses the XDG GlobalShortcuts
portal, preferring Ctrl+Shift+Y. Settings › Shortcuts shows the trigger the
desktop reports. Change… uses portal version 2; version 1 points to desktop
settings. Ibis does not save a portal trigger.

Missing permission, cancellation, session loss or an unusable trigger shows
binding instructions. Restart Ibis to retry; it does not poll or retry consent
in the background. Private-bus tests exercise protocol behaviour, not real
GNOME, KDE, Sway or Hyprland focus and consent.

Some desktops have no tray icon. The launcher, picker and More › Quit remain
available. Closing the picker hides it.

### Bind a system shortcut

`ibis toggle` starts the desktop app if needed, shows a hidden picker or hides
an open one, including a pinned picker. The CLI is a separate download. To
choose a build, use `ibis toggle --app PATH`; on macOS an app name also works.
On Linux, bind the desktop executable directly if preferred:

```sh
ibis-desktop toggle
# Nightly .deb
ibis-nightly toggle
# AppImage: use its absolute path
"/path/to/Ibis-Linux.AppImage" toggle
```

Add that command to your desktop's custom shortcuts and choose an unused
combination. Use an absolute path if its PATH differs from your terminal.
Sway can use `bindsym --no-repeat Ctrl+Shift+y exec ibis toggle`.
Test closed → visible → hidden → visible with the same build/channel.

## Picker shortcuts

The default is Command+Shift+Y on macOS and Ctrl+Shift+Y on Windows/X11.
Settings › Shortcuts changes the opening shortcut. Conflicts produce a warning.
An explicitly saved shortcut is kept; unset uses the default.

On macOS, letter shortcuts follow the active keyboard layout and its Command
layer. If no key types the letter, Ibis uses its US-key position and says so.
Non-English layout coverage is incomplete. Avoid Ctrl+Alt on Windows layouts
where it acts as AltGr.

## Library files

Files are read-only. Long-lived clients refresh local files after changes;
network/unknown mounts and failed watches require Reload or reopening the
picker. Download online-only files before choosing them. Files must be UTF-8
and at most 64 MiB. Skipped records produce warnings with line numbers.

| File | Citation keys | Notes |
| --- | --- | --- |
| BibTeX/BibLaTeX (`.bib`, `.bibtex`, `.biblatex`) | Entry keys | Duplicate keys make all entries under that key unavailable. |
| CSL-JSON (`.json`, `.csljson`) | Only `citation-key` | `id` identifies a record, not its citation key. |
| RIS (`.ris`, `.txt`) | None | Formatted citations only. |
| [CSV template](#csv-template) (`.csv`) | None | Formatted citations only. |
| [A folder of PDFs](#a-folder-of-pdfs) | Keys Ibis gives each PDF | Its `ibis-references.bib`, once kept. |

### A folder of PDFs

With no reference manager, a folder of PDFs is a library
([0061](decisions/0061-a-folder-of-pdfs-and-one-resolve.md)). Keep it once with
`ibis sources keep --file FOLDER`: Ibis reads each PDF's own details (its
title, authors, journal, date and DOI as the file states them, in a separate
process) and appends one entry per PDF to `ibis-references.bib` in the folder,
with a key and the PDF as its file. Then `--file FOLDER` opens that file like
any `.bib`; Zotero, JabRef and Overleaf import it with its PDFs. Keeping again
adds only new PDFs, never changes an entry and never touches a PDF, so your own
edits to the file stay. Subfolders are read four levels deep; links and hidden
files are left out. A PDF whose details say nothing is still listed and its
text searchable, but it is not cited until its entry has a title, author or
editor: any library's reference without one refuses with `missing-details`.
A DOI printed on a first page is never taken as the PDF's own. The desktop app
does not open folders yet.

### EndNote

Ibis reads EndNote 21 through its built-in [source adapter](source-adapters.md#endnote):
an open library on macOS (`--source endnote=Invented.enl`) or an XML export on
every system (`--source endnote=export.xml`). Journal articles, books and book
sections are read; other types are skipped and named. EndNote has no citation
keys, so citations are formatted, and key outputs refuse. The desktop app does
not read EndNote yet. On macOS, allow the program running `ibis` to control
EndNote when asked, or later in System Settings › Privacy & Security ›
Automation.

### RIS from a reference manager

Choose Zotero directly or a bibliography manager's `.bib` where available.
For other managers, export RIS and replace that file after library changes.
Ibis reads reference metadata, not RIS notes, attachments, abstracts or
keywords. Unknown types and conflicting titles, years or page ranges skip
that record. Scopus `CONF` records are skipped. PubMed `.nbib` is not RIS.

### CSV template

Save as UTF-8 CSV with commas or semicolons. The header is case-insensitive;
`type` and `title` are required. Unknown or duplicate columns refuse the file.
Rows with missing required values, unknown CSL types, wrong cell counts or
unclosed quotes are skipped.

| Column | Value |
| --- | --- |
| `type`, `title` | CSL type (for example `book` or `article-journal`) and title. |
| `author`, `editor` | People separated by `;`, as `Family, Given` or a whole organisation name. |
| `issued` | `2026`, `2026-03` or `2026-03-14`; other text stays literal. |
| `container-title`, `collection-title` | Journal/book/site and series. |
| `volume`, `issue`, `page`, `number`, `edition` | Literal values. |
| `publisher`, `publisher-place` | Publisher and city. |
| `DOI`, `ISBN`, `ISSN`, `URL` | Identifiers; invalid DOI values warn and are omitted. |

```csv
type,title,author,issued,container-title,volume,page,DOI
article-journal,Invented tides,"Doe, Jane; Roe, Rafael",2026-03-14,Journal of Invented Studies,12,101-118,10.0000/invented.1
```

## Writing apps

`ibis outputs` lists shipped outputs and forms. This table explains what the
text means; it is not certification of every writing app.

| Destination | Output | Requirement |
| --- | --- | --- |
| Markdown, Quarto, R Markdown (RStudio, Positron), Obsidian | Pandoc keys | Matching bibliography when building the document, or the [Ibis filter](#pandoc-quarto-and-obsidian). |
| LaTeX (TeXShop, TeXstudio, Texifier), Overleaf | BibLaTeX, Natbib or LaTeX keys | Matching project `.bib`. |
| Typst | Typst keys | Matching bibliography. |
| Word, LibreOffice, Google Docs, Pages, email | Formatted text | Static citations; they do not join a managed bibliography or update with a document's style. |
| Zotero RTF/ODF Scan add-on | `scannable-cite` markers | References read from Zotero; the separate RTF/ODF Scan add-on, not Zotero's built-in RTF Scan, reads these five-field markers. Not yet qualified end to end. |

Ibis also writes org-cite, MyST, ConTeXt and other key syntaxes. Supported
forms and field losses depend on the output. Numeric styles give an
unnumbered reference entry; note styles give a full first note. Ibis cannot
determine a document's numbering, ibid. or year-letter disambiguation.
Tests prove that Ibis writes the markers the add-on documents
(`{ prefix | Author, Year | locator | suffix | zu:0:KEY }`, `zg:GROUP:KEY` for a group); they do not
prove that a scan turns them into live Zotero citations in a particular
Zotero, add-on or word-processor version. Zotero's built-in RTF Scan reads
author-date placeholders such as `{Smith, 2009}` instead.

In a browser on macOS or Windows, the website in front chooses the format
when Ibis can read its site name: Overleaf and the Typst web app take keys,
Google Docs, Gmail and Outlook on the web take formatted text, and a format you
choose on any site is remembered for that site. Without the macOS
Accessibility permission, on Linux, or when the site cannot be read, the
browser is one app.

## Editors (Cite as You Write)

Switch on Settings › Citations › Cite from your editor, copy its address and
configure the editor's full CAYW URL. A client that accepts only a Zotero port
cannot use it. [Supported parameters](parity.md#better-bibtex-cite-as-you-write)
are listed separately.

Requests reconstructed from VS Code Citation Picker for Zotero 0.2.1 are
replayed by `cayw_clients.rs`. This is protocol evidence, not an interactive
VS Code acceptance test. Its Check Connection and PDF commands require
Better BibTeX JSON-RPC, which Ibis does not provide.

## Pandoc, Quarto and Obsidian

Two small plugins in the repository run the `ibis` command and hold no
citation logic of their own:

- The [Pandoc and Quarto filter](../plugins/pandoc/README.md) fills in the
  references a document cites from the libraries Ibis reads, Zotero
  included, before citeproc formats them (in Quarto, with `citeproc: true`).
  A project's own `bibliography` and `references` win over it. It needs
  Pandoc 3.1.1 or later.
- The [Obsidian plugin](../plugins/obsidian/README.md) adds **Insert
  citation**, which searches with Ibis and inserts the citation in the format
  Ibis uses for Obsidian. It runs on Obsidian's desktop apps only and is not
  yet in Obsidian's community directory, so it is installed by copying two
  files.

`fidelity.rs` renders a document through Pandoc 3.1 twice, once from the
`.bib` and once through the filter, and requires the same text.
`obsidian.rs` runs the plugin's own tests against the built command with a
stand-in for Obsidian's interface. Quarto 1.11 was checked by hand on macOS on
2026-11-28: with `citeproc: true` in the front matter, the references came from
a `.bib` named in `ibis:` and `@fig-plot` stayed a figure cross-reference.
RStudio and the Obsidian app are not yet checked.

## What the tests establish

[Testing](testing.md#native-journeys-and-installed-updates) describes the current native assertions.
A completed passing run establishes only its recorded scope. Hosted X11,
builds and protocol fakes do not prove native Wayland, screen-reader speech,
mixed-DPI displays, cloud/network provider behaviour or every editor.
