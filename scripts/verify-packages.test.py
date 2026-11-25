"""verify-packages.py against invented packages, good and planted bad ones."""

import gzip
import importlib.util
import io
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

spec = importlib.util.spec_from_file_location("verify", Path(__file__).with_name("verify-packages.py"))
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)

NOTICES = {
    "LICENSE": b"GNU AFFERO GENERAL PUBLIC LICENSE\nsynthetic\n",
    "NOTICE.md": b"# Notices\nsynthetic\n",
    "THIRD-PARTY-NOTICES.txt": b"Third-party notices for Ibis\nRust crates (1)\n  example 1.0.0: MIT\n",
}


def tar_bytes(files, compress=True):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz" if compress else "w") as archive:
        for name, data in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


def app_files(prefix, notices=NOTICES):
    """An app's resource folder at `prefix`: its notices beside the resource that names it."""
    return {f"{prefix}/notices/{name}": data for name, data in notices.items()} | {
        f"{prefix}/resources/samples.csljson": b"[]", f"{prefix}/ibis-desktop": b"synthetic app"}


def deb_bytes(files):
    def member(name, data):
        header = f"{name:<16}{0:<12}{0:<6}{0:<6}{100644:<8}{len(data):<10}`\n".encode()
        return header + data + (b"\n" if len(data) % 2 else b"")
    return b"!<arch>\n" + member("debian-binary", b"2.0\n") + member("control.tar.gz", tar_bytes({"./control": b"Package: ibis\n"})) + member("data.tar.gz", tar_bytes(files))


def rpm_bytes(files):
    def header(tags):
        store, index = b"", b""
        for tag, value in tags.items():
            index += struct.pack(">IIII", tag, 6, len(store), 1)
            store += value.encode() + b"\0"
        return b"\x8e\xad\xe8\x01\0\0\0\0" + struct.pack(">II", len(tags), len(store)) + index + store

    cpio = b""
    for name, data in list(files.items()) + [("TRAILER!!!", b"")]:
        encoded = name.encode() + b"\0"
        fields = [0, 0o100644, 0, 0, 1, 0, len(data), 0, 0, 0, 0, len(encoded), 0]
        entry = b"070701" + b"".join(f"{value:08x}".encode() for value in fields) + encoded
        entry += b"\0" * ((4 - len(entry) % 4) % 4) + data
        cpio += entry + b"\0" * ((4 - len(entry) % 4) % 4)
    signature = header({1000: "synthetic"})
    signature += b"\0" * ((8 - len(signature) % 8) % 8)
    return b"\xed\xab\xee\xdb" + b"\0" * 92 + signature + header({1125: "gzip"}) + gzip.compress(cpio)


class Packages(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp(prefix="ibis-verify-"))
        self.addCleanup(shutil.rmtree, self.dir, True)
        self.notices = self.dir / "notices"
        self.notices.mkdir()
        for name, data in NOTICES.items():
            (self.notices / name).write_bytes(data)
        self.digests = verify.load_notices(self.notices)

    def write(self, name, data):
        path = self.dir / name
        path.write_bytes(data)
        return path

    def check(self, path):
        verify.check_package(path, self.digests)

    def assertFails(self, path, pattern):
        with self.assertRaisesRegex(verify.Failure, pattern):
            self.check(path)

    def test_command_line_archives_carry_the_notices_at_their_top(self):
        self.check(self.write("ibis-Linux.tar.gz", tar_bytes({**NOTICES, "ibis": b"synthetic"})))
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            for name, data in {**NOTICES, "ibis.exe": b"synthetic"}.items():
                archive.writestr(name, data)
        self.check(self.write("ibis-Windows.zip", buffer.getvalue()))
        # The same files one folder down are not where people look.
        self.assertFails(self.write("ibis-macOS-Intel.tar.gz", tar_bytes({f"cli/{n}": d for n, d in NOTICES.items()})), "0 copies of LICENSE")

    def test_notices_outside_their_place_do_not_count(self):
        # The command line's belong at the archive's top, not in a folder.
        self.assertFails(self.write("ibis-Linux.tar.gz", tar_bytes({f"notices/{n}": d for n, d in NOTICES.items()})),
                         "0 copies of LICENSE at the archive's top")
        # An app's belong in its own resource folder, not beside the app or in another app.
        beside = {f"notices/{n}": d for n, d in NOTICES.items()} | {"Ibis.app/Contents/Resources/resources/samples.csljson": b"[]"}
        self.assertFails(self.write("Ibis-macOS-Intel.app.tar.gz", tar_bytes(beside)), "0 copies of LICENSE in the app's resource folder")
        other = {k: v for k, v in app_files("Old.app/Contents/Resources").items() if "samples" not in k}
        other["Ibis.app/Contents/Resources/resources/samples.csljson"] = b"[]"
        self.assertFails(self.write("Ibis-macOS-Intel.app.tar.gz", tar_bytes(other)), "0 copies of LICENSE in the app's resource folder")
        elsewhere = {k: v for k, v in app_files("./usr/share/doc/ibis").items() if "samples" not in k}
        elsewhere["./usr/lib/Ibis/resources/samples.csljson"] = b"[]"
        self.assertFails(self.write("Ibis-Linux.deb", deb_bytes(elsewhere)), "0 copies of LICENSE in the app's resource folder")
        # Two apps cannot share one package.
        two = app_files("Ibis.app/Contents/Resources") | app_files("Old.app/Contents/Resources")
        self.assertFails(self.write("Ibis-macOS-Intel.app.tar.gz", tar_bytes(two)), "2 macOS apps")
        # Resources and notices moved together out of where the app installs them.
        for name, build, root in [("Ibis-Linux.deb", deb_bytes, "./usr/share/doc/ibis"), ("Ibis-Linux.rpm", rpm_bytes, "./usr/share/doc/ibis"),
                                  ("Ibis-macOS-Intel.app.tar.gz", tar_bytes, "docs")]:
            self.assertFails(self.write(name, build(app_files(root))), "0 (macOS|Linux) apps with resources")
        # Another spelling of the same path is the same path.
        aliased = app_files("Ibis.app/Contents/Resources") | {"Ibis.app/Contents/Resources/notices/./LICENSE": b"stale"}
        self.assertFails(self.write("Ibis-macOS-Intel.app.tar.gz", tar_bytes(aliased)), "2 copies of LICENSE")

    def test_a_stale_copy_cannot_hide_behind_a_good_one(self):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            for name, data in [("LICENSE", b"stale licence"), *NOTICES.items(), ("ibis", b"synthetic")]:
                info = tarfile.TarInfo(name)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
        self.assertFails(self.write("ibis-Linux.tar.gz", buffer.getvalue()), "2 copies of LICENSE")

    def test_a_missing_changed_or_doubled_notice_fails(self):
        missing = {k: v for k, v in NOTICES.items() if k != "THIRD-PARTY-NOTICES.txt"}
        self.assertFails(self.write("ibis-Linux.tar.gz", tar_bytes(missing)), "0 copies of THIRD-PARTY-NOTICES.txt")
        changed = {**NOTICES, "THIRD-PARTY-NOTICES.txt": b"Third-party notices for Ibis\nstale\n"}
        self.assertFails(self.write("ibis-Linux.tar.gz", tar_bytes(changed)), "THIRD-PARTY-NOTICES.txt differs")

    def test_app_archives_debian_and_rpm_packages_are_opened(self):
        self.check(self.write("Ibis-macOS-Intel.app.tar.gz", tar_bytes(app_files("Ibis Nightly.app/Contents/Resources"))))
        self.check(self.write("Ibis-Linux.deb", deb_bytes(app_files("./usr/lib/Ibis"))))
        self.check(self.write("Ibis-Linux.rpm", rpm_bytes(app_files("./usr/lib/Ibis"))))
        bare = {"./usr/lib/Ibis/ibis-desktop": b"synthetic app", "./usr/lib/Ibis/resources/samples.csljson": b"[]"}
        self.assertFails(self.write("Ibis-Linux.deb", deb_bytes(bare)), "0 copies of LICENSE")
        self.assertFails(self.write("Ibis-Linux.rpm", rpm_bytes(bare)), "0 copies of LICENSE")

    def test_a_damaged_or_unknown_package_fails(self):
        good = deb_bytes(app_files("./usr/lib/Ibis"))
        self.assertFails(self.write("Ibis-Linux.deb", good[: len(good) // 2]), "could not be opened|damaged")
        self.assertFails(self.write("Ibis-Linux.rpm", b"not an rpm"), "not an rpm")
        self.assertFails(self.write("ibis-Linux.tar.gz", b"\x1f\x8b broken"), "could not be opened")
        self.assertFails(self.write("Ibis-Linux.snap", b""), "no way to open")

    def test_the_generated_notices_folder_is_required(self):
        (self.notices / "LICENSE").write_bytes(b"placeholder")
        with self.assertRaisesRegex(verify.Failure, "LICENSE in the notices folder is not the generated notice"):
            verify.load_notices(self.notices)
        (self.notices / "LICENSE").unlink()
        with self.assertRaisesRegex(verify.Failure, "has no LICENSE"):
            verify.load_notices(self.notices)

    def test_each_target_needs_its_packages_and_nothing_else(self):
        linux = ["Ibis-Linux.AppImage", "Ibis-Linux.deb", "SHA256SUMS-x86_64-unknown-linux-gnu.txt", "ibis-Linux.tar.gz"]
        self.assertEqual(verify.expected_packages("x86_64-unknown-linux-gnu", linux), ["Ibis-Linux.deb", "Ibis-Linux.AppImage", "ibis-Linux.tar.gz"])
        self.assertIn("Ibis-Linux.rpm", verify.expected_packages("x86_64-unknown-linux-gnu", linux + ["Ibis-Linux.rpm"]))
        with self.assertRaisesRegex(verify.Failure, "missing packages: Ibis-Linux.AppImage"):
            verify.expected_packages("x86_64-unknown-linux-gnu", linux[1:])
        with self.assertRaisesRegex(verify.Failure, "unexpected files: Ibis-Linux.deb.sig"):
            verify.expected_packages("x86_64-unknown-linux-gnu", linux + ["Ibis-Linux.deb.sig"])
        with self.assertRaisesRegex(verify.Failure, "missing packages: .*SHA256SUMS"):
            verify.expected_packages("x86_64-pc-windows-msvc", ["Ibis-Windows-setup.exe", "ibis-Windows.zip"])
        with self.assertRaisesRegex(verify.Failure, "no packages are known"):
            verify.expected_packages("riscv64gc-unknown-linux-gnu", [])

    def test_one_bad_package_fails_the_whole_target(self):
        dist = self.dir / "dist"
        dist.mkdir()
        (dist / "SHA256SUMS-x86_64-pc-windows-msvc.txt").write_text("")
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            archive.writestr("ibis.exe", b"synthetic")
        (dist / "ibis-Windows.zip").write_bytes(buffer.getvalue())
        (dist / "Ibis-Windows-setup.exe").write_bytes(b"MZ synthetic")
        with self.assertRaisesRegex(verify.Failure, "2 of 2 packages failed"):
            verify.verify(dist, "x86_64-pc-windows-msvc", self.notices)

    @unittest.skipUnless(sys.platform == "darwin", "disk images open only on macOS")
    def test_a_disk_image_is_mounted_and_read(self):
        # One image (each takes seconds): its notices match, then a newer notice does not.
        source = self.dir / "image"
        for path, data in app_files("Ibis.app/Contents/Resources").items():
            (source / path).parent.mkdir(parents=True, exist_ok=True)
            (source / path).write_bytes(data)
        image = self.dir / "Ibis-macOS-Apple-silicon.dmg"
        subprocess.run(["hdiutil", "create", "-quiet", "-fs", "HFS+", "-srcfolder", str(source), "-format", "UDZO", str(image)], check=True)
        self.check(image)
        (self.notices / "THIRD-PARTY-NOTICES.txt").write_bytes(NOTICES["THIRD-PARTY-NOTICES.txt"] + b"  newer 2.0.0: MIT\n")
        self.digests = verify.load_notices(self.notices)
        self.assertFails(image, "THIRD-PARTY-NOTICES.txt differs")

    @unittest.skipUnless(shutil.which("7z"), "needs 7-Zip")
    def test_a_setup_executable_is_unpacked_with_7zip(self):
        for name, files in (("good", app_files("$INSTDIR")), ("bare", {"ibis-desktop.exe": b"x", "resources/samples.csljson": b"[]"})):
            source = self.dir / name
            for path, data in files.items():
                (source / path).parent.mkdir(parents=True, exist_ok=True)
                (source / path).write_bytes(data)
            archive = self.dir / f"{name}-setup.exe"
            subprocess.run(["7z", "a", "-t7z", str(archive), "."], cwd=source, check=True, capture_output=True)
            if name == "good":
                self.check(archive)
            else:
                self.assertFails(archive, "0 copies of LICENSE")

    @unittest.skipUnless(sys.platform == "linux", "AppImages open only on Linux")
    def test_an_appimage_is_extracted(self):
        payload = tar_bytes(app_files("usr/lib/Ibis"))
        (self.dir / "payload.tar.gz").write_bytes(payload)
        script = f"#!/bin/sh\n[ \"$1\" = --appimage-extract ] || exit 9\nmkdir squashfs-root && tar -xzf '{self.dir}/payload.tar.gz' -C squashfs-root\n"
        self.check(self.write("Ibis-Linux.AppImage", script.encode()))
        self.assertFails(self.write("Ibis-Linux.AppImage", b"#!/bin/sh\nexit 1\n"), "could not be opened")


if __name__ == "__main__":
    unittest.main()
