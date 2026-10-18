# Privacy and data flows

Ibis is local-first. This page lists every way data moves. If a change adds a new flow, it must update this page and add an ADR.

## Today

- **Reads:** the files you pass to the `ibis` command, and the library file you choose in the desktop app (through the system file dialog). Read-only.
- **Writes:** the `ibis` command writes only to your terminal. The desktop app writes the citation you copy to the clipboard, and its own `settings.json` (shortcut, chosen library path, last copy format, with a `.bak` backup) in your user configuration folder. Nothing else.
- **Network:** none. The desktop app's web interface has no file system, clipboard or shell permissions, and its content security policy allows connections only to the app's own commands.
- **Telemetry:** none. There is no analytics, crash reporting or usage tracking, and there never will be by default.

## Planned flows (each opt-in, off by default)

| Flow | What is sent | To whom | When |
| --- | --- | --- | --- |
| Update check | App version and platform | GitHub Releases | Only if enabled at first run or in settings |
| Style updates | Nothing identifying; downloads a signed style bundle | Ibis release host | Only if enabled |
| Metadata lookup / reference verification | Only the identifier you choose to check (e.g. a DOI) | Crossref / OpenAlex / doi.org | Only when you ask, after a one-time explanation |

Local only, never sent anywhere: your libraries, search queries, selected references, documents, clipboard contents and citation history. Citation history is stored on your device, can be turned off, and can be cleared at any time.

Diagnostic exports are manual, use a documented allowlist, and never include library contents, queries, document or clipboard text, file paths or usernames.
