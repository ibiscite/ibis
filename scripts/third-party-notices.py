#!/usr/bin/env python3
"""Write the notices folder every Ibis package ships (#1271).

    python3 scripts/third-party-notices.py OUT_DIR

OUT_DIR receives LICENSE, NOTICE.md and THIRD-PARTY-NOTICES.txt: the
copyright and licence text of every Rust crate and npm package compiled into
the `ibis` command line and the Ibis app, read from Cargo.lock and
apps/desktop/pnpm-lock.yaml. Preview's notices job runs it once; every build
then puts that folder into the app and the command-line archive.

`cargo about` (CARGO_ABOUT, default `cargo-about` on PATH) reads the crates
offline, for every target Ibis ships, using deny.toml's licence list, so
`cargo deny` stays the one licence policy. `pnpm licenses list --prod` names
the npm packages. The script fails when any crate or package has no licence
text: a package that ships no licence file needs an entry in NO_LICENCE_FILE,
reviewed by hand. Nothing here runs in the app.
"""

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# What Preview builds: the app for four targets, the command line also for musl.
TARGETS = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
]
# The crates whose binaries ship; ibis-fuzz and test helpers never do.
SHIPPED = ["apps/desktop/src-tauri/Cargo.toml", "crates/ibis-cli/Cargo.toml"]
# npm packages that ship no licence file, keyed by exact name@version so an
# update is reviewed again. The text is the licence's standard text in
# scripts/licences/, with the copyright line the package itself states.
NO_LICENCE_FILE = {
    "@fluent/bundle@0.19.1": {"licence": "Apache-2.0", "copyright": "Project Fluent Authors (Mozilla)"},
    "@fluent/syntax@0.19.0": {"licence": "Apache-2.0", "copyright": "Project Fluent Authors (Mozilla)"},
    "client-only@0.0.1": {"licence": "MIT", "copyright": "the client-only authors (https://reactjs.org/); the package names no holder"},
}
LICENCE_FILE_PREFIXES = ("license", "licence", "copying", "notice", "unlicense")
HEADER = "Third-party notices for Ibis"


def fail(message):
    print(f"third-party-notices: {message}", file=sys.stderr)
    sys.exit(1)


def about_config(directory):
    """deny.toml's allowed licences, as cargo-about's accepted list."""
    deny = tomllib.loads((ROOT / "deny.toml").read_text(encoding="utf-8"))
    accepted = deny["licenses"]["allow"]
    path = Path(directory) / "about.toml"
    path.write_text(
        f"accepted = {json.dumps(accepted)}\n"
        f"targets = {json.dumps(TARGETS)}\n"
        "ignore-dev-dependencies = true\n"
        "private = { ignore = true }\n",
        encoding="utf-8",
    )
    return path


def rust_crates(tool, directory):
    """{(name, version): {"licence", "authors", "texts": [(id, name, text)], "dir"}}."""
    config = about_config(directory)
    crates = {}
    for manifest in SHIPPED:
        out = Path(directory) / "about.json"
        command = [tool, "generate", "--format", "json", "--frozen", "--fail", "-c", str(config), "-m", str(ROOT / manifest), "-o", str(out)]
        try:
            subprocess.run(command, check=True, cwd=ROOT)
        except FileNotFoundError:
            fail(f"{tool} not found; install cargo-about (see Preview's notices job)")
        except subprocess.CalledProcessError as error:
            fail(f"cargo about failed for {manifest} (exit {error.returncode})")
        report = json.loads(out.read_text(encoding="utf-8"))
        for crate in report["crates"]:
            package = crate["package"]
            crates.setdefault((package["name"], package["version"]), {
                "licence": package.get("license") or "",
                "authors": package.get("authors") or [],
                "texts": set(),
                "dir": Path(package["manifest_path"]).parent,
            })
        for licence in report["licenses"]:
            for use in licence["used_by"]:
                key = (use["crate"]["name"], use["crate"]["version"])
                if key not in crates:
                    fail(f"cargo about names a licence for {key[0]} {key[1]} but not the crate")
                crates[key]["texts"].add((licence["id"], licence["name"], licence["text"]))
    if not crates:
        fail("cargo about found no crates")
    for (name, version), crate in crates.items():
        if not any(text.strip() for _, _, text in crate["texts"]):
            fail(f"no licence text for the Rust crate {name} {version}")
        # Apache-2.0 section 4(d): pass on the NOTICE file a crate ships.
        for path in sorted(crate["dir"].glob("NOTICE*")):
            if path.is_file():
                crate["texts"].add(("NOTICE", f"{name} NOTICE file", path.read_text(encoding="utf-8", errors="replace")))
    return crates


def npm_packages():
    """{(name, version): {"licence", "authors", "texts": [(id, name, text)]}}."""
    try:
        listed = subprocess.run(["pnpm", "-C", str(ROOT / "apps/desktop"), "licenses", "list", "--prod", "--json"],
                                check=True, capture_output=True, text=True).stdout
    except FileNotFoundError:
        fail("pnpm not found")
    except subprocess.CalledProcessError as error:
        fail(f"pnpm licenses list failed: {error.stderr.strip()}")
    packages = {}
    for licence, entries in json.loads(listed).items():
        for entry in entries:
            for version, path in zip(entry["versions"], entry["paths"]):
                key = (entry["name"], version)
                texts = set()
                for file in sorted(Path(path).iterdir()):
                    if file.is_file() and file.name.lower().startswith(LICENCE_FILE_PREFIXES):
                        texts.add((licence, f"{entry['name']} {file.name}", file.read_text(encoding="utf-8", errors="replace")))
                if not texts:
                    reviewed = NO_LICENCE_FILE.get(f"{entry['name']}@{version}")
                    if reviewed is None:
                        fail(f"the npm package {entry['name']} {version} ships no licence file; add it to NO_LICENCE_FILE after review")
                    if reviewed["licence"] != licence:
                        fail(f"{entry['name']} {version} now declares {licence}, not {reviewed['licence']}; review it again")
                    text = (ROOT / "scripts/licences" / f"{licence}.txt").read_text(encoding="utf-8")
                    text = text.replace("<year> <copyright holders>", reviewed["copyright"])
                    texts.add((licence, licence, text))
                author = entry.get("author")
                packages[key] = {"licence": licence, "authors": [author] if author else [], "texts": texts}
    if not packages:
        fail("pnpm listed no production packages")
    return packages


def render(crates, packages):
    groups = {}
    for kind, items in (("Rust crate", crates), ("npm package", packages)):
        for (name, version), item in items.items():
            for licence_id, licence_name, text in item["texts"]:
                digest = hashlib.sha256(text.strip().encode()).hexdigest()
                group = groups.setdefault(digest, {"id": licence_id, "name": licence_name, "text": text.strip(), "users": set()})
                group["users"].add(f"{name} {version}")

    def listing(items):
        lines = []
        for (name, version), item in sorted(items.items()):
            authors = "; ".join(item["authors"])
            lines.append(f"  {name} {version}: {item['licence']}" + (f" ({authors})" if authors else ""))
        return lines

    out = [
        HEADER,
        "",
        "Ibis is free software under the GNU Affero General Public License, version 3",
        "or later (LICENSE). NOTICE.md names the data files Ibis embeds.",
        "",
        "Below are the Rust crates and npm packages compiled into the ibis command",
        "line and the Ibis app, then each licence text they are used under. This file",
        "is generated at build time from Cargo.lock and apps/desktop/pnpm-lock.yaml.",
        "",
        f"Rust crates ({len(crates)})",
        *listing(crates),
        "",
        f"npm packages ({len(packages)})",
        *listing(packages),
        "",
        "Licence texts",
    ]
    for group in sorted(groups.values(), key=lambda g: (g["id"], sorted(g["users"])[0])):
        out += ["", "=" * 78, f"{group['name']} ({group['id']})", "Used by: " + ", ".join(sorted(group["users"])), "-" * 78, group["text"]]
    return "\n".join(out) + "\n"


def main():
    if len(sys.argv) != 2:
        print(__doc__.strip(), file=sys.stderr)
        sys.exit(2)
    target = Path(sys.argv[1])
    target.mkdir(parents=True, exist_ok=True)
    tool = os.environ.get("CARGO_ABOUT", "cargo-about")
    with tempfile.TemporaryDirectory() as directory:
        crates = rust_crates(tool, directory)
    packages = npm_packages()
    (target / "THIRD-PARTY-NOTICES.txt").write_text(render(crates, packages), encoding="utf-8")
    for name in ("LICENSE", "NOTICE.md"):
        shutil.copyfile(ROOT / name, target / name)
    print(f"Wrote notices for {len(crates)} Rust crates and {len(packages)} npm packages to {target}")


if __name__ == "__main__":
    main()
