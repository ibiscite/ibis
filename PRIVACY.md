# Privacy and data flows

Ibis reads your libraries without changing them. This page describes the preview
and its local data. New Ibis data flows need an ADR and an update here.

## What Ibis reads and writes

- **Reads:** files passed to the `ibis` command, the desktop library you choose,
  or your selected Zotero libraries through Zotero's local API. First-run setup
  looks for `.bib` names and modification times in Documents, Desktop, Downloads
  and a few levels of subfolders; it opens a file only after you choose it.
- **Clipboard:** copying writes the citation to your system clipboard. Clipboard
  history, cloud clipboard and other apps may keep it under your system settings.
- **Settings:** the desktop saves `settings.json`, its previous version as
  `settings.json.bak`, and `settings.json.tmp` while saving. An interrupted save
  may leave the temporary file. Settings include the library path or Zotero
  library IDs, shortcut, copy format, style, display choices and update consent.
  Older or hand-added fields are preserved. The command line has no settings
  file of its own and writes its results to the terminal.
- **Start at login (Nightly):** only when enabled, Ibis registers a macOS login
  item, Windows startup value or Linux autostart file. Turning it off removes the
  launch entry. Windows may retain a Task Manager approval value that starts
  nothing by itself.
- **Installation and updates:** the app and downloaded installer stay where
  you put them. Installers also create OS installation records and temporary
  files. Only when you choose an update does Ibis replace the app and stage
  installer/extraction/backup files. AppImage extraction also writes unpacked
  app files. Runtime data and caches are described below.

Ibis does not save search queries or citation history as an app feature. The
interface uses a WebView, which can write browser data and caches. Crash dumps
can contain memory, including library text, queries and paths. These are not
the same as Ibis's settings.

## Network and diagnostics

- **Telemetry:** none, ever ([ADR 0008](docs/adr/0008-network-policy.md)).
  Ibis never sends your libraries, search queries, selected references,
  documents, clipboard contents or citation history off your computer.
- **Web interface:** no file system, clipboard, shell or network plugin
  permissions. Library reads and clipboard writes go through Ibis's
  window-scoped commands ([ADR 0024](docs/adr/0024-window-command-permissions.md)).
  Its content security policy restricts `connect-src` to local IPC.
- **Zotero:** Ibis connects to `127.0.0.1:23119` on your computer. Setup checks
  Zotero's greeting; choosing Zotero reads the selected libraries and refreshes
  them when the picker opens. Zotero's local API must be enabled. These requests
  do not leave your computer.
- **App updates:** automatic checks are off until you agree. When enabled,
  Ibis requests a small GitHub update file a few times a day. **Check for
  Updates…** makes a check on request. GitHub sees your IP address and Ibis
  version, not library contents. Downloads require your agreement and a
  signature matching the app's update key ([ADR 0019](docs/adr/0019-update-checks.md)).
- **Windows runtime:** if WebView2 is missing, installation downloads Microsoft's
  bootstrapper and runtime. Its shared Evergreen runtime is maintained by
  Microsoft's update service, separately from Ibis's update consent.
  See [Tauri's installation options](https://v2.tauri.app/distribute/windows-installer/#webview2-installation-options).
- **While running:** Ibis configures WebView2 to disable background networking
  and use an unreachable proxy. CI samples the app and its child processes for
  connections for 60 seconds with app updates off. This is evidence for that
  test, not a guarantee about every runtime version, crash or system service.
- **Feedback:** Report a Problem and Suggest an Idea open GitHub in your browser
  with the app version and OS filled in. Nothing is posted until you submit it.
- **Diagnostics:** Ibis has no analytics or crash-upload service. Its own
  diagnostics go to standard error, not an app log file. A terminal, launcher
  or OS may retain them. WebView and OS diagnostics, crash reporting, backups
  and swap follow their own settings; Ibis does not control their retention or
  upload policies.

## Local storage locations

In the paths below, replace `<id>` with the identifier for the channel you used:

| Channel | Shipped identifier | Planned replacement in #695 (not active yet) |
| --- | --- | --- |
| Ibis (Stable) | `io.github.ibiscite.ibis` | `com.ibiscite.ibis` |
| Ibis Nightly | `io.github.ibiscite.ibis.nightly` | `com.ibiscite.ibis.nightly` |

The replacement uses the same folder rules with the new identifier. Migration
code copies settings and keeps the old files; it is not active with today's
identifiers. If you have used both identities, check both. Stable and Nightly
have separate settings and app-specific browser-data folders; system caches may be
shared.

| OS | Settings folder | WebView data and cache locations |
| --- | --- | --- |
| Windows | `%APPDATA%\<id>` | `%LOCALAPPDATA%\<id>`, including `EBWebView` profiles, caches and `EBWebView\Crashpad\reports` crash dumps. |
| macOS | `~/Library/Application Support/<id>` | The default persistent WebKit store: `~/Library/WebKit/<id>` and `~/Library/Caches/<id>`; temporary WebKit data under `$TMPDIR/<id>/WebKit`. |
| Linux | `$XDG_CONFIG_HOME/<id>`, default `~/.config/<id>` | `$XDG_DATA_HOME/<id>`, default `~/.local/share/<id>`. WebKitGTK stores its cookies, website data and cache here together, rather than using a separate Ibis folder in `~/.cache`. |

These paths come from Ibis and its Tauri/WebView defaults. Absolute XDG
overrides change Linux roots; Windows folder redirection and WebView2 policy
or environment overrides can change Windows locations. WebKit's exact files
vary with macOS. Shared system/runtime caches and logs are OS-managed.
Runtime file inventories and complete uninstall cleanup remain unverified;
these steps are based on code and platform guidance.

**Other files and OS records:**

- macOS can keep app preferences in `~/Library/Preferences/<id>.plist` and
  app state in `~/Library/Saved Application State/<id>.savedState`.
  Crash/diagnostic reports appear in Console, usually under
  `~/Library/Logs/DiagnosticReports` or `/Library/Logs/DiagnosticReports`.
- Windows can keep Ibis crash dumps in `%LOCALAPPDATA%\CrashDumps` if local
  dumps are enabled, or a configured dump folder. Windows Error Reporting
  can retain reports under `%ProgramData%\Microsoft\Windows\WER`;
  system event logs have their own retention.
- Linux launchers may send diagnostics to the journal. Depending on the
  distribution and configuration, crashes may create a core file, a report in
  `/var/crash` (Apport), or a dump in `/var/lib/systemd/coredump` plus journal
  records. These locations are shared with other apps.
- Installers, AppImage extraction and update staging use temporary storage.
  Updates use the OS temporary folder (`%TEMP%`, `$TMPDIR` or `/tmp`).
  Windows uses folders named like `Ibis-<version>-updater-*` or
  `Ibis Nightly-<version>-updater-*`. macOS/Linux can use
  `tauri_current_app*` and macOS `tauri_updated_app*` next to the app when
  needed; Linux can also fall back to the user cache folder. Interrupted updates
  may leave files behind, including a previous app needed for recovery.

## Remove Ibis and its local data

First turn off start at login, if enabled, for each channel you are removing.
Quit through **More > Quit** or the app menu; closing the picker only hides it.
Keep any settings you want before deleting folders.

- **Windows:** uninstall Ibis or Ibis Nightly in **Settings > Apps > Installed
  apps**. Delete that channel's settings and WebView folders from the table.
  Startup registration uses the value `Ibis` or `Ibis Nightly` under
  `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`.
  An approval value can remain under the matching
  `Explorer\StartupApproved\Run` key; remove only that app's value if you
  need to clear it. Do not remove the shared WebView2 Runtime; other apps use it.
- **macOS:** turn off **Open at Login** in Ibis or remove it from **System
  Settings > General > Login Items**. Trash the app, then its settings and
  WebKit/cache folders from the table. Check the app-specific preference and
  saved-state files above if present.
- **Linux:** uninstall the package with your package manager, or delete the
  AppImage and any extracted copy. Remove `<id>.desktop` from
  `$XDG_CONFIG_HOME/autostart` (default `~/.config/autostart`), then that
  channel's settings and data folders from the table.

After a successful update or uninstall, remove only confirmed Ibis temporary
files and crash reports if you want those gone too. On macOS, Console can
reveal or trash an individual report. On managed systems, ask the administrator about OS logs and dumps.
Do not delete shared cache, log or crash-report directories wholesale. Check
all four identifiers only if you used those builds. These steps do not erase
system backups, clipboard history or OS-managed records, and leave your
library files and Zotero data in place.

## Planned flows (not shipped; each opt-in)

| Flow | What would be sent | To whom | When |
| --- | --- | --- | --- |
| Style updates | A request for a signed style bundle | Ibis release host | Only if enabled |
| Metadata lookup / reference verification | The identifier you choose, such as a DOI | Crossref / OpenAlex / doi.org | Only on request, after an explanation |

Citation history and manual diagnostic export are not shipped features.
