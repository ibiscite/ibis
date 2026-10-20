import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { forbiddenUiImports } from "./ui-imports.mjs";

test("React Aria is confined to ui, including re-exports, subpaths and dynamic imports", (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-ui-imports-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, "ui"));
  mkdirSync(join(root, "features"));
  const write = (path, source) => writeFileSync(join(root, path), source);
  write("ui/Menu.tsx", 'import {Menu} from "react-aria-components/Menu";');
  write("features/allowed.ts", '// import "react-aria"\nconst example = "react-aria"; import {Menu} from "../ui/Menu";');
  assert.deepEqual(forbiddenUiImports(root), []);
  for (const source of [
    'import {Menu} from "react-aria-components";',
    'import type {MenuProps} from "react-aria-components/Menu";',
    'export {useMenu} from "react-aria";',
    'export * from "react-aria/Menu";',
    'const menu = import("react-aria-components/Menu");',
    'const menu = require("react-aria");',
    'import menu = require("react-aria");',
  ]) {
    write("features/leak.tsx", source);
    assert.deepEqual(forbiddenUiImports(root), ["features/leak.tsx"]);
  }
  rmSync(join(root, "features/leak.tsx"));
  write("ui-alike.ts", 'import "react-aria";');
  assert.deepEqual(forbiddenUiImports(root), ["ui-alike.ts"]);
});

test("comments and line breaks do not hide dependency imports", (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-ui-spacing-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const source of [
    'import /* keep boundary */ {Menu}\nfrom /* vendor */ "react-aria-components";',
    'const promise = import(\n/* vendor */ "react-aria/Menu"\n);',
    'export\n* /* vendor */ from\n"react-aria-components";',
  ]) {
    writeFileSync(join(root, "Feature.tsx"), source);
    assert.deepEqual(forbiddenUiImports(root), ["Feature.tsx"]);
  }
  writeFileSync(join(root, "Feature.tsx"), '/* import "react-aria" */\nconst help = "import \\\"react-aria\\\"";\nconst template = `from "react-aria-components"`;');
  assert.deepEqual(forbiddenUiImports(root), []);
});

test("JavaScript syntax cannot hide imports or turn regular expressions into imports", (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-ui-syntax-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const source of [
    'const menu = import(("react-aria-components"));',
    'const text = `${import("react-aria-components")}`;',
    'type Menu = import("react-aria-components").MenuProps;',
    'const menu = import(`react-aria-components/Menu`);',
    'import "react\\u002daria-components";',
  ]) {
    writeFileSync(join(root, "Feature.tsx"), source);
    assert.deepEqual(forbiddenUiImports(root), ["Feature.tsx"]);
  }
  writeFileSync(join(root, "Feature.tsx"), 'const pattern = /from "react-aria-components"/;');
  assert.deepEqual(forbiddenUiImports(root), []);
});
