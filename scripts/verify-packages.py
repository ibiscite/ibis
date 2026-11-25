#!/usr/bin/env python3
"""Open every package Preview built for one target and check what it holds.

    python3 scripts/verify-packages.py DIST TARGET NOTICES

DIST is the folder of collected packages (scripts/collect-packages.sh),
TARGET the app's Rust target, NOTICES the folder that
scripts/third-party-notices.py wrote. Every package must carry those three
notice files byte for byte: the command-line archive at its top, the app in
its `notices` resource folder. A missing or unexpected package, a package that
cannot be opened, a missing tool or a missing or different notice fails.
Nothing here trusts a file name: each package is opened the way its format
needs (tar, zip, ar, rpm, disk image, AppImage, NSIS through 7-Zip).
"""

import gzip
import hashlib
import io
import lzma
import os
import re
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
PLATFORMS = {
    "aarch64-apple-darwin": "macOS-Apple-silicon",
    "x86_64-apple-darwin": "macOS-Intel",
    "x86_64-pc-windows-msvc": "Windows",
    "x86_64-unknown-linux-gnu": "Linux",
}


class Failure(Exception):
    pass


def expected_packages(target, names):
    """(required, optional) package names for TARGET; Nightly has no .rpm."""
    platform = PLATFORMS.get(target)
    if platform is None:
        raise Failure(f"no packages are known for {target}")
    if platform.startswith("macOS"):
        required = [f"Ibis-{platform}.dmg", f"Ibis-{platform}.app.tar.gz", f"ibis-{platform}.tar.gz"]
    elif platform == "Windows":
        required = ["Ibis-Windows-setup.exe", "ibis-Windows.zip"]
    else:
        required = ["Ibis-Linux.deb", "Ibis-Linux.AppImage", "ibis-Linux.tar.gz"]
    optional = ["Ibis-Linux.rpm"] if platform == "Linux" else []
    sums = f"SHA256SUMS-{target}.txt"
    missing = [name for name in required + [sums] if name not in names]
    if missing:
        raise Failure("missing packages: " + ", ".join(missing))
    unexpected = sorted(set(names) - set(required) - set(optional) - {sums})
    if unexpected:
        raise Failure("unexpected files: " + ", ".join(unexpected))
    return [name for name in required + optional if name in names]


# Where each kind of package must hold the notices: the command line at the
# archive's top, the app in its installed resource folder (notices/ beside
# the program on Windows, Contents/Resources on macOS, /usr/lib/<app> on Linux).
NAMES = "(LICENSE|NOTICE\\.md|THIRD-PARTY-NOTICES\\.txt)"
PLACES = {
    "command line": re.compile(NAMES),
    "macOS app": re.compile(r"[^/]+\.app/Contents/Resources/notices/" + NAMES),
    "Linux app": re.compile(r"usr/lib/[^/]+/notices/" + NAMES),
    "Windows app": re.compile(r"(?:\$INSTDIR/)?notices/" + NAMES),
}


def collect(found, path, read):
    """Every file named like a notice, by its normalized path."""
    path = path.replace("\\", "/")
    while path.startswith("./"):
        path = path[2:]
    if path.rsplit("/", 1)[-1] in NOTICE_FILES:
        found[path.lstrip("/")] = read()


def from_tar(data):
    found = {}
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        for member in archive:
            if member.isfile():
                collect(found, member.name, lambda: archive.extractfile(member).read())
    return found


def from_dir(root):
    found = {}
    for dirpath, _, files in os.walk(root):
        for file in files:
            full = Path(dirpath, file)
            if full.is_file() and not full.is_symlink():
                collect(found, full.relative_to(root).as_posix(), full.read_bytes)
    return found


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
        members[name] = data[offset + 60:offset + 60 + size]
        offset += 60 + size + (size % 2)
    return members


def deb(path):
    members = ar_members(path.read_bytes())
    if members.get("debian-binary", b"").strip() != b"2.0":
        raise Failure("not a Debian package")
    data = [name for name in members if name.startswith("data.tar")]
    if len(data) != 1:
        raise Failure("no single data archive in the Debian package")
    return from_tar(decompress(members[data[0]], data[0][len("data.tar"):]))


def rpm_header(data, offset):
    if data[offset:offset + 4] != b"\x8e\xad\xe8\x01":
        raise Failure("damaged rpm header")
    count, size = struct.unpack(">II", data[offset + 8:offset + 16])
    index = offset + 16
    store = index + 16 * count
    tags = {}
    for i in range(count):
        tag, kind, start, _ = struct.unpack(">IIII", data[index + 16 * i:index + 16 * (i + 1)])
        if kind == 6:  # one string
            end = data.index(b"\0", store + start)
            tags[tag] = data[store + start:end].decode()
    return tags, store + size


def rpm(path):
    data = path.read_bytes()
    if data[:4] != b"\xed\xab\xee\xdb":
        raise Failure("not an rpm package")
    _, end = rpm_header(data, 96)
    end += (8 - end % 8) % 8
    tags, payload = rpm_header(data, end)
    cpio = decompress(data[payload:], tags.get(1125, "gzip"))  # PAYLOADCOMPRESSOR
    found, offset = {}, 0
    while True:
        if cpio[offset:offset + 6] not in (b"070701", b"070702"):
            raise Failure("damaged rpm payload")
        size = int(cpio[offset + 54:offset + 62], 16)
        name_size = int(cpio[offset + 94:offset + 102], 16)
        name = cpio[offset + 110:offset + 110 + name_size - 1].decode()
        start = offset + 110 + name_size
        start += (4 - start % 4) % 4
        if name == "TRAILER!!!":
            return found
        collect(found, name, lambda: cpio[start:start + size])
        offset = start + size
        offset += (4 - offset % 4) % 4


def dmg(path):
    if shutil.which("hdiutil") is None:
        raise Failure("hdiutil is needed to open a disk image (macOS only)")
    with tempfile.TemporaryDirectory() as mount:
        subprocess.run(["hdiutil", "attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint", mount, str(path)],
                       check=True, capture_output=True)
        try:
            return from_dir(mount)
        finally:
            subprocess.run(["hdiutil", "detach", mount, "-force"], capture_output=True)


def appimage(path):
    if sys.platform != "linux":
        raise Failure("an AppImage can only be opened on Linux")
    with tempfile.TemporaryDirectory() as work:
        copy = Path(work, "package.AppImage")
        shutil.copyfile(path, copy)
        copy.chmod(0o755)
        subprocess.run([str(copy), "--appimage-extract"], cwd=work, check=True, capture_output=True,
                       env={**os.environ, "APPIMAGE_EXTRACT_AND_RUN": ""}, timeout=300)
        return from_dir(Path(work, "squashfs-root"))


def nsis(path):
    tool = shutil.which("7z") or shutil.which("7z.exe")
    if tool is None:
        raise Failure("7-Zip (7z) is needed to open the setup executable")
    with tempfile.TemporaryDirectory() as work:
        subprocess.run([tool, "x", "-y", f"-o{work}", str(path)], check=True, capture_output=True)
        return from_dir(work)


def place(name):
    """Which rule says where this package holds its notices."""
    if name.startswith("ibis-"):
        return "command line"
    if name.endswith((".dmg", ".app.tar.gz")):
        return "macOS app"
    if name.endswith("-setup.exe"):
        return "Windows app"
    return "Linux app"


def contents(path):
    name = path.name
    if name.endswith(".tar.gz"):
        return from_tar(path.read_bytes())
    if name.endswith(".zip"):
        found = {}
        with zipfile.ZipFile(path) as archive:
            for member in archive.infolist():
                if not member.is_dir():
                    collect(found, member.filename, lambda: archive.read(member))
        return found
    if name.endswith(".deb"):
        return deb(path)
    if name.endswith(".rpm"):
        return rpm(path)
    if name.endswith(".dmg"):
        return dmg(path)
    if name.endswith(".AppImage"):
        return appimage(path)
    if name.endswith("-setup.exe"):
        return nsis(path)
    raise Failure(f"no way to open {name}")


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


def check_package(path, notices):
    try:
        found = contents(path)
    except (OSError, ValueError, EOFError, tarfile.TarError, zipfile.BadZipFile, lzma.LZMAError,
            subprocess.SubprocessError, struct.error, IndexError) as error:
        raise Failure(f"{path.name} could not be opened: {error}") from error
    rule = place(path.name)
    placed = {}
    for member, data in found.items():
        match = PLACES[rule].fullmatch(member)
        if match:
            placed.setdefault(match.group(1), []).append(data)
    problems = []
    for name, digest in notices.items():
        copies = placed.get(name, [])
        if len(copies) != 1:
            problems.append(f"{len(copies)} copies of {name} where the {rule} keeps it")
        elif hashlib.sha256(copies[0]).hexdigest() != digest:
            problems.append(f"{name} differs from the generated notice")
    if problems:
        raise Failure(f"{path.name}: " + "; ".join(problems))


def verify(dist, target, notices_folder):
    dist = Path(dist)
    notices = load_notices(notices_folder)
    names = sorted(p.name for p in dist.iterdir())
    packages = expected_packages(target, names)
    failures = []
    for name in packages:
        try:
            check_package(dist / name, notices)
            print(f"ok   {name}")
        except Failure as failure:
            failures.append(str(failure))
            print(f"FAIL {failure}")
    if failures:
        raise Failure(f"{len(failures)} of {len(packages)} packages failed")


def main():
    if len(sys.argv) != 4:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    try:
        verify(*sys.argv[1:])
    except Failure as failure:
        print(f"::error::verify-packages: {failure}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
