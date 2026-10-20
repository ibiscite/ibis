// ADR 0021: feature code uses Ibis wrappers; only ui imports React Aria.
import { readdirSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
// Reuse Vite's pinned JS/TS parser without adding a dependency or a new linter.
const desktopRequire = createRequire(new URL("../apps/desktop/package.json", import.meta.url));
const viteRequire = createRequire(desktopRequire.resolve("vite"));
const { parseSync } = await import(viteRequire.resolve("rolldown/utils"));

function literal(node) {
  while (node?.type === "ParenthesizedExpression") node = node.expression;
  if (node?.type === "Literal" && typeof node.value === "string") return node.value;
  if (node?.type === "TemplateLiteral" && node.expressions.length === 0) return node.quasis[0].value.cooked;
}

export function forbiddenUiImports(root) {
  const forbidden = [];
  function scan(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const file = join(directory, entry.name);
      if (entry.isDirectory()) { if (file !== join(root, "ui")) scan(file); continue; }
      if (!/\.[cm]?[jt]sx?$/.test(entry.name)) continue;
      const result = parseSync(file, readFileSync(file, "utf8"));
      if (result.errors.length) throw new Error("UI import check could not parse a source file.");
      let found = false;
      function visit(node) {
        if (!node || typeof node !== "object") return;
        const specifier = ["ImportDeclaration", "ExportNamedDeclaration", "ExportAllDeclaration", "ImportExpression", "TSImportType"].includes(node.type) ? node.source
          : node.type === "TSExternalModuleReference" ? node.expression
          : node.type === "CallExpression" && node.callee.type === "Identifier" && node.callee.name === "require" ? node.arguments[0] : undefined;
        if (/^react-aria(?:-components)?(?:\/|$)/.test(literal(specifier) ?? "")) found = true;
        for (const value of Object.values(node)) {
          if (Array.isArray(value)) value.forEach(visit);
          else if (value?.type) visit(value);
        }
      }
      visit(result.program);
      if (found) forbidden.push(relative(root, file).split(sep).join("/"));
    }
  }
  scan(root);
  return forbidden.sort();
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const forbidden = forbiddenUiImports("apps/desktop/src");
  if (forbidden.length) {
    console.error(`React Aria imports must stay in src/ui/: ${forbidden.join(", ")}`);
    process.exitCode = 1;
  }
}
