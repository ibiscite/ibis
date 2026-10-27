import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");

test("Release Please bumps every workspace crate in Cargo.lock", () => {
  const version = read("Cargo.toml").match(/\[workspace\.package\][^[]*?\nversion = "([^"]+)"/)[1];
  const crates = [...read("Cargo.lock").matchAll(/\[\[package\]\]\nname = "(ibis[^"]*)"\nversion = "([^"]+)"\n(?!source)/g)]
    .filter(([, , locked]) => locked === version)
    .map(([, name]) => name)
    .sort();
  const files = JSON.parse(read("release-please-config.json")).packages["."]["extra-files"];
  const bumped = files
    .filter((file) => file.path === "Cargo.lock")
    .map((file) => file.jsonpath.match(/name\.value=='([^']+)'/)[1])
    .sort();
  assert.ok(crates.length > 0, "workspace crates are found in Cargo.lock");
  assert.deepEqual(bumped, crates, "a release must bump every workspace crate or `--locked` builds fail");
});
