import { readFile } from "node:fs/promises";
import { stripTypeScriptTypes } from "node:module";

export const sharedModules = ["platform", "keys", "icon-art"];

/** Keep the desktop modules' relative imports resolvable in the public build. */
export async function sharedSource(name) {
  return stripTypeScriptTypes(await readFile(new URL(`../apps/desktop/src/${name}.ts`, import.meta.url), "utf8"))
    .replaceAll(/from "(\.\/[\w-]+)"/g, 'from "$1.js"');
}

export async function sharedArt() {
  return (await import(`data:text/javascript,${encodeURIComponent(await sharedSource("icon-art"))}`)).art;
}

/** Load the same key module at build time, without writing generated files. */
export async function sharedKeys() {
  const url = source => `data:text/javascript,${encodeURIComponent(source)}`;
  const platform = url(await sharedSource("platform"));
  return import(url((await sharedSource("keys")).replace('"./platform.js"', JSON.stringify(platform))));
}
