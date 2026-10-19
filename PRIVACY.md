# Privacy and data flows

Ibis is local-first. This page lists every way data moves. If a change adds a new flow, it must update this page and add an ADR.

## Today

- **Reads:** the files you pass to the `ibis` command, the library file you choose in the desktop app (through the system file dialog), or, if you choose Zotero, your Zotero library through Zotero's local API. Read-only.
- **Writes:** the `ibis` command writes only to your terminal. The desktop app writes the citation you copy to the clipboard, and its own `settings.json` (shortcut, chosen library file or Zotero, last copy format, with a `.bak` backup) in your user configuration folder. Nothing else.
- **Network while Ibis is running:** none, except one connection on your own computer when you choose to use Zotero: the desktop app then asks Zotero, at `127.0.0.1:23119`, for your library each time the picker opens. This never leaves your computer; nothing is sent but the request, and Zotero answers only if you turned on "Allow other applications on this computer to communicate with Zotero". Apart from that, nothing. The desktop app's web interface has no file system, clipboard or shell permissions, and its content security policy allows connections only to the app's own commands. On Windows, the Microsoft WebView2 runtime that draws the interface would otherwise fetch updates and other background data from Microsoft while Ibis runs; Ibis turns those features off and points the runtime at an unreachable proxy, so it has no route to the internet. Windows CI fails if the running app opens any network connection.
- **Windows installation:** if Microsoft WebView2 Runtime is missing, the installer downloads Microsoft's WebView2 bootstrapper and uses it to install the runtime. This one-time prerequisite setup requires an internet connection. The Ibis app works offline after setup. See [Tauri's WebView2 installation options](https://v2.tauri.app/distribute/windows-installer/#webview2-installation-options).
- **Telemetry:** none. There is no analytics, crash reporting or usage tracking, and there never will be by default.

## Planned flows (each opt-in, off by default)

| Flow | What is sent | To whom | When |
| --- | --- | --- | --- |
| Update check | App version and platform | GitHub Releases | Only if enabled at first run or in settings |
| Style updates | Nothing identifying; downloads a signed style bundle | Ibis release host | Only if enabled |
| Metadata lookup / reference verification | Only the identifier you choose to check (e.g. a DOI) | Crossref / OpenAlex / doi.org | Only when you ask, after a one-time explanation |

Local only, never sent anywhere: your libraries, search queries, selected references, documents, clipboard contents and citation history. Citation history is stored on your device, can be turned off, and can be cleared at any time.

Diagnostic exports are manual, use a documented allowlist, and never include library contents, queries, document or clipboard text, file paths or usernames.
