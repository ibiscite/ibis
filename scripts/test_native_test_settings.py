"""Native tests own a whole empty settings directory for their channel."""
import os
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from native_test_settings import SettingsDirectory


class SettingsTests(unittest.TestCase):
    def test_existing_channel_data_is_preserved(self):
        for name in ("settings.json", "settings.json.lock", "apps.json", "recents.json"):
            with TemporaryDirectory() as temporary:
                settings = Path(temporary) / "com.ibiscite.ibis/settings.json"
                settings.parent.mkdir()
                existing = settings.parent / name
                existing.write_bytes(b"synthetic existing data")
                with self.assertRaises(FileExistsError):
                    SettingsDirectory(settings)
                self.assertEqual(existing.read_bytes(), b"synthetic existing data")

    def test_save_cleanup_and_reclaim(self):
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            settings = root / "nightly/settings.json"
            other = root / "stable/settings.json"
            other.parent.mkdir()
            other.write_bytes(b"other channel")
            for _ in range(2):
                owned = SettingsDirectory(settings)
                settings.write_bytes(b"synthetic settings")
                (settings.parent / "settings.json.lock").touch()
                (settings.parent / "nested").mkdir()
                with self.assertRaises(FileExistsError):
                    SettingsDirectory(settings)
                owned.close()
                self.assertFalse(settings.parent.exists())
            self.assertEqual(other.read_bytes(), b"other channel")

    def test_empty_directory_can_be_claimed(self):
        with TemporaryDirectory() as temporary:
            settings = Path(temporary) / "channel/settings.json"
            settings.parent.mkdir()
            SettingsDirectory(settings).close()
            self.assertFalse(settings.parent.exists())

    @unittest.skipIf(os.name == "nt", "creating symlinks needs extra rights on Windows")
    def test_channel_symlink_is_refused(self):
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "channel").symlink_to(root / "missing", target_is_directory=True)
            with self.assertRaises(FileExistsError):
                SettingsDirectory(root / "channel/settings.json")
            self.assertTrue((root / "channel").is_symlink())


if __name__ == "__main__":
    unittest.main()
