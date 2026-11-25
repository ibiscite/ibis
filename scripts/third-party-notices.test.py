"""third-party-notices.py's choice of licence files, on invented packages."""

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

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
                   "licenses/BSD-3-Clause.txt", "licenses/vendor/BSD.txt", "LICENSE.APACHE2", "licenses/third-party.txt", "licenses/Apache-2.0")
        found = {path.relative_to(self.dir).as_posix() for path in notices.licence_files(self.dir)}
        self.assertEqual(found, {"LICENSE-MIT", "LICENSE-APACHE", "COPYING", "NOTICE", "THIRDPARTYNOTICES",
                                 "ThirdPartyNotices.txt", "third_party_licenses.md", "vendor/zstd/LICENSE",
                                 "licenses/BSD-3-Clause.txt", "licenses/vendor/BSD.txt", "LICENSE.APACHE2", "licenses/third-party.txt", "licenses/Apache-2.0"})

    def test_source_and_unshipped_folders_are_not_licence_texts(self):
        self.write("README.md", "src/license.rs", "src/third_party.rs", "tests/LICENSE", "examples/NOTICE",
                   "node_modules/dep/LICENSE", "benches/COPYING", "package.json", "dist/third_party.css",
                   "dist/third-party.js", "licenses/logo.png", "licenses/vendor/widget.tsx", "LICENSE.jsx",
                   "licenses/photo.jpeg", "src/license-check.sh")
        self.assertEqual(notices.licence_files(self.dir), [])


class OwnLicence(unittest.TestCase):
    def test_only_a_licence_at_the_top_is_the_packages_own(self):
        for name in ["LICENSE", "LICENCE", "license.md", "LICENSE.txt", "LICENSE-MIT", "LICENSE-APACHE",
                     "LICENSE-APACHE-2.0", "COPYING", "UNLICENSE", "License.markdown"]:
            self.assertTrue(notices.own_licence(name), name)
        for name in ["LICENSE-THIRD-PARTY.txt", "license-check", "license.sh", "licenses/LICENSE", "NOTICE",
                     "ThirdPartyNotices.txt", "LICENSE.js", "license-checker.md"]:
            self.assertFalse(notices.own_licence(name), name)


class NpmReview(unittest.TestCase):
    def test_a_package_with_only_code_named_third_party_needs_review(self):
        directory = Path(tempfile.mkdtemp(prefix="ibis-notices-"))
        self.addCleanup(shutil.rmtree, directory, True)
        (directory / "dist").mkdir()
        (directory / "dist/third_party.css").write_text("body { color: red; }\n")
        # A licence only for code it bundles is passed on, but is not the package's own.
        (directory / "licenses/vendor").mkdir(parents=True)
        (directory / "licenses/vendor/LICENSE").write_text("invented vendor licence\n")
        listing = json.dumps({"MIT": [{"name": "invented", "versions": ["1.0.0"], "paths": [str(directory)]}]})
        with patch.object(notices.subprocess, "run") as run, self.assertRaises(SystemExit):
            run.return_value.stdout = listing
            notices.npm_packages()


if __name__ == "__main__":
    unittest.main()
