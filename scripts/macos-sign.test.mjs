// The requirement check in scripts/macos-sign.sh refuses any app that is not
// the expected Ibis signed with the pinned certificate (#1242). codesign and
// plutil are stubbed; the shell under test is the real script.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("./macos-sign.sh", import.meta.url));
const leaf = "0123456789abcdef0123456789abcdef01234567";

function check({ bundle, designated, status = 0 }, identifier = "com.ibiscite.ibis.nightly") {
  const dir = mkdtempSync(join(tmpdir(), "ibis-sign-"));
  try {
    writeFileSync(join(dir, "plutil"), `#!/bin/sh\necho '${bundle}'\n`);
    // codesign -d -r- writes the requirement to stderr, after other lines.
    writeFileSync(join(dir, "codesign"), `#!/bin/sh\ncase "$*" in *--verify*) exit ${status};; esac\necho 'Executable=/x' >&2\necho '${designated}' >&2\n`);
    for (const tool of ["plutil", "codesign"]) chmodSync(join(dir, tool), 0o755);
    return spawnSync("bash", [script, "requirement", "Ibis Nightly.app", identifier, leaf], {
      encoding: "utf8",
      env: { ...process.env, PATH: `${dir}:${process.env.PATH}` },
    });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const pinned = `designated => identifier "com.ibiscite.ibis.nightly" and certificate leaf = H"${leaf}"`;

test("the pinned certificate and identifier pass", { skip: process.platform === "win32" }, () => {
  const result = check({ bundle: "com.ibiscite.ibis.nightly", designated: pinned });
  assert.equal(result.status, 0, result.stderr);
});

test("another certificate, an ad-hoc hash or another app is refused", { skip: process.platform === "win32" }, () => {
  for (const [what, app] of [
    ["another certificate", { bundle: "com.ibiscite.ibis.nightly", designated: pinned.replace(leaf, "f".repeat(40)) }],
    ["an ad-hoc build", { bundle: "com.ibiscite.ibis.nightly", designated: 'designated => cdhash H"8d967bc8"' }],
    ["an unsigned build", { bundle: "com.ibiscite.ibis.nightly", designated: "code object is not signed at all" }],
    ["stable signed as nightly", { bundle: "com.ibiscite.ibis", designated: pinned }],
  ]) {
    const result = check(app);
    assert.notEqual(result.status, 0, what);
    assert.match(result.stderr, /expected/, what);
  }
});

test("a matching requirement cannot hide an invalid signature", { skip: process.platform === "win32" }, () => {
  const result = check({ bundle: "com.ibiscite.ibis.nightly", designated: pinned, status: 1 });
  assert.notEqual(result.status, 0, result.stderr);
});

test("setup refuses an omitted or existing destination without calling GitHub", { skip: process.platform === "win32" }, () => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-sign-setup-"));
  const setup = fileURLToPath(new URL("./macos-signing-setup.sh", import.meta.url));
  try {
    writeFileSync(join(dir, "gh"), '#!/bin/sh\ntouch "$CALLED"\nexit 1\n');
    chmodSync(join(dir, "gh"), 0o755);
    writeFileSync(join(dir, "keep"), "existing backup");
    for (const args of [[], [dir]]) {
      const result = spawnSync("bash", [setup, ...args], {
        encoding: "utf8", env: { ...process.env, PATH: `${dir}:${process.env.PATH}`, CALLED: join(dir, "called") },
      });
      assert.equal(result.status, 2, result.stderr);
      assert.match(result.stderr, /Usage/);
      assert.equal(existsSync(join(dir, "called")), false);
      assert.equal(readFileSync(join(dir, "keep"), "utf8"), "existing backup");
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("hostile archives are rejected before touching a keychain", { skip: process.platform === "win32" }, () => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-sign-archive-"));
  try {
    writeFileSync(join(dir, "security"), '#!/bin/sh\ntouch "$CALLED"\nexit 1\n');
    chmodSync(join(dir, "security"), 0o755);
    for (const kind of ["traversal", "symlink", "two-apps"]) {
      const generated = spawnSync("python3", ["-c", `import sys, tarfile
with tarfile.open(sys.argv[1], 'w:gz') as archive:
    for name in ({'traversal': ['../escaped'], 'symlink': ['Ibis.app'], 'two-apps': ['One.app', 'Two.app']}[sys.argv[2]]):
        info = tarfile.TarInfo(name)
        info.type = tarfile.SYMTYPE if sys.argv[2] == 'symlink' else tarfile.DIRTYPE
        info.linkname = '../../escaped' if sys.argv[2] == 'symlink' else ''
        archive.addfile(info)
`, join(dir, "Ibis-macOS-Apple-silicon.app.tar.gz"), kind], { encoding: "utf8" });
      assert.equal(generated.status, 0, generated.stderr);
      const result = spawnSync("bash", [script, "sign", dir, "macOS-Apple-silicon", "com.ibiscite.ibis.nightly"], {
        encoding: "utf8", env: { ...process.env, PATH: `${dir}:${process.env.PATH}`, CALLED: join(dir, "called"), MACOS_SIGNING_P12: "unused", MACOS_SIGNING_P12_PASSWORD: "unused", MACOS_SIGNING_LEAF: leaf },
      });
      assert.notEqual(result.status, 0, kind);
      assert.equal(existsSync(join(dir, "called")), false, kind);
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a disk-image copy failure detaches it and preserves both input packages", { skip: process.platform === "win32" }, () => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-sign-cleanup-"));
  const tools = join(dir, "tools");
  const dist = join(dir, "dist");
  try {
    const made = spawnSync("python3", ["-c", `import pathlib, sys, tarfile
root = pathlib.Path(sys.argv[1])
(root / 'tools').mkdir()
(root / 'dist').mkdir()
with tarfile.open(root / 'dist/Ibis-macOS-Apple-silicon.app.tar.gz', 'w:gz') as archive:
    info = tarfile.TarInfo('Ibis.app')
    info.type = tarfile.DIRTYPE
    archive.addfile(info)
`, dir], { encoding: "utf8" });
    assert.equal(made.status, 0, made.stderr);
    const stubs = {
      uuidgen: "echo disposable-keychain-password",
      security: `echo "$1" >> "$SECURITY_LOG"\ncase "$1" in find-certificate) echo 'SHA-1 hash: ${leaf}';; esac`,
      plutil: "echo com.ibiscite.ibis.nightly",
      codesign: `case "$*" in *' -r-'*) echo '${pinned}' >&2;; esac`,
      hdiutil: `echo "$1" >> "$IMAGE_LOG"\ncase "$1" in\nconvert) while [ "$1" != -o ]; do shift; done; touch "$2";;\nresize) [ "$2" != -limits ] || echo '1 1000 2000';;\nattach) while [ "$1" != -mountpoint ]; do shift; done; mkdir -p "$2";;\nesac`,
      ditto: "exit 41",
    };
    for (const [tool, body] of Object.entries(stubs)) {
      writeFileSync(join(tools, tool), `#!/bin/sh\n${body}\n`);
      chmodSync(join(tools, tool), 0o755);
    }
    const archive = join(dist, "Ibis-macOS-Apple-silicon.app.tar.gz");
    const before = readFileSync(archive);
    writeFileSync(join(dist, "Ibis-macOS-Apple-silicon.dmg"), "original disk image");
    const result = spawnSync("bash", [script, "sign", dist, "macOS-Apple-silicon", "com.ibiscite.ibis.nightly"], {
      encoding: "utf8", env: { ...process.env, PATH: `${tools}:${process.env.PATH}`, IMAGE_LOG: join(dir, "images"), SECURITY_LOG: join(dir, "security"), MACOS_SIGNING_P12: "eA==", MACOS_SIGNING_P12_PASSWORD: "unused", MACOS_SIGNING_LEAF: leaf },
    });
    assert.equal(result.status, 41, result.stderr);
    assert.match(readFileSync(join(dir, "images"), "utf8"), /detach/);
    const security = readFileSync(join(dir, "security"), "utf8");
    assert.match(security, /delete-keychain/);
    assert.doesNotMatch(security, /list-keychains/);
    assert.deepEqual(readFileSync(archive), before);
    assert.equal(readFileSync(join(dist, "Ibis-macOS-Apple-silicon.dmg"), "utf8"), "original disk image");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("unconfigured Preview skips the signing environment and retains install gates", () => {
  const workflow = readFileSync(new URL("../.github/workflows/preview.yml", import.meta.url), "utf8");
  const signing = workflow.split("  sign-macos:\n")[1].split("  install:\n")[0];
  assert.match(signing, /if: vars\.MACOS_SIGNING_LEAF != ''/);
  const install = workflow.split("  install:\n")[1].split("  publish:\n")[0];
  assert.match(install, /needs\.build\.result == 'success'/);
  assert.match(install, /needs\.sign-macos\.result == 'success' \|\| needs\.sign-macos\.result == 'skipped'/);
  assert.match(workflow, /publish:[\s\S]*?needs: \[build, install\]/);
  assert.match(workflow, /nightly:[\s\S]*?needs: \[build, install\]/);
});
