"""Adapt the final brand master to desktop formats; only Python and librsvg are needed."""

from pathlib import Path
import argparse
import importlib.util
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("ibis_brand", ROOT.parents[3] / "brand/build.py")
brand = importlib.util.module_from_spec(spec)
spec.loader.exec_module(brand)


def tile(small=False):
    if not small:
        return brand.app_icon(False, "a")
    defs, body = brand.background(1024, 1024, brand.TILE, "a",
                                 brand.L["app_icon_radius"] * 1024, brand.C["tile_edge"])
    mark_defs, mark = brand.centred("mark", 1024, brand.CONTAINER * 1024,
                                   brand.SCARLET_MID, "a", False, brand.SMALL_NECK)
    return brand.doc(1024, 1024, defs + mark_defs, body + mark)


def inset(svg, x, y, size):
    # Keep the brand's own viewBox, gradients and filter region when adapting a tile.
    content = svg.split(">", 1)[1].rsplit("</svg>", 1)[0]
    return (f'<svg x="{x}" y="{y}" width="{size}" height="{size}" '
            f'viewBox="0 0 1024 1024">{content}</svg>')


def macos(small=False):
    return brand.doc(1024, 1024, "", inset(tile(small), 100, 100, 824))


def gnome():
    # The existing GNOME profile: a 104px face and a 4px front on a 128px grid.
    front = (f'<rect x="12" y="14" width="104" height="104" '
             f'rx="{brand.L["app_icon_radius"] * 104:g}" fill="{brand.TILE[1]}"/>')
    return brand.doc(128, 128, "", front + inset(tile(), 12, 10, 104))


def sources():
    return {
        "full.svg": tile(), "small.svg": tile(True),
        "windows.svg": brand.mark(brand.GRADIENT, "m"),
        "windows-small.svg": brand.favicon(brand.SCARLET_MID),
        "macos.svg": macos(), "macos-small.svg": macos(True), "gnome.svg": gnome(),
        "io.github.ibiscite.ibis-symbolic.svg": brand.favicon("currentColor").replace(
            'width="32" height="32"', 'width="16" height="16"'),
    }

def render(root, source, size, output):
    subprocess.run([
        "rsvg-convert", "--width", str(size), "--height", str(size),
        "--output", str(output), str(root / "source" / source),
    ], check=True, env=brand.RENDER_ENV)


def main(root=ROOT):
    (root / "source").mkdir(parents=True, exist_ok=True)
    for name, svg in sources().items():
        (root / "source" / name).write_text(svg, encoding="utf-8")
    with tempfile.TemporaryDirectory(prefix="ibis-icons-") as directory:
        temp = Path(directory)
        for name, size in [("32x32.png", 32), ("128x128.png", 128),
                           ("128x128@2x.png", 256), ("source.png", 1024), ("icon.png", 1024)]:
            render(root, "small.svg" if size < 48 else "full.svg", size, root / name)

        # Render the small master at each target size; never shrink a large bitmap.
        images = []
        for size in [16, 24, 32, 48, 256]:
            output = temp / f"windows-{size}.png"
            render(root, "windows-small.svg" if size < 48 else "windows.svg", size, output)
            images.append((size, output.read_bytes()))
        entries, payload = bytearray(), bytearray()
        for size, data in images:
            entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32,
                                   len(data), 6 + len(images) * 16 + len(payload))
            payload += data
        (root / "icon.ico").write_bytes(struct.pack("<HHH", 0, 1, len(images)) + entries + payload)

        payload = bytearray()
        for kind, size in [("icp4", 16), ("icp5", 32), ("icp6", 64), ("ic07", 128),
                           ("ic08", 256), ("ic09", 512), ("ic10", 1024), ("ic11", 32),
                           ("ic12", 64), ("ic13", 256), ("ic14", 512)]:
            output = temp / f"{kind}.png"
            render(root, "macos-small.svg" if size <= 32 else "macos.svg", size, output)
            data = output.read_bytes()
            payload += kind.encode() + struct.pack(">I", len(data) + 8) + data
        (root / "icon.icns").write_bytes(b"icns" + struct.pack(">I", len(payload) + 8) + payload)

        for size in [128, 256, 512]:
            output = root / "linux" / f"{size}x{size}" / "io.github.ibiscite.ibis.png"
            output.parent.mkdir(parents=True, exist_ok=True)
            render(root, "gnome.svg", size, output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT)
    main(parser.parse_args().output)
