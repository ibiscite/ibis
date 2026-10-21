"""Regenerate desktop assets using the locked Tauri CLI and librsvg; no Python packages."""

from pathlib import Path
import shutil
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def render(source, size, output):
    subprocess.run([
        "rsvg-convert", "--width", str(size), "--height", str(size),
        "--output", str(output), str(ROOT / "source" / source),
    ], check=True)


with tempfile.TemporaryDirectory(prefix="ibis-icons-") as directory:
    temp = Path(directory)
    subprocess.run([
        "cargo", "tauri", "icon", str(ROOT / "source/full.svg"),
        "--output", str(temp), "--png", "32,128,256,1024",
    ], check=True)
    for name, size in [("32x32.png", 32), ("128x128.png", 128),
                       ("128x128@2x.png", 256), ("source.png", 1024), ("icon.png", 1024)]:
        shutil.copyfile(temp / f"{size}x{size}.png", ROOT / name)

    # Render the small master at each target size; never shrink a large bitmap.
    images = []
    for size in [16, 24, 32, 48, 256]:
        output = temp / f"windows-{size}.png"
        render("windows-small.svg" if size <= 24 else "windows.svg", size, output)
        images.append((size, output.read_bytes()))
    entries, payload = bytearray(), bytearray()
    for size, data in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32,
                               len(data), 6 + len(images) * 16 + len(payload))
        payload += data
    (ROOT / "icon.ico").write_bytes(struct.pack("<HHH", 0, 1, len(images)) + entries + payload)

    payload = bytearray()
    for kind, size in [("icp4", 16), ("icp5", 32), ("icp6", 64), ("ic07", 128),
                       ("ic08", 256), ("ic09", 512), ("ic10", 1024), ("ic11", 32),
                       ("ic12", 64), ("ic13", 256), ("ic14", 512)]:
        output = temp / f"{kind}.png"
        render("macos-small.svg" if size <= 32 else "macos.svg", size, output)
        data = output.read_bytes()
        payload += kind.encode() + struct.pack(">I", len(data) + 8) + data
    (ROOT / "icon.icns").write_bytes(b"icns" + struct.pack(">I", len(payload) + 8) + payload)

    for size in [128, 256, 512]:
        output = ROOT / "linux" / f"{size}x{size}" / "io.github.ibiscite.ibis.png"
        output.parent.mkdir(parents=True, exist_ok=True)
        render("gnome.svg", size, output)
