"""verify-packages.py against invented packages, good and planted bad ones."""

import argparse
import gzip
import hashlib
import importlib.util
import io
import plistlib
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
VERSION = "0.4.1-nightly.20261125000000"
BUILD = VERSION + "+abc1234"
OPTIONS = argparse.Namespace(version=VERSION, build=BUILD, identifier=None, signing_leaf=None)


def options(**changes):
    return argparse.Namespace(**{**vars(OPTIONS), **changes})


def elf(build=BUILD, machine=62):
    return b"\x7fELF" + b"\0" * 14 + struct.pack("<H", machine) + b"\0" * 44 + build.encode()


def pe(build=BUILD, machine=0x8664):
    head = b"MZ" + b"\0" * 58 + struct.pack("<I", 64)
    return head + b"PE\0\0" + struct.pack("<H", machine) + b"\0" * 18 + build.encode()


def version_resource(version):
    """A version resource's String entry: header, key, alignment pad, value."""
    return struct.pack("<HHH", 0, len(version) + 1, 1) + "ProductVersion".encode("utf-16-le") + b"\0\0" + b"\0\0" + version.encode("utf-16-le") + b"\0\0"


def tar_bytes(files):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
        for name, data in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = 0o755
            archive.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


def app_files(prefix, programs, notices=NOTICES):
    """An app's resource folder at `prefix`: its notices beside the resource that names it."""
    return {f"{prefix}/notices/{name}": data for name, data in notices.items()} | {
        f"{prefix}/resources/samples.csljson": b"[]"} | programs


def linux_app(prefix="./usr", build=BUILD):
    return app_files(f"{prefix}/lib/Ibis Nightly", {f"{prefix}/bin/ibis-nightly": elf(build), f"{prefix}/bin/ibis": elf(build)})


def deb_bytes(files, version=VERSION):
    def member(name, data):
        header = f"{name:<16}{0:<12}{0:<6}{0:<6}{100644:<8}{len(data):<10}`\n".encode()
        return header + data + (b"\n" if len(data) % 2 else b"")

    control = tar_bytes({"./control": f"Package: ibis-nightly\nVersion: {version}\nArchitecture: amd64\n".encode()})
    return b"!<arch>\n" + member("debian-binary", b"2.0\n") + member("control.tar.gz", control) + member("data.tar.gz", tar_bytes(files))


def rpm_bytes(files, version=VERSION):
    def header(tags):
        store, index = b"", b""
        for tag, value in tags.items():
            index += struct.pack(">IIII", tag, 6, len(store), 1)
            store += value.encode() + b"\0"
        return b"\x8e\xad\xe8\x01\0\0\0\0" + struct.pack(">II", len(tags), len(store)) + index + store

    cpio = b""
    for name, data in list(files.items()) + [("TRAILER!!!", b"")]:
        encoded = name.encode() + b"\0"
        fields = [0, 0o100755, 0, 0, 1, 0, len(data), 0, 0, 0, 0, len(encoded), 0]
        entry = b"070701" + b"".join(f"{value:08x}".encode() for value in fields) + encoded
        entry += b"\0" * ((4 - len(entry) % 4) % 4) + data
        cpio += entry + b"\0" * ((4 - len(entry) % 4) % 4)
    signature = header({1000: "synthetic"})
    signature += b"\0" * ((8 - len(signature) % 8) % 8)
    return b"\xed\xab\xee\xdb" + b"\0" * 92 + signature + header({1001: version, 1125: "gzip"}) + gzip.compress(cpio)


def cli_tar(build=BUILD, notices=NOTICES):
    return tar_bytes({**notices, "ibis": elf(build)})


class Packages(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp(prefix="ibis-verify-"))
        self.addCleanup(shutil.rmtree, self.dir, True)
        self.notices = self.dir / "notices"
        self.notices.mkdir()
        for name, data in NOTICES.items():
            (self.notices / name).write_bytes(data)
        self.digests = verify.load_notices(self.notices)
        # Invented programs never start; the real ones do in Preview.
        original = verify.runs_here
        verify.runs_here = lambda data: False
        self.addCleanup(setattr, verify, "runs_here", original)

    def write(self, name, data):
        path = self.dir / name
        path.write_bytes(data)
        return path

    def check(self, path, target="x86_64-unknown-linux-gnu", opts=OPTIONS):
        verify.check_package(path, self.digests, opts, target)

    def assertFails(self, path, pattern, target="x86_64-unknown-linux-gnu", opts=OPTIONS):
        with self.assertRaisesRegex(verify.Failure, pattern):
            self.check(path, target, opts)

    def test_command_line_archives_carry_notices_and_the_built_program(self):
        self.check(self.write("ibis-Linux.tar.gz", cli_tar()))
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            for name, data in {**NOTICES, "ibis.exe": pe()}.items():
                archive.writestr(name, data)
        self.check(self.write("ibis-Windows.zip", buffer.getvalue()), "x86_64-pc-windows-msvc")
        nested = tar_bytes({f"cli/{n}": d for n, d in NOTICES.items()} | {"ibis": elf()})
        self.assertFails(self.write("ibis-Linux.tar.gz", nested), "0 copies of LICENSE")
        self.assertFails(self.write("ibis-Linux.tar.gz", cli_tar(build="0.4.0")), "does not embed version")
        self.assertFails(self.write("ibis-Linux.tar.gz", tar_bytes({**NOTICES, "ibis": elf(machine=183)})), "built for arm64, not x86_64")

    def test_notices_outside_their_place_do_not_count(self):
        # The command line's belong at the archive's top, not in a folder.
        self.assertFails(self.write("ibis-Linux.tar.gz", tar_bytes({f"notices/{n}": d for n, d in NOTICES.items()} | {"ibis": elf()})),
                         "0 copies of LICENSE at the archive's top")
        # An app's belong in its own resource folder, not elsewhere in the package.
        elsewhere = {k: v for k, v in app_files("./usr/share/doc/ibis", {}).items() if "samples" not in k}
        misplaced = elsewhere | {"./usr/lib/Ibis/resources/samples.csljson": b"[]", "./usr/bin/ibis-nightly": elf(), "./usr/bin/ibis": elf()}
        self.assertFails(self.write("Ibis-Linux.deb", deb_bytes(misplaced)), "0 copies of LICENSE in the app's resource folder")
        # Two apps cannot share one package.
        two = linux_app() | app_files("./usr/lib/Old", {})
        self.assertFails(self.write("Ibis-Linux.deb", deb_bytes(two)), "2 apps")

    def test_a_missing_changed_or_doubled_notice_fails(self):
        missing = {k: v for k, v in NOTICES.items() if k != "THIRD-PARTY-NOTICES.txt"}
        self.assertFails(self.write("ibis-Linux.tar.gz", cli_tar(notices=missing)), "0 copies of THIRD-PARTY-NOTICES.txt")
        changed = {**NOTICES, "THIRD-PARTY-NOTICES.txt": b"Third-party notices for Ibis\nstale\n"}
        self.assertFails(self.write("ibis-Linux.tar.gz", cli_tar(notices=changed)), "THIRD-PARTY-NOTICES.txt differs")
        # A stale copy written first cannot hide behind a good one.
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            for name, data in [("LICENSE", b"stale licence"), *NOTICES.items(), ("ibis", elf())]:
                info = tarfile.TarInfo(name)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
        self.assertFails(self.write("ibis-Linux.tar.gz", buffer.getvalue()), "holds LICENSE twice")

    def test_links_are_never_created_and_nothing_is_written_outside(self):
        outside = self.dir / "outside.txt"
        outside.write_bytes(b"untouched")
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            link = tarfile.TarInfo("alias")
            link.type, link.linkname = tarfile.LNKTYPE, str(outside)
            archive.addfile(link)
            for name, data in [("alias", b"BAD"), *NOTICES.items(), ("ibis", elf())]:
                info = tarfile.TarInfo(name)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
        self.check(self.write("ibis-Linux.tar.gz", buffer.getvalue()))
        self.assertEqual(outside.read_bytes(), b"untouched")

    def test_a_program_of_another_system_or_a_longer_version_fails(self):
        # A Linux program as the Intel macOS command line.
        self.assertFails(self.write("ibis-macOS-Intel.tar.gz", cli_tar()), "is a ELF program, not Mach-O", "x86_64-apple-darwin")
        # A longer version is not this one, wherever it can be told apart.
        self.assertTrue(verify.embeds(b"x\x000.4.1\x00y", "0.4.1"))
        self.assertTrue(verify.embeds(b"chosen_in0.4.1+abc12342026-11-25", "0.4.1+abc1234"))
        self.assertFalse(verify.embeds(b"10.4.1 and 0.4.1.2", "0.4.1"))

    def test_debian_and_rpm_packages_state_the_version(self):
        self.check(self.write("Ibis-Linux.deb", deb_bytes(linux_app())))
        self.check(self.write("Ibis-Linux.rpm", rpm_bytes(linux_app())))
        self.assertFails(self.write("Ibis-Linux.deb", deb_bytes(linux_app(), version="0.4.0")), "says version '0.4.0'")
        self.assertFails(self.write("Ibis-Linux.rpm", rpm_bytes(linux_app(), version="0.4.0")), "says version '0.4.0'")
        self.assertFails(self.write("Ibis-Linux.deb", deb_bytes(linux_app(build="0.4.0"))), "the app does not embed version")
        no_program = {k: v for k, v in linux_app().items() if "/bin/ibis-nightly" not in k}
        self.assertFails(self.write("Ibis-Linux.rpm", rpm_bytes(no_program)), "0 app programs")

    def test_a_damaged_escaping_or_unknown_package_fails(self):
        good = deb_bytes(linux_app())
        self.assertFails(self.write("Ibis-Linux.deb", good[: len(good) // 2]), "could not be opened|damaged")
        self.assertFails(self.write("Ibis-Linux.rpm", b"not an rpm"), "not an rpm")
        self.assertFails(self.write("ibis-Linux.tar.gz", b"\x1f\x8b broken"), "could not be opened")
        self.assertFails(self.write("ibis-Linux.tar.gz", tar_bytes({"../escape": b"x"})), "leaves its folder")
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
        self.assertEqual(verify.expected_packages("x86_64-unknown-linux-gnu", linux, VERSION), ["Ibis-Linux.deb", "Ibis-Linux.AppImage", "ibis-Linux.tar.gz"])
        self.assertIn("Ibis-Linux.rpm", verify.expected_packages("x86_64-unknown-linux-gnu", linux + ["Ibis-Linux.rpm"], VERSION))
        with self.assertRaisesRegex(verify.Failure, "missing packages: Ibis-Linux.AppImage"):
            verify.expected_packages("x86_64-unknown-linux-gnu", linux[1:], VERSION)
        with self.assertRaisesRegex(verify.Failure, "unexpected files: Ibis-Linux.deb.sig"):
            verify.expected_packages("x86_64-unknown-linux-gnu", linux + ["Ibis-Linux.deb.sig"], VERSION)
        # A numbered release always has its rpm.
        with self.assertRaisesRegex(verify.Failure, "missing packages: Ibis-Linux.rpm"):
            verify.expected_packages("x86_64-unknown-linux-gnu", linux, "0.4.0")
        with self.assertRaisesRegex(verify.Failure, "missing packages: .*SHA256SUMS"):
            verify.expected_packages("x86_64-pc-windows-msvc", ["Ibis-Windows-setup.exe", "ibis-Windows.zip"], VERSION)
        with self.assertRaisesRegex(verify.Failure, "no packages are known"):
            verify.expected_packages("riscv64gc-unknown-linux-gnu", [], VERSION)

    def test_checksums_must_list_exactly_the_packages(self):
        dist = self.dir / "dist"
        dist.mkdir()
        (dist / "ibis-Linux.tar.gz").write_bytes(cli_tar())
        digest = hashlib.sha256((dist / "ibis-Linux.tar.gz").read_bytes()).hexdigest()
        sums = dist / "SHA256SUMS-x86_64-unknown-linux-gnu.txt"
        sums.write_text(f"{digest}  ibis-Linux.tar.gz\n")
        verify.check_sums(dist, "x86_64-unknown-linux-gnu", ["ibis-Linux.tar.gz"])
        sums.write_text(f"{'0' * 64}  ibis-Linux.tar.gz\n")
        with self.assertRaisesRegex(verify.Failure, "does not match its checksum"):
            verify.check_sums(dist, "x86_64-unknown-linux-gnu", ["ibis-Linux.tar.gz"])
        sums.write_text(f"{digest}  ibis-Linux.tar.gz\n{digest}  extra.bin\n")
        with self.assertRaisesRegex(verify.Failure, "lists"):
            verify.check_sums(dist, "x86_64-unknown-linux-gnu", ["ibis-Linux.tar.gz"])

    def test_executable_headers_name_their_processor(self):
        self.assertEqual(verify.architecture(elf()), "x86_64")
        self.assertEqual(verify.architecture(pe(machine=0xAA64)), "arm64")
        self.assertEqual(verify.architecture(b"\xcf\xfa\xed\xfe" + struct.pack("<I", 0x0100000C)), "arm64")
        self.assertEqual(verify.architecture(b"\x7fELF"), "unknown")
        self.assertEqual(verify.product_version(b"junk" + version_resource("0.4.1")), "0.4.1")

    @unittest.skipUnless(sys.platform == "linux", "AppImages open only on Linux")
    def test_a_whole_linux_build_and_its_planted_bad_copies(self):
        dist = self.dir / "dist"
        dist.mkdir()
        payload = self.dir / "payload.tar.gz"
        payload.write_bytes(tar_bytes(linux_app(prefix="usr")))
        appimage = f"#!/bin/sh\n[ \"$1\" = --appimage-extract ] || exit 9\nmkdir squashfs-root && tar -xzf '{payload}' -C squashfs-root\n"
        packages = {"Ibis-Linux.deb": deb_bytes(linux_app()), "Ibis-Linux.AppImage": appimage.encode(), "ibis-Linux.tar.gz": cli_tar()}
        for name, data in packages.items():
            (dist / name).write_bytes(data)
        (dist / "SHA256SUMS-x86_64-unknown-linux-gnu.txt").write_text(
            "".join(f"{hashlib.sha256(data).hexdigest()}  {name}\n" for name, data in packages.items()))
        verify.self_test(dist, "x86_64-unknown-linux-gnu", self.notices, OPTIONS)
        verify.verify(dist, "x86_64-unknown-linux-gnu", self.notices, OPTIONS)
        # A verifier that accepted anything fails its own self-test.
        accept_all = verify.check_package
        verify.check_package = lambda *args: None
        self.addCleanup(setattr, verify, "check_package", accept_all)
        with self.assertRaisesRegex(verify.Failure, "planted bad packages passed"):
            verify.self_test(dist, "x86_64-unknown-linux-gnu", self.notices, OPTIONS)

    @unittest.skipUnless(shutil.which("7z"), "needs 7-Zip")
    def test_a_setup_executable_is_unpacked_with_7zip(self):
        for name, version in (("good", VERSION), ("old", "0.4.0")):
            source = self.dir / name
            files = app_files("res", {"ibis-desktop.exe": pe() + version_resource(version), "ibis.exe": pe(), "uninstall.exe": pe()})
            for path, data in files.items():
                (source / path).parent.mkdir(parents=True, exist_ok=True)
                (source / path).write_bytes(data)
            archive = self.dir / f"{name}-setup.exe"
            # Stored, not compressed: the program's version resource is readable as the installer's.
            subprocess.run(["7z", "a", "-t7z", "-mx0", str(archive), "."], cwd=source, check=True, capture_output=True)
            if name == "good":
                self.check(archive, "x86_64-pc-windows-msvc")
            else:
                self.assertFails(archive, "says version '0.4.0'", "x86_64-pc-windows-msvc")

    @unittest.skipUnless(sys.platform == "darwin" and shutil.which("cc"), "disk images and signatures need macOS")
    def test_a_disk_image_holds_a_signed_app_of_this_version(self):
        source = self.dir / "image"
        app = source / "Ibis Nightly.app/Contents"
        (app / "MacOS").mkdir(parents=True)
        (app / "Resources/notices").mkdir(parents=True)
        (app / "Resources/resources").mkdir(parents=True)
        (app / "Resources/resources/samples.csljson").write_bytes(b"[]")
        for name, data in NOTICES.items():
            (app / "Resources/notices" / name).write_bytes(data)
        program = self.dir / "main.c"
        program.write_text(f'const char *version = "{BUILD}";\nint main(void) {{ return version[0] == 0; }}\n')
        subprocess.run(["cc", "-arch", "arm64", "-o", str(app / "MacOS/ibis-desktop"), str(program)], check=True)
        info = {"CFBundleShortVersionString": VERSION, "CFBundleIdentifier": "com.example.invented",
                "CFBundleExecutable": "ibis-desktop", "CFBundlePackageType": "APPL"}
        (app / "Info.plist").write_bytes(plistlib.dumps(info))
        subprocess.run(["codesign", "-s", "-", str(app.parent)], check=True, capture_output=True)
        image = self.dir / "Ibis-macOS-Apple-silicon.dmg"
        subprocess.run(["hdiutil", "create", "-quiet", "-fs", "HFS+", "-srcfolder", str(source), "-format", "UDZO", str(image)], check=True)
        # One image (each takes seconds), checked three ways.
        target = "aarch64-apple-darwin"
        self.check(image, target, options(identifier="com.example.invented"))
        self.assertFails(image, "names 'com.example.invented', not com.example.other", target, options(identifier="com.example.other"))
        self.assertFails(image, "signed by .*, not the pinned certificate", target, options(signing_leaf="0" * 40))


if __name__ == "__main__":
    unittest.main()
