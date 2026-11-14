# 0008 Offline by default

No telemetry. Core library and citation work is offline. Network features are
opt-in, say exactly what is sent and are listed in [PRIVACY.md](../../PRIVACY.md).
A new network use needs an entry here and the maintainer's yes. Requests to
Zotero on `127.0.0.1` are local reads, not network use: Zotero's local API and,
read-only, Better BibTeX's `user.groups` and `item.citationkey` on the same
port, with no proxy and no redirect.
Ibis may serve Better BibTeX-compatible Cite as You Write to editor plugins:
`127.0.0.1` only, on its own port, refusing any request with a browser
`Origin`, and off until switched on in Settings.

The editor endpoint uses port 23124, separate from Zotero's 23119. Its address carries a
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

A weekly GitHub workflow reads only the rolling
`stable` release's public asset names, aggregate download counters and asset
`updated_at` timestamps, appending rows of UTC date, asset name, `download_count`
and `updated_at` to `counts.csv` on the orphan `stats` branch. The asset timestamp
identifies the upload for each counter; no version is inferred or `latest.json`
downloaded. Timestamp order cannot establish publication completeness, so it
does not gate snapshots. These support rough download/update estimates, not
counts of people or active installs; replacements reset counters and downloads
between snapshots can be lost. No telemetry, identifiers, app changes or server:
nothing leaves the app.

"References cited since opening" holds record identities in memory, cleared by
quitting or a reset. It leaves memory only through an explicit copy or saved file,
with no network use.

Planned online lookup follows [0051](0051-references-in-and-out.md). One engine
module enforces these rules for every lookup:

- **One switch, off by default.** Setup asks once with nothing preselected;
  Settings changes it at any time. MCP exposes no lookup or write tools.
  Shell access can change settings or run the CLI; privacy docs must say so.
- **Ask before the first use** of each lookup, saying exactly what is sent and
  to which service.
- **Send only what the person gave:** the DOI or the words they typed. Never
  library contents, file paths, document text or record identities. A dropped
  PDF's DOI is read locally; only the DOI leaves.
- **Named services only**: `doi.org`, which hands a
  DOI to `api.crossref.org` or `data.crosscite.org` (DataCite, for arXiv and
  Zenodo); `api.openalex.org`, only to turn a PubMed ID into a DOI; and
  `openlibrary.org` for ISBNs. HTTPS with certificate checks. Redirects are
  never followed automatically: one `Location` from `doi.org` is accepted only
  if it is HTTPS on exactly one of the listed hosts. A pasted URL is never
  fetched. No cookies, no email address (Crossref's public pool), a generic
  user agent, timeouts, a 1 MiB answer limit, one request at a time and at most
  50 per action, respecting 429. Only CSL-JSON answers are used; Ibis writes any
  BibTeX itself. Answers are untrusted input, checked before anything shows them.
- **One owner.** A single engine module holds the only HTTP client for lookups
  and refuses while the switch is off; a test proves nothing is sent then.
- **Nothing changes the library** without the person's own action. Adding an
  item to Zotero goes through its local write API, decided separately.
- **Shipping:** PRIVACY.md, the README and the website change in the same pull
  request, to "offline by default; online lookup only if you turn it on". As a
  network change, it needs an independent review before it merges.
