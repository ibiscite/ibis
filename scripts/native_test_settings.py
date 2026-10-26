"""Settings files owned by native tests in a disposable account."""

import stat

RECEIPTS = ("macos-login-reapproval-v1.pending", "macos-login-reapproval-v1.acknowledged")
RECEIPT = b"macos-open-at-login-reapproval-v1\n"


def migration_settings(settings):
    """Both sides of this exact channel's migration, old first and current last."""
    for suffix in ("", ".nightly"):
        identities = ("io.github.ibiscite.ibis" + suffix, "com.ibiscite.ibis" + suffix)
        if settings.parent.name in identities:
            return tuple(settings.parent.with_name(identifier) / settings.name for identifier in identities)
    return (settings,)


def settings_files(settings):
    return tuple(settings.with_suffix(suffix) for suffix in (".json", ".json.bak", ".json.tmp"))


def settings_exist(settings):
    pair = migration_settings(settings)
    if len(pair) == 2:
        canonical = pair[-1].parent
        if canonical.exists() or canonical.is_symlink():
            if canonical.is_symlink() or not canonical.is_dir() or any(canonical.iterdir()):
                return True
        parent = canonical.parent
        if parent.exists() and any(path.name.startswith(canonical.name + ".migration") for path in parent.iterdir()):
            return True
    return any(path.exists() or path.is_symlink()
               for candidate in pair for path in settings_files(candidate))


def clean_settings(settings):
    """Only after preflight claimed an empty channel and all owned processes stopped."""
    for path in settings_files(settings):
        path.unlink(missing_ok=True)
    pair = migration_settings(settings)
    if len(pair) != 2 or settings != pair[-1]:
        return
    for path in (settings.parent / name for name in RECEIPTS):
        if not path.exists() and not path.is_symlink():
            continue
        metadata = path.lstat()
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size != len(RECEIPT) or path.read_bytes() != RECEIPT:
            raise RuntimeError("Owned identity migration receipt changed; preserving it")
        path.unlink()
    lock = settings.parent.with_name(settings.parent.name + ".migration.lock")
    if lock.exists() or lock.is_symlink():
        metadata = lock.lstat()
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size != 0:
            raise RuntimeError("Owned identity migration lock changed; preserving it")
        lock.unlink()
