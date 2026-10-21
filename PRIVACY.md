# Privacy and data flows

Ibis is local-first. This page lists every way data moves. If a change adds a new flow, it must update this page and add an ADR.

## Today

- **Reads:** the files you pass to the `ibis` command, the library file you choose in the desktop app (through the system file dialog), or, if you choose Zotero, your Zotero library through Zotero's local API. Read-only. While no library is chosen, first-run setup looks for `.bib` files in your Documents, Desktop and Downloads folders and a few levels of subfolders, reading only file names and modification times and opening no file, so you can pick one; the list stays in the app.
- **Writes:** the `ibis` command writes only to your terminal. The desktop app writes:
  - the citation you copy, to the clipboard;
  - its own `settings.json` in your user configuration folder, with a `.bak` backup: the shortcut, the chosen library file or Zotero libraries, the last copy format, the citation style, row density, larger text, and your answer about update checks;
  - only if you turn on starting at login (**Open at Login** on macOS, **Start when you sign in** on Windows, **Start on Login** on Linux), a login entry for the app: on macOS a login item (System Settings > General > Login Items); on Windows a value named after the app under `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`, and, if Windows already has the matching `Explorer\StartupApproved\Run` key that Task Manager's switch uses, an "enabled" value there; on Linux a `.desktop` file in `~/.config/autostart` (or `$XDG_CONFIG_HOME/autostart`). Turning it off removes the entry; on Windows the Task Manager value stays, which starts nothing on its own;
  - only when you choose to install an update, the new version of the app in place of the old one.

  Nothing else.
- **Network while Ibis is running:** none, except connections on your own computer to Zotero at `127.0.0.1:23119`. First-run setup asks once whether Zotero is open; that request reads only Zotero's fixed greeting, no library. When you choose to use Zotero, the desktop app asks it for your library each time the picker opens. This never leaves your computer; nothing is sent but the request, and Zotero answers only if you turned on "Allow other applications on this computer to communicate with Zotero". Apart from that and the update checks below, nothing. The desktop app's web interface has no file system, clipboard or shell permissions, and its content security policy allows connections only to the app's own commands. On Windows, the Microsoft WebView2 runtime that draws the interface would otherwise fetch updates and other background data from Microsoft while Ibis runs; Ibis turns those features off and points the runtime at an unreachable proxy, so it has no route to the internet. Windows CI fails if the running app opens any network connection. **Report a Problem** and **Suggest an Idea** in the menu open a GitHub form in your own browser, with only the Ibis version and operating system filled in; Ibis itself sends nothing, and nothing is posted until you submit the form.
- **Update checks (off until you say yes):** on first launch Ibis asks once whether to check for updates; **Check for Updates Automatically** in the menu changes your answer. When on, Ibis asks `github.com` for one small update file a few times a day; **Check for Updates…** asks once. GitHub sees your IP address and the Ibis version. Nothing from your library is sent. A new version is downloaded only when you choose to install it, and installed only if its signature matches the Ibis update key built into the app ([ADR 0019](docs/adr/0019-update-checks.md)).
- **Windows installation:** if Microsoft WebView2 Runtime is missing, the installer downloads Microsoft's WebView2 bootstrapper and uses it to install the runtime. This one-time prerequisite setup requires an internet connection. The Ibis app works offline after setup. See [Tauri's WebView2 installation options](https://v2.tauri.app/distribute/windows-installer/#webview2-installation-options).
- **Telemetry:** none. There is no analytics, crash reporting or usage tracking, and there never will be by default.

## Planned flows (each opt-in, off by default)

| Flow | What is sent | To whom | When |
| --- | --- | --- | --- |
| Style updates | Nothing identifying; downloads a signed style bundle | Ibis release host | Only if enabled |
| Metadata lookup / reference verification | Only the identifier you choose to check (e.g. a DOI) | Crossref / OpenAlex / doi.org | Only when you ask, after a one-time explanation |

Local only, never sent anywhere: your libraries, search queries, selected references, documents, clipboard contents and citation history. Citation history is stored on your device, can be turned off, and can be cleared at any time.

Diagnostic exports are manual, use a documented allowlist, and never include library contents, queries, document or clipboard text, file paths or usernames.
