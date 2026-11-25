#!/usr/bin/env python3
"""Write the CycloneDX SBOMs for one target's packages (#50).

    python3 scripts/sbom.py OUT TARGET CLI_TARGET VERSION
    python3 scripts/sbom.py --check VERSION FILE...

Preview's build job runs it after collecting the packages. OUT receives two
CycloneDX 1.6 JSON files named like the downloads they describe:

- `Ibis-PLATFORM.cdx.json`, for every app package of the target (disk image,
  app archive, setup executable, deb, rpm, AppImage): the Rust crates of the
  app for TARGET and of the bundled command line for CLI_TARGET, and the npm
  packages of the interface;
- `ibis-PLATFORM-cli.cdx.json`, for the command-line archive: its crates
  alone. (Names differing only in case would collide on macOS and Windows.)

Crates are what Cargo.lock resolves for that platform through normal and build
dependencies (never dev-only ones), each with its registry checksum; npm
packages are pnpm's production list. The SBOM job attests each package with
its file, and publishing attaches the files to the release. Offline: Cargo
reads the crates the build already fetched.
"""

import json
import subprocess
import sys
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PLATFORMS = {
    "aarch64-apple-darwin": "macOS-Apple-silicon",
    "x86_64-apple-darwin": "macOS-Intel",
    "x86_64-pc-windows-msvc": "Windows",
    "x86_64-unknown-linux-gnu": "Linux",
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


def crates(root_name, target):
    """Third-party crates `root_name` links for `target`, as CycloneDX components."""
    try:
        out = subprocess.run(["cargo", "metadata", "--format-version", "1", "--locked", "--offline", "--filter-platform", target],
                             cwd=ROOT, check=True, capture_output=True, text=True).stdout
    except (OSError, subprocess.CalledProcessError) as error:
        fail(f"cargo metadata failed for {target}: {error}")
    metadata = json.loads(out)
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    roots = [package["id"] for package in metadata["packages"] if package["name"] == root_name and package["source"] is None]
    if len(roots) != 1:
        fail(f"no single workspace crate {root_name}")
    seen, stack = set(), roots[:]
    while stack:
        node = nodes[stack.pop()]
        for dep in node["deps"]:
            if any(kind["kind"] != "dev" for kind in dep["dep_kinds"]) and dep["pkg"] not in seen:
                seen.add(dep["pkg"])
                stack.append(dep["pkg"])
    checksums = lock_checksums()
    components = []
    for package in (packages[i] for i in seen):
        if package["source"] is None:
            continue  # Ibis's own crates: the SBOM's subject, not a dependency.
        component = {
            "type": "library",
            "name": package["name"],
            "version": package["version"],
            "purl": f"pkg:cargo/{package['name']}@{package['version']}",
        }
        if package.get("license"):
            component["licenses"] = [{"expression": package["license"]}]
        checksum = checksums.get((package["name"], package["version"]))
        if checksum:
            component["hashes"] = [{"alg": "SHA-256", "content": checksum}]
        components.append(component)
    if not components:
        fail(f"no crates found for {root_name} on {target}")
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
    if len(sys.argv) != 5:
        print(__doc__.strip(), file=sys.stderr)
        sys.exit(2)
    out, target, cli_target, version = Path(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
    platform = PLATFORMS.get(target) or fail(f"no packages are known for {target}")
    out.mkdir(parents=True, exist_ok=True)
    cli = crates("ibis", cli_target)
    app = crates("ibis-desktop", target) + cli + npm_packages()
    files = {
        f"Ibis-{platform}.cdx.json": bom("Ibis", version, f"The Ibis app for {platform}", app),
        f"ibis-{platform}-cli.cdx.json": bom("ibis", version, f"The ibis command line for {platform}", cli),
    }
    for name, document in files.items():
        (out / name).write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8")
        print(f"{name}: {len(document['components'])} components")


if __name__ == "__main__":
    main()
