// Explicit opt-in: exercises actual macOS tools with a disposable identity.
// No trust changes, app execution, repository secrets or personal app data.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("./macos-sign.sh", import.meta.url));
const enabled = process.platform === "darwin" && process.env.IBIS_TEST_MACOS_SIGNING === "1";

test("two real signed DMGs and update archives retain the same requirement", { skip: !enabled }, () => {
  const dir = mkdtempSync(join(tmpdir(), "ibis-sign-native-"));
  const run = (command, args, options = {}) => {
    const result = spawnSync(command, args, { encoding: "utf8", timeout: 60000, ...options });
    assert.equal(result.status, 0, `${command}: ${result.stderr}`);
    return result.stdout;
  };
  const keychains = run("security", ["list-keychains", "-d", "user"]);
  try {
    writeFileSync(join(dir, "req.cnf"), `[req]\ndistinguished_name=dn\nx509_extensions=ext\nprompt=no\n[dn]\nCN=Ibis Disposable Test\n[ext]\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=critical,codeSigning\n`);
    run("/usr/bin/openssl", ["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-config", join(dir, "req.cnf"), "-keyout", join(dir, "key.pem"), "-out", join(dir, "cert.pem")]);
    run("/usr/bin/openssl", ["pkcs12", "-export", "-inkey", join(dir, "key.pem"), "-in", join(dir, "cert.pem"), "-out", join(dir, "identity.p12"), "-passout", "pass:disposable-test"]);
    const leaf = run("/usr/bin/openssl", ["x509", "-in", join(dir, "cert.pem"), "-noout", "-fingerprint", "-sha1"]).split("=")[1].replaceAll(":", "").trim().toLowerCase();
    const requirements = [];
    for (const version of [1, 2]) {
      const source = join(dir, `source${version}`);
      const app = join(source, "Ibis Signing Test.app");
      mkdirSync(join(app, "Contents", "MacOS"), { recursive: true });
      writeFileSync(join(dir, "main.c"), `int main(void) { return ${version}; }\n`);
      run("clang", [join(dir, "main.c"), "-o", join(app, "Contents", "MacOS", "test")]);
      writeFileSync(join(app, "Contents", "Info.plist"), `<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.ibiscite.ibis.nightly</string><key>CFBundleExecutable</key><string>test</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleVersion</key><string>${version}</string></dict></plist>`);
      run("codesign", ["--force", "--sign", "-", app]);
      const dist = join(dir, `dist${version}`);
      mkdirSync(dist);
      run("tar", ["-czf", join(dist, "Ibis-macOS-Apple-silicon.app.tar.gz"), "-C", source, "Ibis Signing Test.app"], { env: { ...process.env, COPYFILE_DISABLE: "1" } });
      run("hdiutil", ["create", "-srcfolder", source, "-volname", "Ibis signing test", "-format", "UDZO", join(dist, "Ibis-macOS-Apple-silicon.dmg")]);
      writeFileSync(join(dist, "ibis-macOS-Apple-silicon.tar.gz"), "unchanged CLI fixture");
      const env = { ...process.env, MACOS_SIGNING_P12: readFileSync(join(dir, "identity.p12")).toString("base64"), MACOS_SIGNING_P12_PASSWORD: "disposable-test", MACOS_SIGNING_LEAF: leaf };
      run("bash", [script, "sign", dist, "macOS-Apple-silicon", "com.ibiscite.ibis.nightly"], { env });
      assert.equal(readFileSync(join(dist, "ibis-macOS-Apple-silicon.tar.gz"), "utf8"), "unchanged CLI fixture");
      run("shasum", ["-a", "256", "--check", "SHA256SUMS-aarch64-apple-darwin.txt"], { cwd: dist });
      const extracted = join(dir, `extracted${version}`);
      mkdirSync(extracted);
      run("tar", ["-xzf", join(dist, "Ibis-macOS-Apple-silicon.app.tar.gz"), "-C", extracted]);
      const signed = join(extracted, "Ibis Signing Test.app");
      run("bash", [script, "requirement", signed, "com.ibiscite.ibis.nightly", leaf]);
      // codesign writes its display output to stderr.
      const display = spawnSync("codesign", ["-d", "-r-", signed], { encoding: "utf8" });
      assert.equal(display.status, 0);
      requirements.push((display.stdout + display.stderr).match(/^designated => (.+)$/m)[1]);
      const mount = join(dir, `mount${version}`);
      run("hdiutil", ["attach", "-readonly", "-nobrowse", "-mountpoint", mount, join(dist, "Ibis-macOS-Apple-silicon.dmg")]);
      try {
        run("bash", [script, "requirement", join(mount, "Ibis Signing Test.app"), "com.ibiscite.ibis.nightly", leaf]);
      } finally {
        run("hdiutil", ["detach", mount]);
      }
    }
    assert.equal(requirements[0], requirements[1]);
    assert.equal(run("security", ["list-keychains", "-d", "user"]), keychains);
    assert.ok(!run("hdiutil", ["info"]).includes(dir), "no test disk image remains mounted");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
