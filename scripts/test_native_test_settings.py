"""Existing native settings ownership must cover both channel identities."""
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from native_test_settings import RECEIPT, RECEIPTS, clean_settings, migration_settings, settings_exist


class MigrationSettingsTests(unittest.TestCase):
    def test_claim_refuses_both_sides_of_each_channel_migration_before_writing(self):
        for suffix in ("", ".nightly"):
            identities = ("io.github.ibiscite.ibis" + suffix, "com.ibiscite.ibis" + suffix)
            for claimed, occupied in (identities, identities[::-1]):
                for name in ("settings.json", "settings.json.bak", "settings.json.tmp"):
                    with TemporaryDirectory() as temporary:
                        root = Path(temporary)
                        existing = root / occupied / name
                        existing.parent.mkdir()
                        existing.write_bytes(b"synthetic existing settings")
                        self.assertTrue(settings_exist(root / claimed / "settings.json"))
                        self.assertEqual(existing.read_bytes(), b"synthetic existing settings")
                        self.assertFalse((root / claimed).exists())

    def test_matching_never_borrows_another_channel_or_development_identity(self):
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            stable = root / "io.github.ibiscite.ibis/settings.json"
            stable.parent.mkdir()
            stable.write_bytes(b"synthetic stable settings")
            for identifier in ("com.ibiscite.ibis.nightly", "com.ibiscite.ibis.development", "other"):
                self.assertFalse(settings_exist(root / identifier / "settings.json"))
            self.assertEqual(migration_settings(stable)[-1], root / "com.ibiscite.ibis/settings.json")

    def test_preexisting_migration_metadata_is_refused_and_owned_receipts_are_cleaned(self):
        for name in (*RECEIPTS, "com.ibiscite.ibis.nightly.migration.lock"):
            with TemporaryDirectory() as temporary:
                root = Path(temporary)
                settings = root / "com.ibiscite.ibis.nightly/settings.json"
                path = root / name if name.endswith(".lock") else settings.parent / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"preexisting synthetic metadata")
                self.assertTrue(settings_exist(settings))
                self.assertEqual(path.read_bytes(), b"preexisting synthetic metadata")
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            old = root / "io.github.ibiscite.ibis.nightly/settings.json"
            self.assertFalse(settings_exist(old))
            old.parent.mkdir()
            old.write_bytes(b"owned synthetic settings")
            current = migration_settings(old)[-1]
            current.parent.mkdir()
            (current.parent / RECEIPTS[0]).write_bytes(RECEIPT)
            lock = root / "com.ibiscite.ibis.nightly.migration.lock"
            lock.touch()
            clean_settings(old)
            clean_settings(current)
            self.assertFalse(settings_exist(current))
            self.assertFalse(lock.exists())


if __name__ == "__main__":
    unittest.main()
