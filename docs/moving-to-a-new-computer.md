# Moving to a new computer

Ibis exports a settings backup that you move yourself. It has no sync service.

1. On the old computer, choose Settings › General › Back Up Settings ›
   Back Up… or run `ibis settings export "Ibis settings.json"`.
2. Move the backup and your library files to the new computer.
3. Choose Settings › General › Restore Settings › Restore…, inspect the
   changes and choose Restore and Restart. Cancel changes nothing.
   From a terminal, preview with
   `ibis settings import "Ibis settings.json" --dry-run`, then omit
   `--dry-run` to restore.

A backup carries the library choice, default citation output/style, appearance,
pasting preference, recents preference and opening shortcuts. Home-relative
library paths expand under the new home directory. Missing libraries keep their
setting so they can load when available. The backup does not contain library
records or recent-citation history.

Paste approvals, formats per app, window positions, small-surprise state,
project bibliographies and writing projects, editor endpoint, update consent
and Zotero profile stay on each computer. The replaced settings become
`settings.json.bak`.

Damaged, newer-format or online-only backups are refused without
changing settings. A live settings file is not a backup. Settings changes
after the restore preview require a fresh preview.

Use `--nightly` with `ibis settings` for Nightly. Storage locations and data
removal are in [PRIVACY.md](../PRIVACY.md#local-storage-locations).
