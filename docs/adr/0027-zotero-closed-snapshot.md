# 0027. Read a closed Zotero from a private copy of its database

Status: accepted (2026-10-26); applies 0006

## Context
ADR 0006 puts a read-only snapshot of `zotero.sqlite` second on the Zotero ladder, for when Zotero is closed (#14). It does not say how the snapshot is taken, which decides whether Ibis can ever disturb Zotero. Zotero 7 to 10 keep the database in WAL mode with an exclusive lock while they run, checkpoint and truncate the WAL when they quit cleanly, and leave committed transactions in a non-empty WAL after a crash. Opening the original with SQLite, even read-only or through the backup API, takes locks: a Zotero starting at that moment fails with "database is locked", and SQLite may create `-wal` or `-shm` files beside it.

## Decision
- **Only when the local API does not answer.** A running Zotero is always read through its API. The snapshot is taken only when nothing listens on Zotero's port, and is discarded if Zotero answers once it is read.
- **Never open the original.** Ibis copies `zotero.sqlite` and its `-wal` and `-journal` files (and Better BibTeX's `better-bibtex.sqlite` with its own) into a fresh directory only the user can read (on Windows, in the profile's own temporary folder whatever `TMP` says), opens only the copy, and removes the directory however the read ends. `-shm` is not copied: SQLite rebuilds it from the copied WAL. Files are opened without following links and only if they are local: data folders on network file systems and cloud files not downloaded to the computer are refused.
- **A copy is trusted only if nothing changed.** Every file's size and modification time must be the same before and after the copy, and a second read of the files must match the copy byte for byte; otherwise it is copied again, three times at most. The copy must pass `PRAGMA quick_check`.
- **Known layouts only.** Zotero's `userdata` schema versions 123 (Zotero 7.0) to 130 (Zotero 10) are read, by table and column name, after checking that each column exists. Older and newer versions are refused with a message to update Zotero or Ibis, never guessed at, and so is a read where a chosen library is missing, so the window keeps what it showed. Triggers, views and untrusted schema functions are off on the copy, and the read stops after 60 seconds, at 4 GiB of files, or at 256 MiB of stored text (1 MiB per value).
- **Zotero's own mapping.** The CSL type, field and creator mappings come from the global schema Zotero stores in the same database, and the conversion follows Zotero's CSL export, so the items feed the same mapping as the local API's answers. Citation keys: Zotero's field, then Extra, then Better BibTeX's database, whose rows count only when item, key and library all match.
- **Unchanged files are not read again.** The desktop remembers its last closed read with the file state it came from, and when nothing changed returns that result without copying again.

## Consequences
- One new dependency: `rusqlite` with SQLite bundled (MIT; SQLite is public domain), plus `miniz_oxide`, already in the lockfile, to inflate the stored schema.
- A copy needs free space the size of the database, briefly, in the system temporary folder.
- Each Zotero release that changes the `userdata` version needs the range widened after its synthetic database passes; until then that Zotero is read only while it runs. The real-Zotero CI job stops Zotero after its API checks and requires the copy to give the same references.
- Known differences from Zotero's export are rare and listed in `ibis_core::zotero::database`: EDTF dates and seasons, the local day of an access date, and item types without a CSL type.
