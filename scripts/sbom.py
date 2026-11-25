#!/usr/bin/env python3
"""Write the CycloneDX SBOMs for every package Preview builds (#50).

    python3 scripts/sbom.py OUT VERSION
    python3 scripts/sbom.py --check VERSION FILE...

Preview's notices job runs it, beside the notices, from the same lockfiles.
For each system OUT receives two CycloneDX 1.6 JSON files named like the
downloads they describe:

- `Ibis-SYSTEM.cdx.json`, for every app package (disk image, app archive,
  setup executable, deb, rpm, AppImage): the Rust crates of the app for that
  target and of the command line it bundles, and the interface's npm packages;
- `ibis-SYSTEM-cli.cdx.json`, for the command-line archive: its crates alone,
  for its own target (static musl on Linux). Names differing only in case
  would collide on macOS and Windows.

Crates are what `cargo tree` shows the package linking for that platform with
a release build's features, through normal and build dependencies (never
dev-only or inactive optional ones), each with its registry checksum; the
citeproc-js, CSL and Zotero files Ibis embeds come from their vendor
manifests with upstream revisions and hashes; npm packages are pnpm's
production list. The sbom job attests each package with
its file, and publishing attaches the files to the release. Offline: Cargo
reads the crates `cargo fetch` already fetched.
"""

import json
import subprocess
import sys
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# app target: (download name, the command line's target)
PLATFORMS = {
    "aarch64-apple-darwin": ("macOS-Apple-silicon", "aarch64-apple-darwin"),
    "x86_64-apple-darwin": ("macOS-Intel", "x86_64-apple-darwin"),
    "x86_64-pc-windows-msvc": ("Windows", "x86_64-pc-windows-msvc"),
    "x86_64-unknown-linux-gnu": ("Linux", "x86_64-unknown-linux-musl"),
}
# A fixed namespace, so the same inputs always give the same serial number.
NAMESPACE = uuid.UUID("5f1b9c3e-7a51-4d1e-9a39-6b0b3c2f8e10")


def fail(message):
    print(f"sbom: {message}", file=sys.stderr)
    sys.exit(1)


def lock_checksums():
    checksums, name, version = {}, None, None
    for line in (ROOT / "Cargo.lock").read_text(encoding="utf-8").splitlines():
        if line == "[[package]]":
            name = version = None
        elif line.startswith("name = "):
            name = json.loads(line[7:])
        elif line.startswith("version = "):
            version = json.loads(line[10:])
        elif line.startswith("checksum = "):
            checksums[(name, version)] = json.loads(line[11:])
    return checksums


# The features a release build turns on beyond the defaults (tauri build).
RELEASE_FEATURES = {"ibis-desktop": ["tauri/custom-protocol"]}


def crates(root_name, target):
    """Third-party crates `root_name` links for `target` with the features a
    release build activates, as CycloneDX components (cargo tree, offline)."""
    command = ["cargo", "tree", "--locked", "--offline", "-p", root_name, "--target", target, "-e", "normal,build",
               "--prefix", "none", "--format", "{p}|{l}"]
    for feature in RELEASE_FEATURES.get(root_name, []):
        command += ["--features", feature]
    try:
        out = subprocess.run(command, cwd=ROOT, check=True, capture_output=True, text=True).stdout
    except (OSError, subprocess.CalledProcessError) as error:
        fail(f"cargo tree failed for {root_name} on {target}: {error}")
    checksums = lock_checksums()
    components = {}
    for line in out.splitlines():
        package, _, licence = line.removesuffix(" (*)").partition("|")
        words = package.split()
        if len(words) < 2 or not words[1].startswith("v"):
            continue
        if len(words) > 2:
            continue  # A path crate, Ibis's own: the SBOM's subject, not a dependency.
        name, version = words[0], words[1][1:]
        component = {"type": "library", "name": name, "version": version, "purl": f"pkg:cargo/{name}@{version}"}
        if licence.strip():
            component["licenses"] = [{"expression": licence.strip()}]
        checksum = checksums.get((name, version))
        if checksum:
            component["hashes"] = [{"alg": "SHA-256", "content": checksum}]
        components[component["purl"]] = component
    if not components:
        fail(f"no crates found for {root_name} on {target}")
    return list(components.values())


# Third-party code and data Ibis embeds from source, with the manifests that
# pin their upstream revisions and hashes; both the app and the command line
# carry them (NOTICE.md).
VENDORED = {
    "crates/ibis-csl/vendor/manifest.json": {"zotero": "CPAL-1.0 OR AGPL-3.0-or-later", "styles": "CC-BY-SA-3.0",
                                              "locales": "CC-BY-SA-3.0"},
    "crates/ibis-sources/vendor/zotero/manifest.json": {},
}


def vendored():
    components = []
    for manifest, licences in VENDORED.items():
        data = json.loads((ROOT / manifest).read_text(encoding="utf-8"))
        files = {}
        for file in data.get("files", []):
            files.setdefault(file["source"], []).append(file)
        for source, origin in data["sources"].items():
            repo, commit = origin["repo"], origin["commit"]
            component = {
                "type": "library" if files.get(source) else "data",
                "name": repo,
                "version": origin.get("tag", commit),
                "purl": f"pkg:github/{repo}@{commit}",
                "externalReferences": [{"type": "vcs", "url": f"https://github.com/{repo}"}],
            }
            if licences.get(source):
                component["licenses"] = [{"expression": licences[source]}]
            if files.get(source):
                component["components"] = [
                    {"type": "file", "name": file["path"], "hashes": [{"alg": "SHA-256", "content": file["sha256"]}]}
                    for file in files[source]
                ]
            components.append(component)
    return components


def npm_packages():
    try:
        listed = subprocess.run(["pnpm", "-C", str(ROOT / "apps/desktop"), "licenses", "list", "--prod", "--json"],
                                check=True, capture_output=True, text=True).stdout
    except (OSError, subprocess.CalledProcessError) as error:
        fail(f"pnpm licenses list failed: {error}")
    components = []
    for licence, entries in json.loads(listed).items():
        for entry in entries:
            for version in entry["versions"]:
                name = entry["name"]
                purl_name = "%40" + name[1:] if name.startswith("@") else name
                components.append({
                    "type": "library",
                    "name": name,
                    "version": version,
                    "purl": f"pkg:npm/{purl_name}@{version}",
                    "licenses": [{"expression": licence}],
                })
    if not components:
        fail("pnpm listed no production packages")
    return components


def bom(name, version, description, components):
    unique = {component["purl"]: component for component in components}
    ordered = [unique[purl] | {"bom-ref": purl} for purl in sorted(unique)]
    serial = uuid.uuid5(NAMESPACE, json.dumps([name, version, sorted(unique)]))
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "serialNumber": f"urn:uuid:{serial}",
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "name": name,
                "version": version,
                "description": description,
                "licenses": [{"expression": "AGPL-3.0-or-later"}],
                "externalReferences": [{"type": "vcs", "url": "https://github.com/ibiscite/ibis"}],
            },
            "tools": {"components": [{"type": "application", "name": "scripts/sbom.py"}]},
        },
        "components": ordered,
    }


def check(version, paths):
    """Each file is one of these SBOMs, for VERSION, listing its components."""
    for path in paths:
        try:
            document = json.loads(Path(path).read_text(encoding="utf-8"))
        except (OSError, ValueError) as error:
            fail(f"{path} is not readable JSON: {error}")
        subject = document.get("metadata", {}).get("component", {})
        components = document.get("components") or []
        if document.get("bomFormat") != "CycloneDX" or document.get("specVersion") != "1.6":
            fail(f"{path} is not a CycloneDX 1.6 SBOM")
        if subject.get("version") != version or subject.get("name") not in ("Ibis", "ibis"):
            fail(f"{path} describes {subject.get('name')} {subject.get('version')}, not Ibis {version}")
        if len(components) < 50 or not all(c.get("purl") and c.get("version") for c in components):
            fail(f"{path} lists {len(components)} components, or one without a package URL")
        print(f"ok   {path}: {len(components)} components")


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "--check":
        check(sys.argv[2], sys.argv[3:])
        return
    if len(sys.argv) != 3:
        print(__doc__.strip(), file=sys.stderr)
        sys.exit(2)
    out, version = Path(sys.argv[1]), sys.argv[2]
    out.mkdir(parents=True, exist_ok=True)
    npm = npm_packages()
    for target, (platform, cli_target) in PLATFORMS.items():
        cli = crates("ibis", cli_target) + vendored()
        # The app bundles this same command line (scripts/build-cli.sh).
        app = crates("ibis-desktop", target) + cli + npm
        files = {
            f"Ibis-{platform}.cdx.json": bom("Ibis", version, f"The Ibis app for {platform}", app),
            f"ibis-{platform}-cli.cdx.json": bom("ibis", version, f"The ibis command line for {platform}", cli),
        }
        for name, document in files.items():
            (out / name).write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8")
            print(f"{name}: {len(document['components'])} components")


if __name__ == "__main__":
    main()
