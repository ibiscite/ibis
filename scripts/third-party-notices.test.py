"""third-party-notices.py's choice of licence files, on invented packages."""

import importlib.util
import shutil
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("notices", Path(__file__).with_name("third-party-notices.py"))
notices = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notices)


class LicenceFiles(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp(prefix="ibis-notices-"))
        self.addCleanup(shutil.rmtree, self.dir, True)

    def write(self, *paths):
        for path in paths:
            (self.dir / path).parent.mkdir(parents=True, exist_ok=True)
            (self.dir / path).write_text(f"invented {path}\n")

    def test_licences_notices_and_third_party_attribution_are_passed_on(self):
        self.write("LICENSE-MIT", "LICENSE-APACHE", "COPYING", "NOTICE", "THIRDPARTYNOTICES",
                   "ThirdPartyNotices.txt", "third_party_licenses.md", "vendor/zstd/LICENSE",
                   "licenses/BSD-3-Clause.txt")
        found = {path.relative_to(self.dir).as_posix() for path in notices.licence_files(self.dir)}
        self.assertEqual(found, {"LICENSE-MIT", "LICENSE-APACHE", "COPYING", "NOTICE", "THIRDPARTYNOTICES",
                                 "ThirdPartyNotices.txt", "third_party_licenses.md", "vendor/zstd/LICENSE",
                                 "licenses/BSD-3-Clause.txt"})

    def test_source_and_unshipped_folders_are_not_licence_texts(self):
        self.write("README.md", "src/license.rs", "src/third_party.rs", "tests/LICENSE", "examples/NOTICE",
                   "node_modules/dep/LICENSE", "benches/COPYING", "package.json")
        self.assertEqual(notices.licence_files(self.dir), [])


if __name__ == "__main__":
    unittest.main()
