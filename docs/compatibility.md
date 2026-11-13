# Compatibility and limits

On macOS and Windows, Enter attempts to paste into the previous app. If the
target or permission cannot be verified, Ibis copies and says why. Linux
copies; paste it yourself. Clipboard managers may retain copied text.

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
| Markdown, Quarto, R Markdown | Pandoc keys | Matching bibliography when building the document. |
| LaTeX, Overleaf | BibLaTeX, Natbib or LaTeX keys | Matching project `.bib`. |
| Typst | Typst keys | Matching bibliography. |
| Word, LibreOffice, Google Docs, Pages, email | Formatted text | Static citations; they do not join a managed bibliography or update with a document's style. |
| Zotero RTF/ODF Scan | `scannable-cite` markers | References read from Zotero, then processed by Zotero's scan tools. |

Ibis also writes org-cite, MyST, ConTeXt and other key syntaxes. Supported
forms and field losses depend on the output. Numeric styles give an
unnumbered reference entry; note styles give a full first note. Ibis cannot
determine a document's numbering, ibid. or year-letter disambiguation.
A scan marker being generated does not prove a round trip through a particular
Zotero or word-processor version.

## Editors (Cite as You Write)

Switch on Settings › Citations › Cite from your editor, copy its address and
configure the editor's full CAYW URL. A client that accepts only a Zotero port
cannot use it. [Supported parameters](parity.md#better-bibtex-cite-as-you-write)
are listed separately.

Requests reconstructed from VS Code Citation Picker for Zotero 0.2.1 are
replayed by `cayw_clients.rs`. This is protocol evidence, not an interactive
VS Code acceptance test. Its Check Connection and PDF commands require
Better BibTeX JSON-RPC, which Ibis does not provide.

## What the tests establish

[Testing](testing.md#native-journeys) describes the current native assertions.
A completed passing run establishes only its recorded scope. Hosted X11,
builds and protocol fakes do not prove native Wayland, screen-reader speech,
mixed-DPI displays, cloud/network provider behaviour or every editor.
