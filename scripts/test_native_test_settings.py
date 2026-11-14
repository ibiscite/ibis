"""Native tests claim only an empty settings directory for their channel."""
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from native_test_settings import clean_settings, settings_exist


class SettingsTests(unittest.TestCase):
    def test_existing_channel_data_is_preserved(self):
        for name in ("settings.json", "settings.json.bak", "settings.json.tmp", "apps.json", "recents.json"):
            with TemporaryDirectory() as temporary:
                settings = Path(temporary) / "com.ibiscite.ibis/settings.json"
                self.assertFalse(settings_exist(settings))
                settings.parent.mkdir()
                self.assertFalse(settings_exist(settings))
                existing = settings.parent / name
                existing.write_bytes(b"synthetic existing data")
                self.assertTrue(settings_exist(settings))
                self.assertEqual(existing.read_bytes(), b"synthetic existing data")

    def test_claim_does_not_borrow_another_channel(self):
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            stable = root / "com.ibiscite.ibis/settings.json"
            stable.parent.mkdir()
            stable.write_bytes(b"synthetic stable settings")
            for identifier in ("com.ibiscite.ibis.nightly", "com.ibiscite.ibis.development", "other"):
                self.assertFalse(settings_exist(root / identifier / "settings.json"))

    def test_cleanup_removes_only_owned_settings_files(self):
        with TemporaryDirectory() as temporary:
            settings = Path(temporary) / "settings.json"
            for name in ("settings.json", "settings.json.bak", "settings.json.tmp"):
                settings.with_name(name).write_bytes(b"owned synthetic settings")
            preserved = settings.with_name("other.json")
            preserved.write_bytes(b"other data")
            clean_settings(settings)
            self.assertEqual(list(settings.parent.iterdir()), [preserved])


if __name__ == "__main__":
    unittest.main()
