# 0008 Offline by default

No telemetry, ever. Everything works offline. Update checks, style updates,
metadata lookup and reference checks are each off until the person turns them
on, say exactly what is sent, and are listed in [PRIVACY.md](../../PRIVACY.md).
A new network use needs an entry here and the maintainer's yes. Requests to
Zotero on `127.0.0.1` are local reads, not network use: Zotero's local API and,
read-only, Better BibTeX's `user.groups` and `item.citationkey` on the same
port, with no proxy and no redirect (maintainer, 2026-10-27).
Ibis may serve Better BibTeX-compatible Cite as You Write to editor plugins:
`127.0.0.1` only, on its own port, refusing any request with a browser
`Origin`, and off until switched on in Settings (maintainer, 2026-10-27).

2026-11-02, the editor endpoint as built (#42): port 23124, never Zotero's 23119.
Following the architecture's rule for local endpoints, the address carries a
random 128-bit token in its path, because `Origin` alone cannot keep a web page out:
an `<img src>` sends none. Requests with `Origin`, `Referer`, `Cookie`, any `Sec-`
header or a CORS preflight header are refused, as is any `Host` but `127.0.0.1` or
`localhost` on that port (DNS rebinding), anything but `GET`, and any body. Answers
carry no CORS header, are `text/plain` with `nosniff`, and repeat nothing a client
sent. One request waits at a time. Switching off forgets the token and refuses
connections already open, so switching on again makes a new address. If the port is
taken at launch, the endpoint switches itself off, so whoever holds the port cannot
learn the token from an editor. The token is kept in `settings.json` (and its `.bak`
until the next save), never in exported backups, logs or the picker. It exposes only
whether Ibis is ready and the citation the person chooses.

2026-10-30 (maintainer-approved): A weekly GitHub workflow reads only the rolling
`stable` release's public asset names, aggregate download counters and asset
`updated_at` timestamps, appending rows of UTC date, asset name, `download_count`
and `updated_at` to `counts.csv` on the orphan `stats` branch. The asset timestamp
identifies the upload for each counter; no version is inferred or `latest.json`
downloaded. Timestamp order cannot establish publication completeness, so it
does not gate snapshots. These support rough download/update estimates, not
counts of people or active installs; replacements reset counters and downloads
between snapshots can be lost. No telemetry, identifiers, app changes or server:
nothing leaves the app.

"References used this session" (#356) holds record identities in memory, cleared by
quitting or a reset, and leaves memory only through a file the person saves. It is a new
data flow of references already read, with no network use (2026-11-02).
