"""Settings files owned by native tests in a disposable account."""


def settings_exist(settings):
    directory = settings.parent
    return (directory.is_symlink() or directory.exists()
            and (not directory.is_dir() or any(directory.iterdir())))


def clean_settings(settings):
    """Only after preflight claimed an empty channel and all owned processes stopped."""
    for suffix in (".json", ".json.bak", ".json.tmp"):
        settings.with_suffix(suffix).unlink(missing_ok=True)
