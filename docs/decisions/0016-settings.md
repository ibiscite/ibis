# 0016 Settings

`settings.json` lives in the platform configuration folder for `com.ibiscite.ibis`
(`com.ibiscite.ibis.nightly` for Nightly). The engine owns persistence and change
ordering ([0040](0040-one-settings-owner.md)). Saves are atomic and flushed,
keeping the previous file as `settings.json.bak`. Reads and saves are capped at
1 MiB. Replacement preserves permissions; new Unix files are owner-only.

Before 1.0, settings have no compatibility guarantee or migrations. Invalid
primary files refuse ordinary edits even with a valid backup; explicit Restore
preserves damaged bytes before replacing them. Files from a newer Ibis are
never overwritten. From 1.0, a format change needs an upgrade path. A load
problem blocks automatic paste even when a backup can be read.

Home-relative bibliography paths use `~/…` and `/` separators. Opening shortcuts
are stored per operating system; hand-written plain text means this system's
shortcut. Saves through a symlink preserve the link. Sync-conflict copies are
reported, never merged. Cloud placeholders are reported without reading them.

Portable backups omit computer-local state, paths and paste approvals. The
update question is answered on each computer because it authorizes network use.
Restore validates the whole backup, previews every change, refuses if settings
changed after preview, and uses the normal atomic save. The desktop restarts
after restoration. `ibis settings import` writes only Ibis settings.
