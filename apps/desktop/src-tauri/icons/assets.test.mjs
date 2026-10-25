// @vitest-environment node
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "vitest";

const root = fileURLToPath(new URL("./", import.meta.url));
const read = (name) => readFileSync(root + name);
const config = JSON.parse(read("../tauri.conf.json"));
const pngSize = (bytes) => {
  expect(bytes.subarray(0, 8).toString("hex")).toBe("89504e470d0a1a0a");
  expect(bytes.subarray(12, 16).toString()).toBe("IHDR");
  return [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
};

test("desktop art uses the final brand kit rather than the retired B/G1 design", () => {
  expect(read("source/full.svg").toString()).toBe(
    readFileSync(new URL("../../../../brand/exports/ibis-app-icon.svg", import.meta.url), "utf8"));
  expect(read("source/windows.svg").toString()).toBe(
    readFileSync(new URL("../../../../brand/exports/ibis-mark.svg", import.meta.url), "utf8"));
});

test("bundle icons have valid shared depth, small-size art and declared sizes", () => {
  const svg = read("source/full.svg").toString();
  expect(svg).toContain('viewBox="0 0 1024 1024"');
  for (const name of ["full.svg", "macos.svg", "windows.svg", "gnome.svg"]) {
    const source = read(`source/${name}`).toString();
    expect(source.match(/<filter /g)).toHaveLength(1);
    expect(source).toContain('mask="url(#');
    const ids = [...source.matchAll(/ id="([^"]+)"/g)].map((match) => match[1]);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) expect(id).toMatch(/^[A-Za-z_][\w.-]*$/);
  }
  const sizes = { "32x32.png": 32, "128x128.png": 128, "128x128@2x.png": 256 };
  expect(config.bundle.icon).toEqual([
    ...Object.keys(sizes).map((name) => `icons/${name}`), "icons/icon.icns", "icons/icon.ico",
    "icons/linux/512x512/io.github.ibiscite.ibis.png",
  ]);
  for (const [name, size] of Object.entries({ ...sizes, "source.png": 1024, "icon.png": 1024 })) {
    expect(pngSize(read(name))).toEqual([size, size]);
  }
  for (const name of ["small.svg", "macos-small.svg", "windows-small.svg"]) {
    const small = read(`source/${name}`).toString();
    expect(small).toContain('stroke-width="10.5"');
    expect(small).not.toMatch(/<filter|<mask/);
  }
  const symbolic = read("source/io.github.ibiscite.ibis-symbolic.svg").toString();
  expect(symbolic).toContain('width="16" height="16"');
  expect(symbolic).toContain('stroke="currentColor"');
  expect(symbolic).not.toMatch(/gradient|<filter|#[0-9a-f]{3,6}\b/i);
});

test("both Linux packages install named icons and macOS ships only its reviewed ICNS", () => {
  for (const platform of ["deb", "appimage"]) {
    const files = config.bundle.linux[platform].files;
    for (const size of [128, 256, 512]) {
      const source = files[`/usr/share/icons/hicolor/${size}x${size}/apps/io.github.ibiscite.ibis.png`];
      expect(pngSize(read(source.replace("icons/", "")))).toEqual([size, size]);
    }
    expect(files["/usr/share/icons/hicolor/symbolic/apps/io.github.ibiscite.ibis-symbolic.svg"])
      .toBe("icons/source/io.github.ibiscite.ibis-symbolic.svg");
  }
  expect(read(config.bundle.linux.deb.desktopTemplate.replace("icons/", "")).toString())
    .toContain("Icon=io.github.ibiscite.ibis");
  expect(config.bundle.icon).not.toContain("icons/source/Ibis.icon");
});

test("Windows and macOS containers hold actual images at every required size", () => {
  const ico = read("icon.ico");
  expect(ico.subarray(0, 4).toString("hex")).toBe("00000100");
  const sizes = [];
  for (let index = 0; index < ico.readUInt16LE(4); index++) {
    const offset = 6 + index * 16;
    const size = ico[offset] || 256;
    expect(ico[offset + 1] || 256).toBe(size);
    const start = ico.readUInt32LE(offset + 12);
    const image = ico.subarray(start, start + ico.readUInt32LE(offset + 8));
    expect(pngSize(image)).toEqual([size, size]);
    sizes.push(size);
  }
  expect(sizes).toEqual([16, 24, 32, 48, 256]);
  const icns = read("icon.icns");
  expect(icns.subarray(0, 4).toString()).toBe("icns");
  expect(icns.readUInt32BE(4)).toBe(icns.length);
  const entries = {};
  for (let offset = 8; offset < icns.length;) {
    const length = icns.readUInt32BE(offset + 4);
    expect(length).toBeGreaterThan(8);
    entries[icns.subarray(offset, offset + 4).toString()] = pngSize(icns.subarray(offset + 8, offset + length));
    offset += length;
  }
  expect(entries).toMatchObject({ icp4: [16, 16], icp5: [32, 32], ic07: [128, 128], ic10: [1024, 1024] });
});
