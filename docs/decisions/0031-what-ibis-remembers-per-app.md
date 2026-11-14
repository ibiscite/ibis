# 0031 What Ibis remembers per app

Formats and writings live in `settings.json`, resolved by
[0042](0042-formats-apps-and-writings.md). `apps.json` holds at most 200 app
entries: operating system, normalized app ID, delivery choice, identity binding
and whether the plugin note was shown. Least recently chosen entries leave
first. No document title, URL, text or citation is saved there.

Unlisted-app Paste approval binds the OS app ID, platform and exact OS-reported
app location with SHA-256. Only that fingerprint persists or crosses IPC; the
raw path stays transient. No canonicalization, signing or executable-content
read is needed. Missing or changed binding requires approval again; Copy only
and format remain keyed by app ID. An update at the same location retains
approval; a replacement at that exact location is indistinguishable.

Atomic saves preserve durable choices on failure. Unreadable `apps.json` blocks
automatic paste: losing Copy only cannot grant permission. A durable recovery
flag remains until an explicit delivery choice resolves that app alone. A
format save cannot clear it, and a delayed save cannot authorize an earlier copy.

The engine owns recent citations in a bounded store: at most 20 apps, 30 opaque
reference identities per app and 256 KiB on disk. Unknown destinations and
sample references are excluded. Rows resolve against current source data;
queries, pages, forms and citation text are not restored. Failed saves retain
in-memory rows; failed Forget remains retryable. Turning Recent citations off
forgets immediately and records nothing. Only a fresh successful native copy
can add an identity. Portable backups exclude this history.

Recently added uses validated source-added dates, never publication years or
invented import times. BibTeX reads `date-added`, `creationdate`, `timestamp`,
then `dateadded`, using the first valid value. Offsets are converted to UTC;
values without offsets are read as UTC, so cross-source ordering can differ
from local time. These display dates do not change citation fingerprints.
