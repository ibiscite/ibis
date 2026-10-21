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

test("bundle icons are the approved B/G1 sources and have their declared sizes", () => {
  const svg = read("source/full.svg").toString();
  expect(svg).toContain('viewBox="0 0 100 100"');
  expect(svg).toContain('translate(-2.74 -2.83) scale(1.025)');
  expect(svg).toContain("M36 80C35 70 30 62 32 52 33.5 45 36.5 40.5 41 38");
  expect(svg).toContain("M41 25C62 25.5 77 39 84.5 69 75 50 63 41.5 43 43.2z");
  expect(svg).toContain("#e5532d");
  expect(svg).toContain("#c0361a");
  expect(svg).not.toMatch(/filter=|rx=/);
  const sizes = { "32x32.png": 32, "128x128.png": 128, "128x128@2x.png": 256 };
  expect(config.bundle.icon).toEqual([
    ...Object.keys(sizes).map((name) => `icons/${name}`), "icons/icon.icns", "icons/icon.ico",
    "icons/linux/512x512/io.github.ibiscite.ibis.png",
  ]);
  for (const [name, size] of Object.entries({ ...sizes, "source.png": 1024, "icon.png": 1024 })) {
    expect(pngSize(read(name))).toEqual([size, size]);
  }
  const small = read("source/small.svg").toString();
  expect(small).toContain('stroke-width="11"');
  expect(small).not.toContain('r="2.1"');
  for (const name of ["windows.svg", "windows-small.svg"]) {
    expect(read(`source/${name}`).toString()).toContain('viewBox="20 19 70 70"');
  }
  const symbolic = read("source/io.github.ibiscite.ibis-symbolic.svg").toString();
  expect(symbolic).toContain('viewBox="0 0 16 16"');
  expect(symbolic).toContain('stroke-width="1.75"');
  expect(symbolic).not.toMatch(/gradient|r="2.1"/i);
});

test("both Linux packages install named app and symbolic icons, and macOS retains layers", () => {
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
  const layered = JSON.parse(read("source/Ibis.icon/icon.json"));
  // Icon Composer lists groups front to back: the eye must cover the head.
  expect(layered.groups.map((group) => group.name)).toEqual(["Eye", "Bird"]);
  expect(layered["fill-specializations"].find((entry) => entry.appearance === "dark").value)
    .toEqual(layered["fill-specializations"].find((entry) => !entry.appearance).value);
  expect(layered.groups.find((group) => group.name === "Bird").layers[0]["fill-specializations"])
    .toContainEqual({ appearance: "dark", value: { solid: "srgb:1,1,1,1" } });
  for (const group of layered.groups) {
    for (const layer of group.layers) {
      expect(read(`source/Ibis.icon/Assets/${layer["image-name"]}`).toString()).toContain("<svg");
    }
  }
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
