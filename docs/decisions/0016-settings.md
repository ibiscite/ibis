# 0016 Settings (with 0018, 0020, 0022, 0023 and 0027)

`settings.json` in the platform configuration folder for `com.ibiscite.ibis`
(Nightly: `com.ibiscite.ibis.nightly`). Saves are atomic and flushed, keeping
the previous file as `settings.json.bak`. Until 1.0 there are no migrations:
settings Ibis cannot read are kept as a copy, Ibis starts from defaults and
says so, and it never silently overwrites a file it could not read. Existing
migration code is deleted, not extended, when its area is rebuilt. From 1.0 a
format change needs an upgrade path.

Primary, backup and damaged-file recovery reads are capped at 1 MiB, including
growth during a read; saves must fit that same limit. Atomic replacement keeps
the existing file permissions; new Unix files start owner-only. A load problem
blocks automatic paste and reports paste unavailable, even with a valid backup
(2026-10-31, #1525).

Planned copied-key warnings (#1092; maintainer-approved, 2026-10-30) may keep a
bounded private local record of library/item identity and its last successfully
copied key. This is not document or research history: no queries, document text,
quotations or personal comments. Failed or superseded copies cannot update it;
failed storage keeps the previous durable record and reports that it was not
updated. The implementation defines the capacity and file format under the
existing atomic-write and single-owner rules. Users can forget the record.
Only native identities that survive a key rename qualify; never infer continuity
from titles/DOIs when a `.bib` entry key changes its identity. A successful new
key copy replaces the baseline, not the keys in any manuscript. Clipboard success
is the commit event; diagnostic old/new-key copies, previews, CLI stdout and MCP
formatting do not count. Batch history writes occur outside the source/clipboard
barrier and preserve successful copy order rather than disk-completion order.
Stable and Nightly stay isolated.
This permission does not give agents notes, annotations or private links (see decision 0033).

2026-11-02 (#1561): the settings file works on another computer. A bibliography
inside the home folder is written as `~/…` with `/` between names, and read back
under that computer's home; other paths stay as written. The opening shortcut is
kept per system and always written as `{"macos": …, "windows": …}`, so a file
shared live between systems never hands one system's shortcut to another; plain
text written by hand is this system's. Builds before this one cannot read the
per-system form or `~/…` and fall back as for any damaged file (0016, pre-1.0).
Saves through a symlink replace the linked file and keep the link. Copies a sync
service made (iCloud `settings 2.json`, OneDrive `settings-PC.json`, Dropbox,
Google Drive, Syncthing) are named in the settings notice, never merged. A
backup (`ibis_settings_backup: 1`, `made_on`, `made_by`, `settings`) holds the
settings without the ones that belong to one computer: window places, quiet
feedback and the update question, which each computer answers for itself because
it allows network use (0008). Paste approvals live in `apps.json` and never
travel, so a restored file cannot grant paste (0031). A restore checks the whole
backup first, shows every change, is refused if the settings changed since that
preview, and saves through the normal atomic write, so the replaced settings
become `settings.json.bak`; damaged settings are kept as `.invalid-N` and
replaced, settings from a newer Ibis never are. Placeholders a cloud folder has
not downloaded are reported without reading them. The app restarts after a
restore so every part starts from the same settings. `ibis settings import`
writes only Ibis's own settings file; the command line stays read-only for
libraries and sources (0033).
