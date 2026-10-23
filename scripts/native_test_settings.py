"""Settings files owned by native tests in a disposable account."""


def settings_files(settings):
    return tuple(settings.with_suffix(suffix) for suffix in (".json", ".json.bak", ".json.tmp"))


def settings_exist(settings):
    return any(path.exists() or path.is_symlink() for path in settings_files(settings))


def clean_settings(settings):
    for path in settings_files(settings):
        path.unlink(missing_ok=True)
