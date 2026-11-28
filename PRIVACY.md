# Privacy and data flows

Ibis reads your libraries without changing them, except a change you apply
yourself in the app. It has no account or crash-upload service. This page describes current data flows; plans live in
[ROADMAP.md](ROADMAP.md).

## What Ibis reads and writes

- **Libraries:** chosen files or selected Zotero personal/group libraries.
  Setup can inspect Zotero configuration and its local API greeting. When
  asked to look for libraries, it searches names and modification times of
  `.bib` files under Documents, Desktop and Downloads; it opens a file only
  after selection. Closed Zotero databases and Better BibTeX key files are
  read from one private copy in the app's per-user cache. It remains between
  reads and is replaced when those source files change; clearing the app cache
  removes it. No source database is opened by SQLite or modified by Ibis.
- **Attachment text:** `ibis outline`, `ibis read` and `ibis find-in` read
  the text Zotero keeps for the files it indexed: from Zotero while it runs,
  or from the `.zotero-ft-cache` file in the attachment's storage folder while
  it is closed, with page counts from the private copy of Zotero's database.
  A PDF Zotero has not fully indexed, and a PDF named in a `.bib` file's
  `file` field, is read from the file itself: `ibis` opens it, only if it is a
  regular file on this computer of at most 256 MiB, and hands the opened file
  to a short-lived copy of itself that has no other file, no environment and
  ten seconds to give the text and printed page labels. The desktop app never
  reads PDFs. These commands need no per-library MCP opt-in, so an AI app that can run
  commands can read attachment text this way. The MCP tools `outline`, `read` and `find_in` are
  offered only for libraries where you allow AI apps to read attachment text;
  this is off by default, separate for each library, kept only on this
  computer and not part of a settings backup. Text is held in memory for one
  request, never written to disk, and returned in bounded parts: at most five
  pages or 32 KiB, or ten passages. Ids name an attachment, never its path.
  CLI and MCP attachment text tools refuse files whose resolved path is outside the `.bib` library’s folder or Zotero’s storage folder unless **Allow attachment text outside the library’s folder** is enabled in **Settings › AI Apps › Attachment text** (`attachment_text_outside_library`, device-only and off by default).
- **Proposed changes:** only when an AI app's `ibis mcp` command includes
  `--allow-writes`, its tools can propose adding a reference, changing a
  reference's fields or adding and removing its tags. Each proposal is one
  private file in a `proposals` folder beside `settings.json`, at most 64 KiB
  and at most 100 waiting: what the AI app sent, after the same filter as every
  shared record (no notes, files or private links), the name the app gave
  itself, the library's id and, for a `.bib` file, its path, the reference's
  key and title, and a fingerprint of the entry. Nothing in a library changes
  until you apply the proposal in the Ibis app; the app shows the entry before
  and after, refuses if it changed since, edits only that entry and keeps the
  replaced file as `<file>.ibis-backup-N`. `ibis proposals` lists, shows and
  rejects proposals but cannot apply them. Only `.bib` files take proposals;
  Ibis does not change Zotero libraries.
  An AI app that can run commands can still edit files you can edit.
- **A folder of PDFs:** `ibis sources keep --file FOLDER` reads each new PDF
  in the folder in a separate process and appends what the PDF says about
  itself to `ibis-references.bib` in that folder, keeping the replaced file in
  Ibis's data folder as for a proposal. Nothing is sent anywhere; no PDF and no
  existing entry changes. Opening the folder afterwards only reads that file.
- **Project bibliographies:** chosen files are checked without changes.
  Settings retain their paths; the picker receives only file names.
- **Clipboard:** copying replaces its contents. Clipboard managers, cloud
  clipboard and other apps may retain them. Copy Entry preserves original
  BibTeX fields, including `file` and `note`; inspect it before sharing.
- **macOS paste:** reads app/process and opaque Accessibility document/surface
  identities, plus secure-input state and elapsed time since keyboard/button
  input. It does not read document titles, field values, key contents or click
  coordinates. For permitted apps missing required identities, Ibis can set
  `AXManualAccessibility=true` once per live app instance. This can increase
  that app's resource use and expose its accessibility tree to the OS; it
  does not grant Ibis Accessibility permission.
- **Selected text on macOS:** short selections in eligible apps can become
  the search. Ibis reads at opening and checks again before paste, under
  secure-input and permission guards. Text stays in memory and is not saved
  or logged. Turning pasting off disables this read. It never reads clipboard
  contents to seed the search.
- **Website in front of a browser:** when the picker opens over a browser Ibis
  lists, it reads the front tab's address (macOS: through the Accessibility
  permission, only if already granted; Windows: UI Automation of the address
  field) and keeps only the matching site name in the destination table, such
  as `overleaf.com` or `docs.google.com`, so a format can be remembered per
  listed site. Unlisted hosts use the browser's per-app format; no unlisted
  host label is stored. Nothing is asked for, sent or logged; the path,
  query and full address are dropped at once. A format you choose on a listed
  site is saved for that site name in `settings.json`. Linux does not read it.
- **Windows paste:** checks foreground window/process, integrity and password
  field status through UI Automation. Uncertain or disallowed targets copy
  instead. Delivery safeguards are in [architecture](docs/architecture.md#desktop-app).
- **Command line:** on macOS and Linux the app's first launch links
  `~/.local/bin/ibis` to its bundled command, unless another `ibis` comes first
  on PATH, with a receipt and a `command-settled` marker in
  `~/.local/share/ibis`. Later launches leave the choice in Settings alone;
  Remove Command deletes the link and receipt.
- **AI apps:** `ibis agents setup` and `remove`, run by you, add or remove
  one `ibis` server entry in the AI apps' own settings files (or through their
  own `mcp add` command), keeping a new `<file>.ibis-backup-N` copy of each file
  they change. Backups are created privately and never replace an older backup.
  They read those files only to find that entry, and send nothing.
- **Settings:** `settings.json` holds library choices, formats, appearance,
  shortcuts, update consent, optional project names/paths and selections per
  app, and formats per app or website site name. Atomic saves use `.tmp`, `.bak` and a writer `.lock`. `apps.json` keeps
  app paste choices, including an app-location fingerprint for unknown-app
  approval, with an empty `apps.json.lock` that serializes changes between Ibis
  processes and shows the choices were once saved: if `apps.json` is then
  missing, Ibis pastes into no app until it is chosen again. [Backups](docs/moving-to-a-new-computer.md) exclude device-local
  choices and do not grant paste permission.
- **Library identity:** `library-ids.key` contains a random private key that
  makes opaque IDs repeatable on one computer. Deleting it changes those IDs.
- **Recents:** engine-owned `recents.json` stores only library/item identities
  and app IDs: up to 20 apps and 30 items each, bounded to 256 KiB. No citation
  text, pages, forms, queries or document contents. Unknown apps, samples and
  editor requests do not persist recents. Forget Recent Citations clears
  them; obsolete WebView citation history is removed rather than imported.
- **Cited since opening:** identities and undo stay in memory. Copy/export
  writes a bibliography only when requested.
- **Start at login:** enabling it creates a macOS login item, Windows startup
  value or Linux autostart file. Disabling it removes the launch entry.

Ibis does not save searches. WebViews, OS caches, swap and crash dumps can
contain runtime data. A terminal or launcher may retain standard-error output;
Ibis writes no diagnostic log file. `IBIS_MAC_PASTE_DIAGNOSTICS=1` enables fixed
paste stages, numeric errors/timings and ownership booleans, without text,
paths or native object descriptions. OS reporting follows system settings.

## Network and diagnostics

**Telemetry:** none, ever. Ibis never sends your libraries, search queries, selected references,
documents, clipboard contents or citation history to an Ibis service. Explicit
MCP responses go to the client that started the server, as listed below.

The command line and MCP share a local engine. On Unix a private domain
socket checks the peer's user ID. On Windows a local named pipe checks the
peer's user token. The engine holds loaded libraries and sessions in memory
and has no network listener. Temporary runtime files contain no reference
data. Zotero and MCP keep the data flows listed below.

| Flow | Destination and consent |
| --- | --- |
| Running Zotero | `127.0.0.1:23119`, after choosing Zotero. Reads metadata and Better BibTeX keys; requests stay on this computer. |
| Editor endpoint | Off by default. When enabled, listens on `127.0.0.1:23124`; token-protected, rejects browsers. [Protocol](docs/parity.md#better-bibtex-cite-as-you-write). |
| App updates | Until 1.0, automatic checks are on by default, shown as a switch on the welcome screen before any check runs and in Settings › About; Check for Updates… checks on request. GitHub receives IP address and app version, not library data. Downloads require agreement and a valid channel signature. |
| Windows installation | Downloads Microsoft's WebView2 bootstrapper/runtime if missing. Microsoft's runtime updates are separate. |
| Feedback | Opens GitHub in your browser with app version and OS; you decide whether to submit. |
| DOI lookup | Planned for the native Mac app, not in this release ([0069](docs/decisions/0069-doi-lookup.md)). Off until you agree once. Then a pasted DOI that is not in your library can be looked up at `doi.org`: only that DOI is sent, never your library, search or document. |
| AI clients | `ibis mcp` passes bibliographic fields and citations over stdio to its caller, and attachment text from libraries where you allow it. With `--allow-writes` it also takes proposed changes, kept on this computer as above. That client may send what it receives to its own service. [MCP privacy boundary](docs/cli.md#ai-agents-mcp). |

The web interface has window-scoped native commands rather than file,
clipboard, shell or network plugin permissions. Its CSP limits connections
to IPC. Ibis configures WebView2 to disable background networking and use an
unreachable proxy. CI observes app/child connections for 60 seconds with
updates off; that covers the tested runtime, not every OS service.

## Local storage locations

Stable uses `com.ibiscite.ibis`; Nightly uses `com.ibiscite.ibis.nightly`.
Replace `<id>` below with the channel ID. They have separate settings and
app-specific WebView data.

| OS | Settings | WebView data/cache defaults |
| --- | --- | --- |
| macOS | `~/Library/Application Support/<id>` | `~/Library/WebKit/<id>`, `~/Library/Caches/<id>`, temporary `$TMPDIR/<id>/WebKit`. |
| Windows | `%APPDATA%\<id>` | `%LOCALAPPDATA%\<id>`, including `EBWebView` and its Crashpad reports. |
| Linux | `$XDG_CONFIG_HOME/<id>`, default `~/.config/<id>` | `$XDG_DATA_HOME/<id>`, default `~/.local/share/<id>`. |

XDG overrides, folder redirection and runtime policy can change these paths.
macOS can also keep `<id>.plist` preferences and `<id>.savedState` state.
Updates/installers use temporary files; interrupted operations may leave
staging or previous-app copies. OS crash reports and logs have separate
retention. This is a code/default-path inventory, not a complete runtime
filesystem audit.

## Remove Ibis and its local data

Turn off start at login and quit through More › Quit. Closing only hides the
picker. Save a settings backup if wanted, uninstall the package or delete the
app/AppImage, then remove that channel's settings and WebView folders above.

On Linux, remove `<id>.desktop` from `$XDG_CONFIG_HOME/autostart` (default
`~/.config/autostart`) if present. Windows startup uses the `Ibis` or
`Ibis Nightly` value under
`HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`; a matching
`Explorer\StartupApproved\Run` approval value may remain. Remove only the
app's values. Keep the shared WebView2 Runtime for other apps.

Delete only confirmed Ibis staging files or app-specific crash reports.
System logs, backups, clipboard history and shared caches are governed by
their owners. These steps leave library files and Zotero data intact.

## The website

ibiscite.org is served by Cloudflare. Cloudflare counts visits from its own
request records and shows the project only totals: page views, visits,
countries and referring sites. The website has no analytics script, cookies
or trackers, and the project never sees who you are. GitHub likewise shows
the project only download totals for releases.
