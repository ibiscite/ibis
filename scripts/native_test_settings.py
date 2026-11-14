"""Settings directories owned by native tests in a disposable account."""

import shutil


class SettingsDirectory:
    def __init__(self, settings):
        self.settings = settings
        directory = settings.parent
        if directory.is_symlink() or (directory.exists()
                and (not directory.is_dir() or any(directory.iterdir()))):
            raise FileExistsError("settings directory is not empty")
        directory.mkdir(parents=True, exist_ok=True)

    def close(self):
        """Remove the claimed directory after all owned processes have stopped."""
        shutil.rmtree(self.settings.parent)
