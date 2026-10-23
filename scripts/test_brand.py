"""The brand drift check reports stale, missing and unexpected generated files."""
from pathlib import Path
import os
import runpy
import tempfile
import unittest
from unittest.mock import patch

BUILD = runpy.run_path(str(Path(__file__).resolve().parents[1] / "brand/build.py"))


class BrandDrift(unittest.TestCase):
    def test_text_rendering_does_not_inherit_the_native_font_backend(self):
        with patch.dict(os.environ, {"PANGOCAIRO_BACKEND": "coretext"}), patch("subprocess.run") as render:
            BUILD["png"]("synthetic.svg", "synthetic.png", 32)
            self.assertEqual(render.call_args.kwargs.get("env", {}).get("PANGOCAIRO_BACKEND"), "fc")
            self.assertEqual(os.environ["PANGOCAIRO_BACKEND"], "coretext")

    @unittest.skipIf(os.name == "nt", "symlinks require developer-mode privileges")
    def test_publishing_refuses_symlink_outputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, generated = root / "source", root / "generated"
            name = Path("brand/exports/logo.svg")
            for tree in (source, generated):
                (tree / name).parent.mkdir(parents=True)
            outside = root / "untouched.txt"
            outside.write_text("synthetic unrelated file")
            (source / name).symlink_to(outside)
            (generated / name).write_text("new artwork")
            self.assertEqual(BUILD["drift"](source, generated), [name.as_posix()])
            with self.assertRaisesRegex(SystemExit, "symlink"):
                BUILD["publish"](generated, source)
            self.assertEqual(outside.read_text(), "synthetic unrelated file")
            extra = source / "brand/exports/retired.svg"
            extra.symlink_to(root / "missing.svg")
            self.assertIn(extra.relative_to(source).as_posix(), BUILD["drift"](source, generated))
            (source / name).unlink()
            BUILD["publish"](generated, source)
            self.assertFalse(extra.is_symlink())

    def test_names_every_changed_missing_or_extra_asset_without_touching_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, generated = root / "source", root / "generated"
            files = ["brand/exports/logo.svg", "brand/exports/png/logo.png",
                     "apps/desktop/src-tauri/icons/icon.ico", "apps/desktop/src/brand.ts"]
            for tree in (source, generated):
                for name in files:
                    path = tree / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(b"synthetic artwork")
            code = source / "apps/desktop/src-tauri/icons/source/generate.py"
            code.parent.mkdir(parents=True)
            code.write_text("synthetic input, not a generated file")
            compare = BUILD["drift"]
            self.assertEqual(compare(source, generated), [])
            (source / files[0]).write_text("changed by hand")
            (source / files[1]).unlink()
            extra = source / "apps/desktop/src-tauri/icons/source/retired.svg"
            extra.write_text("retired artwork")
            self.assertEqual(compare(source, generated), sorted(files[:2] + [extra.relative_to(source).as_posix()]))
            self.assertEqual(code.read_text(), "synthetic input, not a generated file")
            self.assertEqual((generated / files[0]).read_bytes(), b"synthetic artwork")
            BUILD["publish"](generated, source)
            self.assertEqual(compare(source, generated), [])
            self.assertFalse(extra.exists())
            self.assertEqual(code.read_text(), "synthetic input, not a generated file")


if __name__ == "__main__":
    unittest.main()
