# 0006 Sources

Ibis never writes to a library. BibTeX and BibLaTeX entries are parsed one by
one; broken entries, duplicate keys and undefined macros become warnings with
line numbers. A repeated BibTeX key makes every entry under that key unavailable,
including as an inheritance parent (2026-10-30, #1519); none is kept, because
Ibis cannot know which one the person meant. The warning names the key at
every entry, the first included, and looking the key up says it is repeated
(2026-11-02, #1617). A library's id is an opaque token: nothing reads a kind from
it, and it is keyed by a per-person secret so that a seen id confirms no guessed
path. That key is the one file the `ibis` command writes. A CSL `id` is never a
citation key, and a repeated one refuses every record under it (2026-11-02, #1535).
Zotero is read through its local API while it runs and from a
private read-only copy of `zotero.sqlite` (with `-wal` and `-shm`) while it is
closed, tested on Zotero 7 to 10, leaving out trashed items. Citation keys
come from Zotero's native field, Extra and Better BibTeX; when they disagree,
key output for that item is refused until it is fixed in Zotero. Local API requests send
no browser `Origin`, bypass proxies and check `Zotero-API-Version`.
On Windows the closed reader's process, drive, attribute and ACL checks call
Win32 directly through `windows-sys`, never PowerShell (changed 2026-10-28 in
#1261: PowerShell was slow and fails under AppLocker and Constrained Language
Mode). That module and the macOS secure-input boundary are the scoped
`unsafe` exceptions described in decision 0029; their crates deny it
elsewhere and the workspace forbids it.

2026-10-31 (maintainer): Remove the saved `.bib` cache (#1498, #1515); #1534 found no first-result gain at 10k and 2.7× slower citation readiness. Parse files fresh and retain Zotero's coherent-read guard. The cleanup of old cache files went too (2026-11-04, #1712); early testers may keep unused old files.

2026-11-04 (#1645): A project bibliography is one more `.bib` the person
chooses, read only to check the keys a citation emits; it is never searched or
written. Its path is saved in settings as a `LOCAL` value, so a backup never
carries it and a restore keeps this computer's. It is read when chosen, at launch
and when the picker opens; nothing polls or watches it. Only its file name, each
key's state and line numbers cross to the web view, never the path or the file's
text. Copy Entry writes one library entry as its file has it, `file` and `note`
fields included, from native code straight to the clipboard (listed in
[PRIVACY.md](../../PRIVACY.md)).
