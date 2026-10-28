// The requirement check in scripts/macos-sign.sh refuses any app that is not
// the expected Ibis signed with the pinned certificate (#1242). macOS tools
// are stubbed; the shell under test is the real script.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("./macos-sign.sh", import.meta.url));
const leaf = "0123456789abcdef0123456789abcdef01234567";

function check({ bundle, designated, status = 0, displayStatus = 0 }, identifier = "com.ibiscite.ibis.nightly") {
  const dir = mkdtempSync(join(tmpdir(), "ibis-sign-"));
  try {
    writeFileSync(join(dir, "plutil"), `#!/bin/sh\necho '${bundle}'\n`);
    // codesign -d -r- writes the requirement to stderr, after other lines.
    writeFileSync(join(dir, "codesign"), `#!/bin/sh\ncase "$*" in *--verify*) exit ${status};; esac\necho 'Executable=/x' >&2\necho '${designated}' >&2\nexit ${displayStatus}\n`);
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
  assert.match(result.stderr, /codesign verify.*failed/);
});

test("a failed codesign display reports its step despite captured stderr", { skip: process.platform === "win32" }, () => {
  const result = check({ bundle: "com.ibiscite.ibis.nightly", designated: "display error", displayStatus: 42 });
  assert.equal(result.status, 42, result.stderr);
  assert.match(result.stderr, /codesign display.*failed/);
  assert.match(result.stderr, /display error/);
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

const originalKeychains = [
  "/example/Login Keychain.keychain-db",
  '/example/Build "quoted" $dollar `literal` \\ path.keychain-db',
];

function signCheck({ fail = "", keychains = originalKeychains, found = leaf, autoAdd = false } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "ibis-sign-cleanup-"));
  const tools = join(dir, "tools");
  const dist = join(dir, "dist");
  try {
    mkdirSync(tools);
    mkdirSync(dist);
    const archive = join(dist, "Ibis-macOS-Apple-silicon.app.tar.gz");
    const made = spawnSync("python3", ["-c", `import sys, tarfile
with tarfile.open(sys.argv[1], 'w:gz') as archive:
    info = tarfile.TarInfo('Ibis.app')
    info.type = tarfile.DIRTYPE
    archive.addfile(info)
`, archive], { encoding: "utf8" });
    assert.equal(made.status, 0, made.stderr);
    writeFileSync(join(dir, "search-list"), keychains.map((path) => `${path}\n`).join(""));
    const failure = `
fail() {
  if [ "$FAIL_STEP" = "$1" ]; then
    echo 'error: The specified item could not be found in the keychain.' >&2
    exit 44
  fi
}
`;
    const stubs = {
      uuidgen: "echo disposable-keychain-password",
      security: `${failure}
step="security $1"
case "$1" in
  list-keychains)
    [ "$2" = -d ] && [ "$3" = user ] || exit 90
    if [ "$#" -eq 3 ]; then
      echo 'security save search list' >> "$CALLS"
      fail 'security save search list'
      while IFS= read -r path; do printf '    "%s"\n' "$path"; done < "$SEARCH_LIST"
      exit 0
    fi
    [ "$4" = -s ] || exit 91
    shift 4
    step='security restore search list'
    [ "\${1:-}" != "$(cat "$KEYCHAIN_FILE")" ] || step='security add search list'
    : > "$SEARCH_LIST"
    for path do printf '%s\n' "$path" >> "$SEARCH_LIST"; done
    # Simulate a setter that changes state before reporting failure.
    [ "$step" != 'security add search list' ] || cp "$SEARCH_LIST" "$ADDED_LIST"
    ;;
  create-keychain)
    for path do :; done
    printf '%s\n' "$path" > "$KEYCHAIN_FILE"
    # Some macOS versions add a newly created keychain automatically.
    [ "$AUTO_ADD" != 1 ] || printf '%s\n' "$path" >> "$SEARCH_LIST"
    ;;
  delete-keychain)
    while IFS= read -r path; do
      [ "$path" = "$2" ] || printf '%s\n' "$path"
    done < "$SEARCH_LIST" > "$SEARCH_LIST.next"
    mv "$SEARCH_LIST.next" "$SEARCH_LIST"
    ;;
  find-certificate) printf 'SHA-1 hash: %s\n' "$FOUND_LEAF" ;;
esac
echo "$step" >> "$CALLS"
fail "$step"`,
      plutil: "echo com.ibiscite.ibis.nightly",
      codesign: `${failure}
case "$1" in
  --force)
    step='codesign sign'
    searchable=0
    while IFS= read -r path; do
      [ "$path" != "$(cat "$KEYCHAIN_FILE")" ] || searchable=1
    done < "$SEARCH_LIST"
    # --keychain alone must not make this stub succeed.
    [ "$searchable" = 1 ] || { echo 'identity is outside the search list' >&2; exit 45; }
    ;;
  --verify) step='codesign verify' ;;
  -d) step='codesign display'; echo '${pinned}' >&2 ;;
  *) exit 92 ;;
esac
echo "$step" >> "$CALLS"
fail "$step"`,
      hdiutil: `echo "$1" >> "$IMAGE_LOG"\ncase "$1" in\nconvert) while [ "$1" != -o ]; do shift; done; touch "$2";;\nresize) [ "$2" != -limits ] || echo '1 1000 2000';;\nattach) while [ "$1" != -mountpoint ]; do shift; done; mkdir -p "$2";;\nesac`,
      ditto: '[ "$FAIL_STEP" != ditto ] || exit 41',
    };
    for (const [tool, body] of Object.entries(stubs)) {
      writeFileSync(join(tools, tool), `#!/bin/sh\n${body}\n`);
      chmodSync(join(tools, tool), 0o755);
    }
    const before = readFileSync(archive);
    writeFileSync(join(dist, "Ibis-macOS-Apple-silicon.dmg"), "original disk image");
    const result = spawnSync("bash", [script, "sign", dist, "macOS-Apple-silicon", "com.ibiscite.ibis.nightly"], {
      encoding: "utf8", timeout: 15000,
      env: {
        ...process.env, PATH: `${tools}:${process.env.PATH}`, IMAGE_LOG: join(dir, "images"), CALLS: join(dir, "calls"),
        SEARCH_LIST: join(dir, "search-list"), ADDED_LIST: join(dir, "added-list"), KEYCHAIN_FILE: join(dir, "keychain"),
        FAIL_STEP: fail, FOUND_LEAF: found, AUTO_ADD: autoAdd ? "1" : "0",
        MACOS_SIGNING_P12: "eA==", MACOS_SIGNING_P12_PASSWORD: "fixture-p12-password", MACOS_SIGNING_LEAF: leaf,
      },
    });
    const read = (name) => existsSync(join(dir, name)) ? readFileSync(join(dir, name), "utf8") : "";
    const paths = (name) => read(name).split("\n").filter(Boolean);
    const keychain = read("keychain").trim();
    assert.doesNotMatch(result.stdout + result.stderr, /fixture-p12-password|disposable-keychain-password|eA==/);
    return {
      ...result, calls: paths("calls"), keychains: paths("search-list"), added: paths("added-list"),
      keychain, workRemains: keychain ? existsSync(join(keychain, "..")) : false, images: read("images"),
      archivePreserved: readFileSync(archive).equals(before), dmg: readFileSync(join(dist, "Ibis-macOS-Apple-silicon.dmg"), "utf8"),
    };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("signing adds its keychain and restores the exact original search list", { skip: process.platform === "win32" }, () => {
  for (const keychains of [originalKeychains, []]) {
    for (const autoAdd of [false, true]) {
      const result = signCheck({ keychains, autoAdd });
      assert.equal(result.status, 0, result.stderr);
      assert.deepEqual(result.added, [result.keychain, ...keychains]);
      assert.deepEqual(result.keychains, keychains);
      assert.equal(result.calls[0], "security save search list");
      assert.ok(result.calls.indexOf("security add search list") < result.calls.indexOf("codesign sign"));
      assert.ok(result.calls.indexOf("security delete-keychain") < result.calls.indexOf("security restore search list"));
      assert.equal(result.workRemains, false);
    }
  }
});

test("security and codesign failures identify the step and restore the search list", { skip: process.platform === "win32" }, () => {
  for (const fail of [
    "security save search list", "security create-keychain", "security add search list",
    "security set-keychain-settings", "security unlock-keychain", "security import",
    "security set-key-partition-list", "security find-certificate", "codesign sign",
    "codesign verify", "codesign display", "security delete-keychain", "security restore search list",
  ]) {
    const result = signCheck({ fail, autoAdd: true });
    assert.notEqual(result.status, 0, fail);
    assert.ok(result.stderr.includes(`${fail} failed`), `${fail}: ${result.stderr}`);
    assert.deepEqual(result.keychains, originalKeychains, fail);
    if (fail !== "security save search list") {
      assert.ok(result.calls.includes("security delete-keychain"), fail);
      assert.ok(result.calls.includes("security restore search list"), fail);
    }
    if (fail === "codesign sign") {
      assert.deepEqual(result.added, [result.keychain, ...originalKeychains]);
      assert.equal(result.status, 44, result.stderr);
      assert.equal(result.archivePreserved, true);
      assert.equal(result.dmg, "original disk image");
    }
    assert.equal(result.workRemains, false, fail);
  }
});

test("a wrong or extra certificate is refused before codesign and restores the search list", { skip: process.platform === "win32" }, () => {
  for (const found of ["f".repeat(40), `${leaf}\nSHA-1 hash: ${"f".repeat(40)}`]) {
    const result = signCheck({ found });
    assert.notEqual(result.status, 0, result.stderr);
    assert.match(result.stderr, /exactly the pinned certificate/);
    assert.equal(result.calls.includes("codesign sign"), false);
    assert.deepEqual(result.keychains, originalKeychains);
    assert.ok(result.calls.includes("security delete-keychain"));
    assert.ok(result.calls.includes("security restore search list"));
  }
});

test("a disk-image copy failure detaches it and preserves both input packages", { skip: process.platform === "win32" }, () => {
  const result = signCheck({ fail: "ditto" });
  assert.equal(result.status, 41, result.stderr);
  assert.match(result.images, /detach/);
  assert.ok(result.calls.includes("security delete-keychain"));
  assert.ok(result.calls.includes("security restore search list"));
  assert.deepEqual(result.keychains, originalKeychains);
  assert.equal(result.archivePreserved, true);
  assert.equal(result.dmg, "original disk image");
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
