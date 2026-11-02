# Moving to a new computer

Ibis keeps its settings in one small file on each computer. To take your setup to
another computer, make a backup on the old one and restore it on the new one. Ibis
has no account or sync service of its own; the backup is an ordinary file that you
move however you like.

1. On the old computer, open **Settings › General › Back Up Settings** and choose
   **Back Up…**. The save dialog starts in iCloud Drive on a Mac and OneDrive on
   Windows when you use them, so the file reaches your other computers by itself.
   From a terminal: `ibis settings export "Ibis settings.json"`.
2. On the new computer, install Ibis, open **Settings › General › Restore
   Settings** and choose **Restore…**, then the backup. Ibis checks the whole file
   and lists every setting it would change. Choose **Restore and Restart**, or
   **Cancel** to change nothing. From a terminal:
   `ibis settings import "Ibis settings.json" --dry-run` shows the changes, and the
   same command without `--dry-run` restores them.

**What travels:** the library you chose, your citation style and key format, how
the picker looks, Enter pastes, recent citations, small surprises and the opening
shortcut for each system. A bibliography in your home folder is saved as
`~/Documents/refs.bib`, so it is found again under the other computer's home
folder even with another user name or system. If the file is not there yet, Ibis
keeps the setting and finds it once the file arrives.

**What stays on each computer:** paste approvals for apps, which you give again on
each computer, window places and sizes, and whether Ibis checks for updates, which
each computer asks once. The settings that a restore replaces are kept as
`settings.json.bak` beside the settings file.

**A backup that cannot be used** (damaged, from a newer version of Ibis, the
settings file itself rather than a backup, or not yet downloaded from your cloud
folder) is refused with the reason, and nothing changes.

## Each system

- **macOS.** Settings live in `~/Library/Application Support/com.ibiscite.ibis/`.
  Migration Assistant and Time Machine already carry them to a new Mac, along with
  the rest of your home folder. A backup in iCloud Drive that shows a cloud icon in
  Finder has not downloaded yet: open it in Finder once, then restore.
- **Windows.** Settings live in `%APPDATA%\com.ibiscite.ibis\`. Windows does not
  restore settings for apps installed outside the Microsoft Store, so use a
  backup. A OneDrive file marked "available when online" downloads when you open
  it in File Explorer.
- **Linux.** Settings live in `~/.config/com.ibiscite.ibis/` (Flatpak:
  `~/.var/app/<app id>/config/`). Copy a backup, or keep your dotfiles in sync as
  you already do. Ibis keeps a settings file that is a link into a synced folder as
  a link when it saves.

If a sync service makes a second copy of the settings file, such as `settings
2.json` (iCloud Drive) or `settings-PC.json` (OneDrive), Ibis says so, keeps using
`settings.json` and never merges them. Check the copy and delete it.

Ibis Nightly keeps its own settings (`com.ibiscite.ibis.nightly`); use `--nightly`
with `ibis settings` to back it up or restore it.
