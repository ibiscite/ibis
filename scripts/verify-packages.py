#!/usr/bin/env python3
"""Open every package Preview built for one target and check what it holds.

    python3 scripts/verify-packages.py DIST TARGET NOTICES --version V
        [--build V+SHA] [--identifier ID] [--signing-leaf SHA1] [--self-test]

DIST is the folder of collected packages (scripts/collect-packages.sh),
TARGET the app's Rust target, NOTICES the folder scripts/third-party-notices.py
wrote, V the version this run must have built (the tag's, or the nightly's
from scripts/update-feed.mjs) and --build the version the programs report (a
nightly's V+commit). Each package is unpacked the way its format needs (tar,
zip, ar, rpm, disk image, AppImage, NSIS through 7-Zip) and must:

- be listed with its digest in SHA256SUMS-TARGET.txt, and nothing else listed;
- carry the three notice files byte for byte (the command line at its top,
  the app in its `notices` resource folder);
- declare version V where its format records one (Info.plist, Debian control,
  rpm header, the setup executable's ProductVersion), and hold one app or
  command-line executable built for TARGET's processor that embeds --build;
- on macOS, have a valid code signature, made by --signing-leaf when given,
  and the bundle identifier --identifier;
- run its command line, where this computer can, and report --build.

A missing or unexpected package, a package that cannot be opened, a missing
tool or any difference fails. --self-test first plants bad copies of the real
packages (cut in half, a wrong version, the command line without its notices)
and fails unless each is refused, so the check cannot pass by opening nothing.
"""

import argparse
import gzip
import hashlib
import io
import lzma
import os
import platform
import plistlib
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path

NOTICE_FILES = ("LICENSE", "NOTICE.md", "THIRD-PARTY-NOTICES.txt")
# A notices folder that is the generator's output, not a stand-in.
MARKERS = {
    "LICENSE": b"GNU AFFERO GENERAL PUBLIC LICENSE",
    "NOTICE.md": b"# Notices",
    "THIRD-PARTY-NOTICES.txt": b"Third-party notices for Ibis",
}
# target: (download name, processor its executables are built for)
PLATFORMS = {
    "aarch64-apple-darwin": ("macOS-Apple-silicon", "arm64"),
    "x86_64-apple-darwin": ("macOS-Intel", "x86_64"),
    "x86_64-pc-windows-msvc": ("Windows", "x86_64"),
    "x86_64-unknown-linux-gnu": ("Linux", "x86_64"),
}
CLI_NAMES = ("ibis", "ibis.exe")
OPEN_ERRORS = (OSError, ValueError, EOFError, tarfile.TarError, zipfile.BadZipFile, lzma.LZMAError,
               subprocess.SubprocessError, struct.error, IndexError, UnicodeDecodeError, plistlib.InvalidFileException)


class Failure(Exception):
    pass


def expected_packages(target, names):
    """Package names for TARGET in dist order; Nightly has no .rpm."""
    if target not in PLATFORMS:
        raise Failure(f"no packages are known for {target}")
    name = PLATFORMS[target][0]
    if name.startswith("macOS"):
        required = [f"Ibis-{name}.dmg", f"Ibis-{name}.app.tar.gz", f"ibis-{name}.tar.gz"]
    elif name == "Windows":
        required = ["Ibis-Windows-setup.exe", "ibis-Windows.zip"]
    else:
        required = ["Ibis-Linux.deb", "Ibis-Linux.AppImage", "ibis-Linux.tar.gz"]
    optional = ["Ibis-Linux.rpm"] if name == "Linux" else []
    sums = f"SHA256SUMS-{target}.txt"
    missing = [n for n in required + [sums] if n not in names]
    if missing:
        raise Failure("missing packages: " + ", ".join(missing))
    unexpected = sorted(set(names) - set(required) - set(optional) - {sums})
    if unexpected:
        raise Failure("unexpected files: " + ", ".join(unexpected))
    return [n for n in required + optional if n in names]


# --- Unpacking -------------------------------------------------------------


def decompress(data, kind):
    if kind in ("gzip", ".gz"):
        return gzip.decompress(data)
    if kind in ("xz", ".xz", "lzma"):
        return lzma.decompress(data)
    if kind in ("zstd", ".zst"):
        if shutil.which("zstd") is None:
            raise Failure("zstd is needed to read this package")
        return subprocess.run(["zstd", "-dc"], input=data, capture_output=True, check=True).stdout
    if kind in ("", ".tar"):
        return data
    raise Failure(f"unknown compression {kind!r}")


def inside(name):
    name = name.replace("\\", "/")
    return not name.startswith("/") and ".." not in name.split("/")


def untar(data, into):
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        members = archive.getmembers()
        outside = [m.name for m in members if not inside(m.name)]
        if outside:
            raise Failure(f"an archive entry leaves its folder: {outside[0]}")
        if sys.version_info >= (3, 12):
            archive.extractall(into, filter="tar")
        else:
            archive.extractall(into)


def ar_members(data):
    if not data.startswith(b"!<arch>\n"):
        raise Failure("not an ar archive")
    offset, members = 8, {}
    while offset < len(data):
        header = data[offset:offset + 60]
        if len(header) < 60 or header[58:60] != b"`\n":
            raise Failure("damaged ar archive")
        name = header[:16].decode().strip().rstrip("/")
        size = int(header[48:58].decode().strip())
        if offset + 60 + size > len(data):
            raise Failure("damaged ar archive")
        members[name] = data[offset + 60:offset + 60 + size]
        offset += 60 + size + (size % 2)
    return members


def rpm_header(data, offset):
    if data[offset:offset + 4] != b"\x8e\xad\xe8\x01":
        raise Failure("damaged rpm header")
    count, size = struct.unpack(">II", data[offset + 8:offset + 16])
    index = offset + 16
    store = index + 16 * count
    if store + size > len(data):
        raise Failure("damaged rpm header")
    tags = {}
    for i in range(count):
        tag, kind, start, _ = struct.unpack(">IIII", data[index + 16 * i:index + 16 * (i + 1)])
        if kind in (6, 9):  # a string, or the first of an i18n string array
            end = data.index(b"\0", store + start)
            tags[tag] = data[store + start:end].decode()
    return tags, store + size


def unpack_rpm(path, into):
    data = path.read_bytes()
    if data[:4] != b"\xed\xab\xee\xdb":
        raise Failure("not an rpm package")
    _, end = rpm_header(data, 96)
    end += (8 - end % 8) % 8
    tags, payload = rpm_header(data, end)
    cpio = decompress(data[payload:], tags.get(1125, "gzip"))  # PAYLOADCOMPRESSOR
    offset = 0
    while True:
        if cpio[offset:offset + 6] not in (b"070701", b"070702"):
            raise Failure("damaged rpm payload")
        mode = int(cpio[offset + 14:offset + 22], 16)
        size = int(cpio[offset + 54:offset + 62], 16)
        name_size = int(cpio[offset + 94:offset + 102], 16)
        name = cpio[offset + 110:offset + 110 + name_size - 1].decode()
        start = offset + 110 + name_size
        start += (4 - start % 4) % 4
        if name == "TRAILER!!!":
            break
        name = name[2:] if name.startswith("./") else name.lstrip("/")
        if not inside(name):
            raise Failure(f"an rpm entry leaves its folder: {name}")
        if mode & 0o170000 == 0o100000:
            target = Path(into, name)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(cpio[start:start + size])
        offset = start + size
        offset += (4 - offset % 4) % 4
    return {"Version": tags.get(1001, "")}  # RPMTAG_VERSION


def unpack_deb(path, into):
    members = ar_members(path.read_bytes())
    if members.get("debian-binary", b"").strip() != b"2.0":
        raise Failure("not a Debian package")
    control = [m for m in members if m.startswith("control.tar")]
    data = [m for m in members if m.startswith("data.tar")]
    if len(control) != 1 or len(data) != 1:
        raise Failure("no single control and data archive in the Debian package")
    untar(decompress(members[data[0]], data[0][len("data.tar"):]), into)
    with tarfile.open(fileobj=io.BytesIO(decompress(members[control[0]], control[0][len("control.tar"):]))) as archive:
        entry = next((m for m in archive if m.name.lstrip("./") == "control"), None)
        if entry is None:
            raise Failure("the Debian package has no control file")
        text = archive.extractfile(entry).read().decode()
    fields = {}
    for line in text.splitlines():
        if ":" in line and not line.startswith((" ", "\t")):
            key, value = line.split(":", 1)
            fields[key.strip()] = value.strip()
    return fields


def unpack(path, into, mounts):
    """Unpack PATH into the empty folder INTO; returns (root folder, metadata)."""
    name = path.name
    if name.endswith(".tar.gz"):
        untar(path.read_bytes(), into)
        return into, {}
    if name.endswith(".zip"):
        with zipfile.ZipFile(path) as archive:
            outside = [m for m in archive.namelist() if not inside(m)]
            if outside:
                raise Failure(f"an archive entry leaves its folder: {outside[0]}")
            archive.extractall(into)
        return into, {}
    if name.endswith(".deb"):
        return into, unpack_deb(path, into)
    if name.endswith(".rpm"):
        return into, unpack_rpm(path, into)
    if name.endswith(".dmg"):
        if shutil.which("hdiutil") is None:
            raise Failure("hdiutil is needed to open a disk image (macOS only)")
        subprocess.run(["hdiutil", "attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint", str(into), str(path)],
                       check=True, capture_output=True)
        mounts.append(into)
        return into, {}
    if name.endswith(".AppImage"):
        if sys.platform != "linux":
            raise Failure("an AppImage can only be opened on Linux")
        copy = Path(into, "package.AppImage")
        shutil.copyfile(path, copy)
        copy.chmod(0o755)
        subprocess.run([str(copy), "--appimage-extract"], cwd=into, check=True, capture_output=True, timeout=300)
        return Path(into, "squashfs-root"), {}
    if name.endswith("-setup.exe"):
        tool = shutil.which("7z") or shutil.which("7z.exe")
        if tool is None:
            raise Failure("7-Zip (7z) is needed to open the setup executable")
        subprocess.run([tool, "x", "-y", f"-o{into}", str(path)], check=True, capture_output=True)
        return into, {"ProductVersion": product_version(path.read_bytes())}
    raise Failure(f"no way to open {name}")


# --- What a package holds ----------------------------------------------------


def files(root):
    for dirpath, _, names in os.walk(root):
        for name in names:
            full = Path(dirpath, name)
            if full.is_file() and not full.is_symlink():
                yield full.relative_to(root).as_posix(), full


def product_version(exe):
    """The ProductVersion string in a Windows executable's version resource."""
    key = "ProductVersion".encode("utf-16-le") + b"\0\0"
    at = exe.find(key)
    if at < 0:
        return ""
    # The value follows the key after up to one 32-bit alignment pad.
    start = at + len(key)
    for _ in range(2):
        if exe[start:start + 2] == b"\0\0":
            start += 2
    end = start
    while end + 1 < len(exe) and exe[end:end + 2] != b"\0\0":
        end += 2
    return exe[start:end].decode("utf-16-le", errors="replace")


def architecture(data):
    """The processor an executable is built for, from its header."""
    try:
        return header_architecture(data)
    except struct.error:
        return "unknown"


def header_architecture(data):
    if data[:4] == b"\x7fELF":
        return {62: "x86_64", 183: "arm64"}.get(struct.unpack("<H", data[18:20])[0], "other")
    if data[:2] == b"MZ" and len(data) > 64:
        pe = struct.unpack("<I", data[60:64])[0]
        if data[pe:pe + 4] == b"PE\0\0":
            return {0x8664: "x86_64", 0xAA64: "arm64"}.get(struct.unpack("<H", data[pe + 4:pe + 6])[0], "other")
    if data[:4] == b"\xcf\xfa\xed\xfe":
        return {0x01000007: "x86_64", 0x0100000C: "arm64"}.get(struct.unpack("<I", data[4:8])[0], "other")
    return "unknown"


def runs_here(data):
    """Whether this computer can start an executable with this header."""
    machine = platform.machine().lower()
    host = {"amd64": "x86_64", "aarch64": "arm64"}.get(machine, machine)
    system = {b"\x7fELF": "linux", b"\xcf\xfa\xed\xfe": "darwin"}.get(data[:4], "win32" if data[:2] == b"MZ" else "")
    return architecture(data) == host and sys.platform == system


class Check:
    """One package's problems, collected so a report names them all."""

    def __init__(self, notices, options, arch):
        self.notices, self.options, self.arch = notices, options, arch
        self.problems = []

    def require(self, condition, problem):
        if not condition:
            self.problems.append(problem)

    def notice_files(self, tree, top_level):
        found = {}
        for rel, full in tree:
            for name in NOTICE_FILES:
                if (top_level and rel == name) or rel == f"notices/{name}" or rel.endswith(f"/notices/{name}"):
                    found.setdefault(name, []).append(full)
        for name, digest in self.notices.items():
            copies = found.get(name, [])
            if len(copies) != 1:
                self.problems.append(f"{len(copies)} copies of {name}")
            elif hashlib.sha256(copies[0].read_bytes()).hexdigest() != digest:
                self.problems.append(f"{name} differs from the generated notice")

    def executable(self, candidates, what):
        if len(candidates) != 1:
            self.problems.append(f"{len(candidates)} {what} programs, not one")
            return None
        data = candidates[0].read_bytes()
        found = architecture(data)
        self.require(found == self.arch, f"the {what} is built for {found}, not {self.arch}")
        self.require(self.options.build.encode() in data, f"the {what} does not embed version {self.options.build}")
        return candidates[0]

    def command_line(self, root):
        tree = list(files(root))
        self.notice_files(tree, top_level=True)
        binary = self.executable([full for rel, full in tree if rel in CLI_NAMES], "command-line")
        if binary is None or not runs_here(binary.read_bytes()):
            return  # Cross-built (Intel on Apple silicon): checked above, not run.
        binary.chmod(0o755)
        try:
            out = subprocess.run([str(binary), "--version"], capture_output=True, text=True, timeout=30)
        except (OSError, subprocess.SubprocessError) as error:
            self.problems.append(f"the command line did not start: {error}")
            return
        self.require(out.returncode == 0 and self.options.build in out.stdout,
                     f"`ibis --version` did not report {self.options.build}")

    def mac_app(self, root):
        apps = [p for p in Path(root).rglob("*.app") if (p / "Contents/Info.plist").is_file()]
        apps = [p for p in apps if not any(parent.suffix == ".app" for parent in p.relative_to(root).parents)]
        if len(apps) != 1:
            self.problems.append(f"{len(apps)} apps in the package, not one")
            return
        app, options = apps[0], self.options
        info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
        self.require(info.get("CFBundleShortVersionString") == options.version,
                     f"Info.plist says version {info.get('CFBundleShortVersionString')!r}, not {options.version}")
        if options.identifier:
            self.require(info.get("CFBundleIdentifier") == options.identifier,
                         f"Info.plist names {info.get('CFBundleIdentifier')!r}, not {options.identifier}")
        self.executable([app / "Contents/MacOS" / str(info.get("CFBundleExecutable", "?"))], "app")
        if shutil.which("codesign") is None:
            self.problems.append("codesign is needed to check a macOS app")
            return
        signed = subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], capture_output=True, text=True)
        self.require(signed.returncode == 0, f"the app's signature is not valid: {signed.stderr.strip()[:200]}")
        if options.signing_leaf:
            with tempfile.TemporaryDirectory() as work:
                prefix = Path(work, "certificate")
                subprocess.run(["codesign", "-d", f"--extract-certificates={prefix}", str(app)], capture_output=True)
                leaf = Path(f"{prefix}0")
                digest = hashlib.sha1(leaf.read_bytes()).hexdigest() if leaf.is_file() else "nothing"
            self.require(digest.lower() == options.signing_leaf.lower(), f"the app is signed by {digest}, not the pinned certificate")

    def linux_app(self, root):
        self.executable([full for rel, full in files(root) if rel.startswith("usr/bin/") and Path(rel).name not in CLI_NAMES], "app")

    def windows_app(self, root):
        self.executable([full for rel, full in files(root)
                         if rel.lower().endswith(".exe") and Path(rel).name.lower() not in ("uninstall.exe", "ibis.exe")
                         and not rel.startswith("$")], "app")


def check_package(path, notices, options, target):
    check = Check(notices, options, PLATFORMS[target][1])
    mounts = []
    with tempfile.TemporaryDirectory() as work:
        try:
            try:
                root, meta = unpack(path, Path(work), mounts)
            except OPEN_ERRORS as error:
                raise Failure(f"{path.name} could not be opened: {error}") from error
            name = path.name
            if name.startswith("ibis-"):
                check.command_line(root)
            else:
                check.notice_files(list(files(root)), top_level=False)
                if name.endswith((".dmg", ".app.tar.gz")):
                    check.mac_app(root)
                elif name.endswith((".deb", ".rpm")):
                    check.require(meta.get("Version") == options.version, f"the package says version {meta.get('Version')!r}")
                    check.linux_app(root)
                elif name.endswith(".AppImage"):
                    check.linux_app(root)
                elif name.endswith("-setup.exe"):
                    check.require(meta.get("ProductVersion") == options.version,
                                  f"the setup executable says version {meta.get('ProductVersion')!r}")
                    check.windows_app(root)
        except OPEN_ERRORS as error:
            raise Failure(f"{path.name} could not be read: {error}") from error
        finally:
            for mount in mounts:
                subprocess.run(["hdiutil", "detach", str(mount), "-force"], capture_output=True)
    if check.problems:
        raise Failure(f"{path.name}: " + "; ".join(check.problems))


def load_notices(folder):
    notices = {}
    for name in NOTICE_FILES:
        file = Path(folder, name)
        if not file.is_file():
            raise Failure(f"the notices folder has no {name}")
        data = file.read_bytes()
        if MARKERS[name] not in data[:4096]:
            raise Failure(f"{name} in the notices folder is not the generated notice")
        notices[name] = hashlib.sha256(data).hexdigest()
    return notices


def check_sums(dist, target, packages):
    listed = {}
    for line in Path(dist, f"SHA256SUMS-{target}.txt").read_text(encoding="utf-8").splitlines():
        if line.strip():
            digest, name = line.split(maxsplit=1)
            listed[name.lstrip("*")] = digest.lower()
    if sorted(listed) != sorted(packages):
        raise Failure(f"SHA256SUMS-{target}.txt lists {sorted(listed)}, not {sorted(packages)}")
    for name in packages:
        if hashlib.sha256(Path(dist, name).read_bytes()).hexdigest() != listed[name]:
            raise Failure(f"{name} does not match its checksum")


def verify(dist, target, notices_folder, options):
    dist = Path(dist)
    notices = load_notices(notices_folder)
    packages = expected_packages(target, sorted(p.name for p in dist.iterdir()))
    check_sums(dist, target, packages)
    failures = []
    for name in packages:
        try:
            check_package(dist / name, notices, options, target)
            print(f"ok   {name}")
        except Failure as failure:
            failures.append(str(failure))
            print(f"FAIL {failure}")
    if failures:
        raise Failure(f"{len(failures)} of {len(packages)} packages failed")


def repack_without_notice(source, planted):
    """The command-line archive SOURCE again, without THIRD-PARTY-NOTICES.txt."""
    with tempfile.TemporaryDirectory() as work:
        root, _ = unpack(source, Path(work), [])
        Path(root, "THIRD-PARTY-NOTICES.txt").unlink()
        if planted.name.endswith(".zip"):
            with zipfile.ZipFile(planted, "w") as archive:
                for rel, full in files(root):
                    archive.write(full, rel)
        else:
            with tarfile.open(planted, "w:gz") as archive:
                for rel, full in files(root):
                    archive.add(full, rel)


def self_test(dist, target, notices_folder, options):
    """Planted bad copies of the real packages must each be refused."""
    dist = Path(dist)
    notices = load_notices(notices_folder)
    packages = expected_packages(target, sorted(p.name for p in dist.iterdir()))
    wrong = argparse.Namespace(**{**vars(options), "version": options.version + ".9", "build": options.build + ".9"})
    passed_open = []
    with tempfile.TemporaryDirectory() as work:
        for name in packages:
            planted = Path(work, name)
            data = Path(dist, name).read_bytes()
            planted.write_bytes(data[: len(data) // 2])
            for label, path, opts in [("cut in half", planted, options), ("with a wrong version", Path(dist, name), wrong)]:
                try:
                    check_package(path, notices, opts, target)
                    passed_open.append(f"{name} {label}")
                except Failure:
                    pass
            if name.startswith("ibis-"):
                repack_without_notice(Path(dist, name), planted)
                try:
                    check_package(planted, notices, options, target)
                    passed_open.append(f"{name} without its notices")
                except Failure:
                    pass
            planted.unlink()
    if passed_open:
        raise Failure("planted bad packages passed: " + ", ".join(passed_open))
    print(f"ok   every planted bad copy of the {len(packages)} packages was refused")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("dist")
    parser.add_argument("target")
    parser.add_argument("notices")
    parser.add_argument("--version", required=True)
    parser.add_argument("--build")
    parser.add_argument("--identifier")
    parser.add_argument("--signing-leaf")
    parser.add_argument("--self-test", action="store_true")
    options = parser.parse_args()
    options.build = options.build or options.version
    try:
        if options.self_test:
            self_test(options.dist, options.target, options.notices, options)
        verify(options.dist, options.target, options.notices, options)
    except Failure as failure:
        print(f"::error::verify-packages: {failure}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
