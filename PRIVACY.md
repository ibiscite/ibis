# Privacy and data flows

Ibis reads your libraries without changing them. It has no account or
crash-upload service. This page describes current data flows; plans live in
[ROADMAP.md](ROADMAP.md).

## What Ibis reads and writes

- **Libraries:** chosen files or selected Zotero personal/group libraries.
  Setup can inspect Zotero configuration and its local API greeting. When
  asked to look for libraries, it searches names and modification times of
  `.bib` files under Documents, Desktop and Downloads; it opens a file only
  after selection. Closed Zotero databases and Better BibTeX key files are
  read from private temporary copies that are removed after use.
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
- **Windows paste:** checks foreground window/process, integrity and password
  field status through UI Automation. Uncertain or disallowed targets copy
  instead. Delivery safeguards are in [architecture](docs/architecture.md#desktop-app).
- **Settings:** `settings.json` holds library choices, formats, appearance,
  shortcuts, update consent, optional project names/paths and selections per
  app. Atomic saves use `.tmp`, `.bak` and a writer `.lock`. `apps.json` keeps
  app paste choices, including an app-location fingerprint for unknown-app
  approval. [Backups](docs/moving-to-a-new-computer.md) exclude device-local
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

| Flow | Destination and consent |
| --- | --- |
| Running Zotero | `127.0.0.1:23119`, after choosing Zotero. Reads metadata and Better BibTeX keys; requests stay on this computer. |
| Editor endpoint | Off by default. When enabled, listens on `127.0.0.1:23124`; token-protected, rejects browsers. [Protocol](docs/parity.md#better-bibtex-cite-as-you-write). |
| App updates | Automatic checks require consent; Check for Updates… checks on request. GitHub receives IP address and app version, not library data. Downloads require agreement and a valid channel signature. |
| Windows installation | Downloads Microsoft's WebView2 bootstrapper/runtime if missing. Microsoft's runtime updates are separate. |
| Feedback | Opens GitHub in your browser with app version and OS; you decide whether to submit. |
| AI clients | `ibis mcp` passes bibliographic fields and citations over stdio to its caller. That client may send them to its own service. [MCP privacy boundary](docs/cli.md#ai-agents-mcp). |

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
