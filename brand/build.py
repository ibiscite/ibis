"""Build every Ibis brand export from master.svg.

Standard library only. It needs librsvg's `rsvg-convert` for PNG, ICO and the
optical centre, and the Inter font for the social preview's text. Run:

    python3 brand/build.py

Sizes and spacing follow brand/README.md. M is the bird's height.
"""

from pathlib import Path
import argparse
import json
import math
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib
from xml.sax.saxutils import escape

HERE = Path(__file__).resolve().parent
OUT = HERE / "exports"
# Match fc-match and Linux CI on every host; macOS otherwise chooses CoreText.
RENDER_ENV = {**os.environ, "PANGOCAIRO_BACKEND": "fc"}

# The values people tune (colours, sizes, depth, motion, words) live in settings.json;
# these names keep the code readable.
SET = json.loads((HERE / "settings.json").read_text(encoding="utf-8"))
C, L, D, MO = SET["colours"], SET["layout"], SET["depth"], SET["motion"]
# Words go into SVG text, so escape &, < and > once here.
W = {k: escape(v) if isinstance(v, str) else [escape(x) for x in v] for k, v in SET["words"].items()}
SCARLET, SCARLET_DARK, SCARLET_MID = C["scarlet"], C["scarlet_dark"], C["scarlet_favicon"]
GRADIENT, GRADIENT_DARK = tuple(C["gradient"]), tuple(C["gradient_dark"])
INK, INK_DARK, SLATE, MIST = C["ink"], C["ink_dark"], C["slate"], C["mist"]
TILE, TILE_DARK, PAPER, NIGHT = tuple(C["tile"]), tuple(C["tile_dark"]), tuple(C["paper"]), tuple(C["night"])
PAD, WORD_CAP, WORD_GAP, CONTAINER = L["pad"], L["word_cap"], L["word_gap"], L["container"]
OPTICAL, LIFT, SMALL_NECK = L["optical_sideways"], L["optical_lift"], L["favicon_neck"]
TAGLINE, SUBLINE = W["tagline"], W["subline"]


def symbols():
    text = (HERE / "master.svg").read_text(encoding="utf-8")
    return {sid: ([float(v) for v in box.split()], body)
            for sid, box, body in re.findall(r'<symbol id="(\w+)" viewBox="([^"]+)">(.*?)</symbol>', text, re.S)}


SYM = symbols()


def ratio(name):
    _, _, w, h = SYM[name][0]
    return w / h


def rasterize(svg):
    """Render SVG text with rsvg-convert; return (width, height, alpha rows)."""
    data = subprocess.run(["rsvg-convert"], input=svg.encode(), capture_output=True, check=True, env=RENDER_ENV).stdout
    pos, chunks, width = 8, [], 0
    while pos < len(data):
        size, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + size]
        if kind == b"IHDR":
            width, height, depth, colour, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if (depth, colour, interlace) != (8, 6, 0):
                raise SystemExit("rsvg-convert must write 8-bit RGBA PNG without interlacing")
        elif kind == b"IDAT":
            chunks.append(body)
        pos += size + 12
    raw, stride, rows, prev = zlib.decompress(b"".join(chunks)), width * 4, [], bytearray(width * 4)
    for y in range(height):
        f, line = raw[y * (stride + 1)], bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - 4] if i >= 4 else 0
            b, c = prev[i], prev[i - 4] if i >= 4 else 0
            if f == 1:
                line[i] = (line[i] + a) & 255
            elif f == 2:
                line[i] = (line[i] + b) & 255
            elif f == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(line[3::4])
        prev = line
    return width, height, rows


def thicken(body, neck):
    if 'stroke-width="8.5"' not in body:
        raise SystemExit("master.svg's neck is no longer 8.5 wide; update thicken() in build.py")
    return body.replace('stroke-width="8.5"', f'stroke-width="{neck}"')


def optical_centre(name, neck=None):
    """The point to centre on: the outline's centre, moved sideways toward the centre of mass."""
    (bx, by, bw, bh), body = SYM[name]
    if neck:
        body = thicken(body, neck)
    scale = 8
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{bw * scale:.0f}" height="{bh * scale:.0f}" '
           f'viewBox="{bx} {by} {bw} {bh}">{body.replace("currentColor", "#000")}</svg>')
    width, height, rows = rasterize(svg)
    total = sx = sy = 0
    for y, row in enumerate(rows):
        for x, a in enumerate(row):
            if a:
                total += a
                sx += (x + 0.5) * a
                sy += (y + 0.5) * a
    mass = (bx + sx / total * bw / width, by + sy / total * bh / height)
    centre = (bx + bw / 2, by + bh / 2)
    # Sideways, the heavy head pulls the eye left, so lean toward the mass. Vertically the head and beak
    # already read as top-heavy, so keep the outline's centre and lift it slightly (classic optical centre).
    return centre[0] + (mass[0] - centre[0]) * OPTICAL, centre[1]


def depth_filter(fid, box):
    """One soft light along the top rim, a shade along the bottom rim, and two shadows: a tight
    contact shadow right under the shape and a wide, faint ambient one.

    The region is fixed in the symbol's units around its whole outline, so the shadow is never cut
    off, even mid-animation when only part of the shape is drawn.
    """
    bx, by, bw, bh = box
    m = 4 * D["shadow_blur"]
    return (
        f'<filter id="{fid}" filterUnits="userSpaceOnUse" x="{bx - m:g}" y="{by - m:g}" '
        f'width="{bw + 2 * m:g}" height="{bh + 2 * m + D["shadow_drop"]:g}" color-interpolation-filters="sRGB">'
        f'<feGaussianBlur in="SourceAlpha" stdDeviation="{D["rim_blur"]}" result="b"/>'
        f'<feOffset in="b" dy="{D["rim_offset"]}" result="bd"/><feComposite in="SourceAlpha" in2="bd" operator="out" result="top"/>'
        f'<feFlood flood-color="#fff" flood-opacity="{D["rim_light"]}"/><feComposite in2="top" operator="in" result="hl"/>'
        f'<feOffset in="b" dy="{-D["rim_offset"]}" result="bu"/><feComposite in="SourceAlpha" in2="bu" operator="out" result="bot"/>'
        f'<feFlood flood-color="#000" flood-opacity="{D["rim_shade"]}"/><feComposite in2="bot" operator="in" result="sh"/>'
        f'<feGaussianBlur in="SourceAlpha" stdDeviation="{D["contact_blur"]}" result="cb"/>'
        f'<feOffset in="cb" dy="{D["contact_drop"]}" result="co"/>'
        f'<feFlood flood-color="{D["shadow_colour"]}" flood-opacity="{D["contact_opacity"]}"/>'
        '<feComposite in2="co" operator="in" result="contact"/>'
        f'<feGaussianBlur in="SourceAlpha" stdDeviation="{D["shadow_blur"]}" result="db"/>'
        f'<feOffset in="db" dy="{D["shadow_drop"]}" result="do"/>'
        f'<feFlood flood-color="{D["shadow_colour"]}" flood-opacity="{D["shadow_opacity"]}"/>'
        '<feComposite in2="do" operator="in" result="drop"/>'
        '<feMerge><feMergeNode in="drop"/><feMergeNode in="contact"/><feMergeNode in="SourceGraphic"/>'
        '<feMergeNode in="hl"/><feMergeNode in="sh"/></feMerge>'
        "</filter>"
    )


def draw(name, k, tx, ty, paint, uid, depth=False, neck=None):
    """The symbol in its own units, scaled by k, with its unit origin moved to (tx, ty)."""
    (_, by, _, bh), body = SYM[name]
    defs, attr = "", ""
    if depth:
        top, bottom = paint
        # Gradient and filter work in the symbol's units, so depth scales with size.
        defs = (f'<linearGradient id="{uid}g" gradientUnits="userSpaceOnUse" x1="0" y1="{by - 1}" x2="0" y2="{by + bh}">'
                f'<stop offset="0" stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient>'
                + depth_filter(f"{uid}f", SYM[name][0]))
        paint, attr = f"url(#{uid}g)", f' filter="url(#{uid}f)"'
    if neck:
        body = thicken(body, neck)       # favicon sizes: no eye, as in the app's small icon
    body = body.replace("currentColor", paint)
    shape = f"<g{attr}>{body}</g>"
    if name == "mark" and not neck:
        mask, shape = cut_eye(body, uid, attr)
        defs += mask
    return defs, f'<g transform="translate({tx:.3f} {ty:.3f}) scale({k:.5f})">{shape}</g>'


def cut_eye(body, uid, attr):
    """Cut the eye out of the bird, so whatever is behind it shows through on any background.

    The cut is made after the depth, so the eye is crisp. `eye_depth` (0 to 1) then fades in the
    depth's own rim light and shade inside the eye, for a slightly inset look.
    """
    bx, by, bw, bh = SYM["mark"][0]
    m = 4 * D["shadow_blur"]
    box = f'x="{bx - m:g}" y="{by - m:g}" width="{bw + 2 * m:g}" height="{bh + 2 * m + D["shadow_drop"]:g}"'
    eye = SYM["eye"][1].replace("currentColor", "#000")
    defs = f'<mask id="{uid}e" maskUnits="userSpaceOnUse" {box}><rect {box} fill="#fff"/>{eye}</mask>'
    shape = f'<g mask="url(#{uid}e)"><g{attr}>{body}</g></g>'
    if attr and D["eye_depth"] > 0:
        (ex, ey, ew, eh), _ = SYM["eye"]
        cx, cy, r = ex + ew / 2, ey + eh / 2, 3.2 * ew      # feathered, so the shading has no edge
        defs += (f'<radialGradient id="{uid}r" gradientUnits="userSpaceOnUse" cx="{cx:g}" cy="{cy:g}" r="{r:g}">'
                 '<stop offset="0.3" stop-color="#fff"/><stop offset="1" stop-color="#000"/></radialGradient>'
                 f'<mask id="{uid}s" maskUnits="userSpaceOnUse" x="{cx - r:g}" y="{cy - r:g}" width="{2 * r:g}" height="{2 * r:g}">'
                 f'<circle cx="{cx:g}" cy="{cy:g}" r="{r:g}" fill="url(#{uid}r)"/></mask>')
        shape += (f'<g class="eyeshade" mask="url(#{uid}s)" opacity="{D["eye_depth"]:g}">'
                  f'<g{attr}><g mask="url(#{uid}e)">{body}</g></g></g>')
    return defs, shape


def place(name, x, y, height, paint, uid, depth=False):
    """The symbol with its outline's top-left at (x, y), `height` tall."""
    bx, by, _, bh = SYM[name][0]
    k = height / bh
    return draw(name, k, x - bx * k, y - by * k, paint, uid, depth)


def centred(name, side, height, paint, uid, depth, neck=None):
    """The symbol `height` tall, its optical centre on the middle of a `side` square."""
    k = height / SYM[name][0][3]
    ox, oy = optical_centre(name, neck)
    return draw(name, k, side / 2 - ox * k, side / 2 - oy * k - LIFT * side, paint, uid, depth, neck)


def fitted_side(name, pad, neck=None):
    """The smallest square, in symbol units, that holds the outline plus `pad` around the optical centre."""
    bx, by, bw, bh = SYM[name][0]
    ox, oy = optical_centre(name, neck)
    extra = ((neck or 8.5) - 8.5) / 2
    reach = max(ox - bx + extra, bx + bw - ox, oy - by, by + bh + extra - oy)
    return 2 * (reach + pad)


def doc(w, h, defs, body, size=None):
    width, height = size or (w, h)
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{width:g}" height="{height:g}" viewBox="0 0 {w:g} {h:g}">'
            f"<defs>{defs}</defs>{body}</svg>\n")


def mark(paint, uid, depth=True):
    bh = SYM["mark"][0][3]
    side = fitted_side("mark", PAD * bh + L["shadow_room"])
    defs, body = centred("mark", side, bh, paint, uid, depth)
    return doc(side, side, defs, body, (512, 512))


def logo(dark, uid):
    m = 100
    pad = PAD * m
    mark_w = m * ratio("mark")
    cap = WORD_CAP * m
    (_, wy, _, wh), _ = SYM["wordmark"]
    word_h = cap * wh / 100                 # the wordmark symbol's cap height is 100
    word_w = word_h * ratio("wordmark")
    baseline = pad + m / 2 + cap / 2        # cap height centred on the bird
    w = pad + mark_w + WORD_GAP * m + word_w + pad
    h = m + 2 * pad
    d1, b1 = place("mark", pad, pad, m, GRADIENT_DARK if dark else GRADIENT, uid, True)
    d2, b2 = place("wordmark", pad + mark_w + WORD_GAP * m, baseline + wy * cap / 100, word_h,
                   INK_DARK if dark else INK, uid + "w")
    return doc(w, h, d1 + d2, b1 + b2, (round(w * 200 / h), 200))


def background(w, h, colours, uid, radius=0, edge=None):
    top, bottom = colours
    defs = (f'<linearGradient id="{uid}bg" x1="0" y1="0" x2="0.17" y2="1">'
            f'<stop offset="0" stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient>')
    body = f'<rect width="{w}" height="{h}" rx="{radius}" fill="url(#{uid}bg)"/>'
    if edge:
        # A hairline inside the edge keeps the tile visible on a background of the same colour.
        s = w / 512
        body += (f'<rect x="{s / 2}" y="{s / 2}" width="{w - s}" height="{h - s}" rx="{radius - s / 2}" '
                 f'fill="none" stroke="{edge}" stroke-width="{s}"/>')
    return defs, body


def app_icon(dark, uid):
    side = 1024
    d1, b1 = background(side, side, TILE_DARK if dark else TILE, uid, L["app_icon_radius"] * side,
                        C["tile_edge_dark"] if dark else C["tile_edge"])
    d2, b2 = centred("mark", side, CONTAINER * side, GRADIENT_DARK if dark else GRADIENT, uid, True)
    return doc(side, side, d1 + d2, b1 + b2)


def square_tile(uid):
    """A full square for circle crops and masks: avatars, Apple touch and maskable icons."""
    side = 512
    d1, b1 = background(side, side, TILE, uid)
    d2, b2 = centred("mark", side, CONTAINER * side, GRADIENT, uid, True)
    return doc(side, side, d1 + d2, b1 + b2)


def favicon(paint, style=""):
    side = fitted_side("mark", 5, SMALL_NECK)      # 5 units: at least 1 px clear at 16 px
    defs, body = centred("mark", side, SYM["mark"][0][3], paint, "fv", False, SMALL_NECK)
    return doc(side, side, style + defs, body, (32, 32))


def card(w, h, uid):
    """The social card: logo, lead line and status line, centred as one block."""
    m = 184 * h / 640
    logo_svg = logo(False, uid)
    lw, lh = (float(v) for v in re.search(r'viewBox="0 0 ([\d.]+) ([\d.]+)"', logo_svg).groups())
    k = m * (1 + 2 * PAD) / lh
    inner = re.search(r"<defs>(.*)</defs>(.*)</svg>", logo_svg, re.S)
    tag, sub = 56 * h / 640, 32 * h / 640
    block = lh * k + 0.9 * tag + 1.2 * tag + sub
    x, y = (w - lw * k) / 2, (h - block) / 2
    d1, body = background(w, h, PAPER, uid)
    body += f'<g transform="translate({x:.2f} {y:.2f}) scale({k:.5f})">{inner.group(2)}</g>'
    text = f'font-family="Inter Display, Inter, sans-serif" text-anchor="middle" x="{w / 2:g}"'
    body += f'<text {text} y="{y + lh * k + 1.65 * tag:.1f}" font-size="{tag:.1f}" font-weight="600" fill="{INK}">{TAGLINE}</text>'
    body += f'<text {text} y="{y + block - 0.25 * sub:.1f}" font-size="{sub:.1f}" fill="{SLATE}">{SUBLINE}</text>'
    return doc(w, h, d1 + inner.group(1), body)


def beak():
    m = 100
    w, h = m * ratio("beak") + 2 * PAD * m, m * (1 + 2 * PAD)
    defs, body = place("beak", PAD * m, PAD * m, m, GRADIENT, "bk", True)
    return doc(w, h, defs, body, (round(256 * w / h), 256))


def oklch(lightness, chroma, hue):
    """OKLCH to an sRGB hex string, reducing chroma until the colour fits sRGB."""
    while True:
        a, b = chroma * math.cos(math.radians(hue)), chroma * math.sin(math.radians(hue))
        l_ = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3
        m_ = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3
        s_ = (lightness - 0.0894841775 * a - 1.2914855480 * b) ** 3
        rgb = (4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
               -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
               -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_)
        if all(-0.0005 <= v <= 1.0005 for v in rgb):
            break
        chroma -= 0.002
    srgb = [12.92 * v if v <= 0.0031308 else 1.055 * max(v, 0) ** (1 / 2.4) - 0.055 for v in rgb]
    return "#" + "".join(f"{round(min(1, max(0, v)) * 255):02X}" for v in srgb)


def palette():
    """Scarlet runs from 50 to 900 at the bird's hue; 400 and 600 are the bird's own colours."""
    steps = {50: (0.97, 0.02), 100: (0.94, 0.04), 200: (0.88, 0.08), 300: (0.81, 0.13), 400: SCARLET_DARK,
             500: (0.64, 0.18), 600: SCARLET, 700: (0.47, 0.155), 800: (0.39, 0.125), 900: (0.30, 0.09)}
    scarlet = {k: v if isinstance(v, str) else oklch(v[0], v[1], 34) for k, v in steps.items()}
    # Gray leans faintly toward the ink's cool blue, more in the darks than the lights.
    grays = {50: (0.975, 0.004), 100: (0.955, 0.005), 200: (0.91, 0.007), 300: (0.85, 0.009), 400: (0.72, 0.012),
             500: (0.58, 0.014), 600: (0.47, 0.016), 700: (0.38, 0.016), 800: (0.29, 0.013), 850: (0.25, 0.011),
             900: (0.21, 0.010), 950: (0.16, 0.008)}
    gray = {k: oklch(v[0], v[1], 268) for k, v in grays.items()}
    neutral = {"ink": INK, "slate": SLATE, "mist": MIST, "paper": "#FFFFFF",
               "warm-paper": PAPER[1], "snow": INK_DARK}
    themes = {
        "light": {"bg": "#FFFFFF", "surface": gray[50], "line": gray[200], "text": INK, "muted": SLATE,
                  "accent": SCARLET, "accent-soft": scarlet[50]},
        "dark": {"bg": gray[950], "surface": gray[900], "line": gray[800], "text": INK_DARK, "muted": gray[300],
                 "accent": SCARLET_DARK, "accent-soft": scarlet[900]},
        "dim": {"bg": gray[850], "surface": gray[800], "line": gray[700], "text": INK_DARK, "muted": gray[300],
                "accent": SCARLET_DARK, "accent-soft": scarlet[900]},
    }
    return {"scarlet": scarlet, "gray": gray, "neutral": neutral, "themes": themes}


def write_palette():
    colours = palette()
    (OUT / "colors.json").write_text(json.dumps(colours, indent=2) + "\n", encoding="utf-8")
    lines = [f"  --ibis-scarlet-{k}: {v};" for k, v in colours["scarlet"].items()]
    lines += [f"  --ibis-gray-{k}: {v};" for k, v in colours["gray"].items()]
    lines += [f"  --ibis-{k}: {v};" for k, v in colours["neutral"].items()]
    theme = lambda name: "".join(f"  --{k}: {v};\n" for k, v in colours["themes"][name].items())
    css = ("/* Ibis brand colours. Made by brand/build.py; do not edit.\n"
           "   Themes follow the system unless the page sets data-theme=\"light\", \"dark\" or \"dim\". */\n"
           ":root {\n" + "\n".join(lines) + "\n" + theme("light") + "}\n"
           "@media (prefers-color-scheme: dark) {\n  :root:not([data-theme]) {\n"
           + theme("dark").replace("  --", "    --") + "  }\n}\n"
           ":root[data-theme=\"light\"] {\n" + theme("light") + "}\n"
           ":root[data-theme=\"dark\"] {\n" + theme("dark") + "}\n"
           ":root[data-theme=\"dim\"] {\n" + theme("dim") + "}\n")
    (OUT / "colors.css").write_text(css, encoding="utf-8")


def png(svg_name, out_name, width, height=None):
    """Render at `width`; `height` defaults to square, and "auto" keeps the file's own proportions."""
    args = ["rsvg-convert", "--width", str(width)]
    if height != "auto":
        args += ["--height", str(height or width)]
    subprocess.run(args + ["--output", str(OUT / "png" / out_name), str(OUT / svg_name)], check=True, env=RENDER_ENV)


def ico(svg_name, sizes):
    entries, payload, images = bytearray(), bytearray(), []
    for size in sizes:
        png(svg_name, f"favicon-{size}.png", size)
        images.append((size, (OUT / "png" / f"favicon-{size}.png").read_bytes()))
    for size, data in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data),
                               6 + len(images) * 16 + len(payload))
        payload += data
    (OUT / "favicon.ico").write_bytes(struct.pack("<HHH", 0, 1, len(images)) + entries + payload)


def motion_css():
    """The finished logo is the base style, so renderers that do not animate still show it.
    Each animation starts from its keyframes' first frame and holds it until it begins."""
    t = MO
    return (
        f".neck{{stroke-dasharray:1;animation:draw {t['neck_duration']}s {t['ease']} {t['neck_start']}s backwards}}"
        f".crown{{transform-box:fill-box;transform-origin:18% 22%;"
        f"animation:grow {t['crown_duration']}s {t['crown_ease']} {t['crown_start']}s backwards}}"
        # The eye's shading fades in once the head has grown, so the two copies never show apart.
        f".eyeshade{{animation:fade 0.2s linear {t['crown_start'] + t['crown_duration']:g}s backwards}}"
        f".word{{animation:word {t['word_duration']}s {t['ease']} {t['word_start']}s backwards}}"
        "@keyframes draw{0%{stroke-dashoffset:1;opacity:0}8%{opacity:1}to{stroke-dashoffset:0}}"
        f"@keyframes grow{{0%{{transform:rotate({t['crown_turn']}deg) scale({t['crown_from']});opacity:0}}15%{{opacity:1}}}}"
        f"@keyframes word{{0%{{opacity:0;transform:translateX({-t['word_slide']}px)}}}}"
        "@keyframes fade{0%{opacity:0}}"
        "@media (prefers-reduced-motion:reduce){.neck,.crown,.word,.eyeshade{animation:none}}"
    )


def animate(svg):
    """Tag the neck and group the head with the beak so they grow from the head's centre as one shape."""
    out = svg.replace('<path d="M36', '<path class="neck" pathLength="1" d="M36')
    out = re.sub(r'(<path d="M41 25[^"]*"[^>]*/>)(<circle [^>]*/>)', r'<g class="crown">\1\2</g>', out)
    if 'class="neck"' not in out or 'class="crown"' not in out:
        raise SystemExit("master.svg's bird changed shape order; update animate() in build.py")
    return out.replace("<defs>", f"<defs><style>{motion_css()}</style>", 1)


def animated_logo(dark, uid):
    """The logo drawing itself in about 1.5 s: the neck rises, the head and beak grow out as one shape,
    then the wordmark slides in."""
    head, word = logo(dark, uid).rsplit("<g transform=", 1)
    # Wrap the wordmark: a CSS transform on the group itself would replace its placement.
    return animate(head + '<g class="word"><g transform=' + word.replace("</svg>", "</g></svg>"))


def bird_animated_logo(dark, uid):
    """The logo with only the bird drawing itself and the wordmark already in place: website headers,
    where the name should be readable at once and the bird can redraw when the home link is used."""
    return animate(logo(dark, uid))


def animated_mark(dark, uid):
    """The bird alone drawing itself, for splash screens, avatars in videos and loading states."""
    return animate(mark(GRADIENT_DARK if dark else GRADIENT, uid))


def video_card(w, h, dark, uid, lines, logo_size=0.2):
    """A full-frame card for video and social: the logo above one or two lines of text, centred as a block."""
    m = h * logo_size
    logo_svg = logo(dark, uid)
    lw, lh = (float(v) for v in re.search(r'viewBox="0 0 ([\d.]+) ([\d.]+)"', logo_svg).groups())
    k = m * (1 + 2 * PAD) / lh
    inner = re.search(r"<defs>(.*)</defs>(.*)</svg>", logo_svg, re.S)
    sizes = [h * logo_size * 0.29, h * logo_size * 0.17][:len(lines)]
    block = lh * k + sum(1.9 * z for z in sizes)
    x, y = (w - lw * k) / 2, (h - block) / 2
    d1, body = background(w, h, NIGHT if dark else PAPER, uid)
    body += f'<g transform="translate({x:.2f} {y:.2f}) scale({k:.5f})">{inner.group(2)}</g>'
    top = y + lh * k
    for i, (line, z) in enumerate(zip(lines, sizes)):
        top += 1.9 * z if i else 1.6 * z
        fill = (INK_DARK if dark else INK) if i == 0 else (MIST if dark else SLATE)
        weight = ' font-weight="600"' if i == 0 else ""
        body += (f'<text x="{w / 2:g}" y="{top:.1f}" font-family="Inter Display, Inter, sans-serif" text-anchor="middle" '
                 f'font-size="{z:.1f}"{weight} fill="{fill}">{line}</text>')
    return doc(w, h, d1 + inner.group(1), body)


def thumbnail(uid):
    """A YouTube thumbnail made from the two text lines in settings.json."""
    w, h = 1280, 720
    d1, body = background(w, h, NIGHT, uid)
    d2, bird = centred("mark", 440, 300, GRADIENT_DARK, uid + "m", True)
    body += f'<g transform="translate(760 140)">{bird}</g>'
    text = 'font-family="Inter Display, Inter, sans-serif" font-weight="700"'
    body += f'<text x="80" y="300" font-size="104" fill="{INK_DARK}" {text}>{W["thumbnail"][0]}</text>'
    body += f'<text x="80" y="420" font-size="104" fill="{SCARLET_DARK}" {text}>{W["thumbnail"][1]}</text>'
    body += f'<text x="84" y="500" font-size="36" fill="{MIST}" font-family="Inter, sans-serif">{TAGLINE}</text>'
    return doc(w, h, d1 + d2, body)


MANIFEST = {
    "name": "Ibis", "short_name": "Ibis", "theme_color": SCARLET, "background_color": "#FFFFFF",
    "icons": [
        {"src": "png/icon-192.png", "sizes": "192x192", "type": "image/png"},
        {"src": "png/icon-512.png", "sizes": "512x512", "type": "image/png"},
        {"src": "png/icon-maskable-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable"},
    ],
}


def main(root=HERE.parent):
    global OUT
    OUT = root / "brand/exports"
    (OUT / "png").mkdir(parents=True, exist_ok=True)
    dark_style = f"<style>svg{{color:{SCARLET}}}@media (prefers-color-scheme:dark){{svg{{color:{SCARLET_DARK}}}}}</style>"
    files = {
        "ibis-mark.svg": mark(GRADIENT, "m"),
        "ibis-mark-on-dark.svg": mark(GRADIENT_DARK, "md"),
        "ibis-mark-flat.svg": mark(SCARLET, "mf", False),
        "ibis-mark-black.svg": mark(INK, "mb", False),
        "ibis-mark-white.svg": mark("#FFFFFF", "mw", False),
        "ibis-logo.svg": logo(False, "l"),
        "ibis-logo-on-dark.svg": logo(True, "ld"),
        "ibis-app-icon.svg": app_icon(False, "a"),
        "ibis-app-icon-dark.svg": app_icon(True, "ad"),
        "ibis-avatar.svg": square_tile("av"),
        "ibis-quote-beak.svg": beak(),
        "favicon.svg": favicon("currentColor", dark_style),
        "github-social-preview.svg": card(1280, 640, "s"),
        "og-image.svg": card(1200, 630, "og"),
        "favicon-png.svg": favicon(SCARLET_MID),
        "ibis-logo-animated.svg": animated_logo(False, "la"),
        "ibis-logo-animated-on-dark.svg": animated_logo(True, "lad"),
        "ibis-logo-bird-animated.svg": bird_animated_logo(False, "lb"),
        "ibis-logo-bird-animated-on-dark.svg": bird_animated_logo(True, "lbd"),
        "ibis-mark-animated.svg": animated_mark(False, "ma"),
        "ibis-mark-animated-on-dark.svg": animated_mark(True, "mad"),
        "video/title-card.svg": video_card(1920, 1080, False, "vt", [TAGLINE, SUBLINE]),
        "video/title-card-dark.svg": video_card(1920, 1080, True, "vtd", [TAGLINE, SUBLINE]),
        "video/end-card.svg": video_card(1920, 1080, True, "ve", W["end_card"]),
        "video/youtube-banner.svg": video_card(2560, 1440, True, "yb", [TAGLINE], L["banner_logo"]),
        "video/thumbnail-template.svg": thumbnail("th"),
    }
    (OUT / "video").mkdir(exist_ok=True)
    for name, text in files.items():
        (OUT / name).write_text(text, encoding="utf-8")
    for size in (1024, 512, 256, 128):
        png("ibis-app-icon.svg", f"ibis-app-icon-{size}.png", size)
    png("ibis-app-icon-dark.svg", "ibis-app-icon-dark-1024.png", 1024)
    png("ibis-avatar.svg", "ibis-avatar-512.png", 512)
    png("ibis-mark.svg", "ibis-mark-512.png", 512)
    png("ibis-mark-on-dark.svg", "ibis-mark-on-dark-512.png", 512)
    png("ibis-logo.svg", "ibis-logo-800.png", 800, "auto")
    png("ibis-logo-on-dark.svg", "ibis-logo-on-dark-800.png", 800, "auto")
    png("github-social-preview.svg", "github-social-preview.png", 1280, 640)
    png("og-image.svg", "og-image.png", 1200, 630)
    png("ibis-avatar.svg", "apple-touch-icon.png", 180)
    png("ibis-app-icon.svg", "icon-192.png", 192)
    png("ibis-app-icon.svg", "icon-512.png", 512)
    png("ibis-avatar.svg", "icon-maskable-512.png", 512)
    for name in ("title-card", "title-card-dark", "end-card"):
        png(f"video/{name}.svg", f"video-{name}.png", 1920, 1080)
    png("video/youtube-banner.svg", "video-youtube-banner.png", 2560, 1440)
    png("video/thumbnail-template.svg", "video-thumbnail-template.png", 1280, 720)
    ico("favicon-png.svg", (16, 32, 48))
    (OUT / "favicon-png.svg").unlink()
    (OUT / "site.webmanifest").write_text(json.dumps(MANIFEST, indent=2) + "\n", encoding="utf-8")
    write_palette()
    # The small in-app mark is generated too: no copied geometry in the UI.
    box, body = SYM["beak"]
    ui = root / "apps/desktop/src/brand.ts"
    ui.parent.mkdir(parents=True, exist_ok=True)
    ui.write_text('// Generated by brand/build.py; do not edit.\nexport const beak = '
                  + json.dumps({"viewBox": " ".join(f"{v:g}" for v in box), "body": body})
                  + ';\n', encoding="utf-8")
    subprocess.run([sys.executable, str(HERE.parent / "apps/desktop/src-tauri/icons/source/generate.py"),
                    "--output", str(root / "apps/desktop/src-tauri/icons")], check=True)


def generated_files(root):
    files = set()
    for directory in ("brand/exports", "apps/desktop/src-tauri/icons"):
        for path in (root / directory).rglob("*"):
            if (path.is_file() or path.is_symlink()) and (directory == "brand/exports" or path.suffix in {".svg", ".png", ".ico", ".icns", ".json"}):
                files.add(path.relative_to(root))
    if (root / "apps/desktop/src/brand.ts").exists():
        files.add(Path("apps/desktop/src/brand.ts"))
    return files


def drift(source, generated):
    changed = []
    for name in sorted(generated_files(source) | generated_files(generated)):
        before, after = source / name, generated / name
        if before.is_symlink() or after.is_symlink() or not before.is_file() or not after.is_file() or before.read_bytes() != after.read_bytes():
            changed.append(name.as_posix())
    return changed


def publish(generated, root):
    fresh = generated_files(generated)
    for name in fresh:
        for path in (root / name, *(root / name).parents):
            if path == root:
                break
            if path.is_symlink():
                raise SystemExit(f"Refusing generated output through a symlink: {name.as_posix()}")
    for name in generated_files(root) - fresh:
        (root / name).unlink()
    for name in fresh:
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(generated / name, target)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="rebuild in a temporary folder and report stale assets")
    parser.add_argument("--output", type=Path, default=HERE.parent, help=argparse.SUPPRESS)
    args = parser.parse_args()
    try:
        font = subprocess.run(["fc-match", "-f", "%{family}", "Inter Display"], capture_output=True, text=True, check=True)
        if "Inter Display" not in font.stdout.split(","):
            raise SystemExit("Install Inter Display before rebuilding the brand (Ubuntu: fonts-inter).")
        # Build completely before touching the checkout, even for an ordinary run.
        with tempfile.TemporaryDirectory(prefix="ibis-brand-") as directory:
            output = Path(directory)
            main(output)
            if args.check:
                changed = drift(args.output, output)
                if changed:
                    print("Generated brand files differ; run python3 brand/build.py:", *changed, sep="\n", file=sys.stderr)
                    raise SystemExit(1)
                print("Brand exports, desktop icons and UI artwork are current.")
            else:
                publish(output, args.output)
    except FileNotFoundError as error:
        print(f"Brand build tool is missing: {error.filename}", file=sys.stderr)
        raise SystemExit(2)
