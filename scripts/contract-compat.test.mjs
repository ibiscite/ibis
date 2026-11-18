import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { test } from "node:test";
import { report } from "./contract-compat.mjs";
import { releaseNotes } from "./release-notes.mjs";

const ENGINE = "fixtures/engine-contract.schema.json";
const METHODS = "fixtures/cli-schema.json";
const TOOLS = "fixtures/mcp-tools.json";
const CLI = "fixtures/cli-surface.json";
const DOCS = "docs/cli.md";
const read = (files) => (path) => typeof files[path] === "string" ? files[path] : JSON.stringify(files[path]);

function fixture(overrides = {}) {
  return {
    [ENGINE]: {},
    [METHODS]: { ibis: 1, ok: true, data: [] },
    [TOOLS]: [],
    [CLI]: { command: { arguments: {}, subcommands: {} }, errors: [] },
    ...overrides,
  };
}

function compare(before, after, options) {
  return report("v0.3.10", read(before), read(after), options);
}

test("reports exact defaults next to a ref or enum, and a changed map value definition", () => {
  const schema = {
    $defs: {
      Form: { enum: ["normal", "author"] },
      Row: { type: "string" },
      Output: { type: "object", additionalProperties: { $ref: "#/$defs/Row" } },
    },
    properties: {
      first: { $ref: "#/$defs/Form", default: "normal" },
      second: { enum: ["normal", "author"], default: "normal" },
    },
  };
  const before = fixture({ [ENGINE]: schema });
  const after = structuredClone(before);
  after[ENGINE].$defs.Row.type = ["string", "null"];
  after[ENGINE].properties.first.default = "author";
  after[ENGINE].properties.second.default = "author";
  assert.match(compare(before, after), /0 added, 0 removed, 3 changed/);
  assert.match(compare(before, after), /Engine schema \/\$defs\/Row\/type  "string" → \["string","null"\]/);
  assert.match(compare(before, after), /Engine schema \/properties\/first\/default  "normal" → "author"/);
  assert.match(compare(before, after), /Engine schema \/properties\/second\/default  "normal" → "author"/);
});

test("keeps unknown keywords and concurrent differences in the published notes", () => {
  const before = fixture({ [METHODS]: { data: [{ method: "show", result: { maximum: 10, "x-old": true } }] } });
  const after = fixture({ [METHODS]: { data: [{ method: "show", result: { maximum: 11, "x-future/option": { enabled: true } } }] } });
  const notes = releaseNotes("v0.3.11", "", "example/ibis", "", compare(before, after, { includeUnchanged: false }));
  assert.match(notes, /\*\*Contract changes\*\*[\s\S]*1 added, 1 removed, 1 changed/);
  assert.match(notes, /\*\*Added\*\*[\s\S]*Method schemas \/data\/show\/result\/x-future~1option  \{"enabled":true\}/);
  assert.match(notes, /\*\*Removed\*\*[\s\S]*Method schemas \/data\/show\/result\/x-old  true/);
  assert.match(notes, /\*\*Changed\*\*[\s\S]*Method schemas \/data\/show\/result\/maximum  10 → 11/);
  assert.equal(compare(before, before, { includeUnchanged: false }), "");
});

test("reports exact large numeric constraints and keeps positional array order", () => {
  const before = fixture({ [ENGINE]: '{"maximum":9223372036854775806,"prefixItems":[{"type":"number"},{"type":"string"}]}' });
  const after = fixture({ [ENGINE]: '{"maximum":9223372036854775807,"prefixItems":[{"type":"string"},{"type":"number"}]}' });
  assert.match(compare(before, after), /Engine schema \/maximum  9223372036854775806 → 9223372036854775807/);
  assert.match(compare(before, after), /Engine schema \/prefixItems\/0\/type  "number" → "string"/);
  assert.match(compare(before, after), /Engine schema \/prefixItems\/1\/type  "string" → "number"/);
});

test("reports Clap library flags, short aliases, positional arity and requiredness", () => {
  const cli = JSON.parse(readFileSync(new URL(`../${CLI}`, import.meta.url), "utf8"));
  const before = fixture({ [CLI]: cli });
  const after = structuredClone(before);
  const command = after[CLI].command;
  delete command.arguments.file;
  command.subcommands.agents.subcommands.setup.arguments.yes.short = "a";
  const query = command.subcommands.search.arguments.query;
  query.arity.max = 2;
  query.required = false;
  command.subcommands.search.arguments.extra = { arity: { min: 1, max: 1 }, index: 2, required: true };
  const text = compare(before, after);
  assert.match(text, /\*\*Added\*\*[\s\S]*CLI \/command\/subcommands\/search\/arguments\/extra  \{"arity":\{"min":1,"max":1\},"index":2,"required":true\}/);
  assert.match(text, /\*\*Removed\*\*[\s\S]*CLI \/command\/arguments\/file  .*"long":"file"/);
  assert.match(text, /\*\*Changed\*\*[\s\S]*CLI \/command\/subcommands\/agents\/subcommands\/setup\/arguments\/yes\/short  "y" → "a"/);
  assert.match(text, /CLI \/command\/subcommands\/search\/arguments\/query\/arity\/max  1 → 2/);
  assert.match(text, /CLI \/command\/subcommands\/search\/arguments\/query\/required  true → false/);
});

test("reports MCP approval hints alongside schema changes", () => {
  const before = fixture({ [TOOLS]: [{ name: "show", annotations: { readOnlyHint: true }, inputSchema: { type: "object" } }] });
  const after = structuredClone(before);
  after[TOOLS][0].annotations.readOnlyHint = false;
  after[TOOLS][0].inputSchema.additionalProperties = false;
  assert.match(compare(before, after), /MCP tools \/show\/annotations\/readOnlyHint  true → false/);
  assert.match(compare(before, after), /MCP tools \/show\/inputSchema\/additionalProperties  false/);
});

const help = (rows, exit = 4) => `<!-- Generated by node scripts/gen.mjs from the command line's own options. -->
**\`ibis show\`**: Show one reference
| Option | Does |
| --- | --- |
${rows}
<!-- End of generated options. -->
<!-- gen:errors -->
| ${exit} | \`not-found\` |
<!-- /gen:errors -->`;

test("earlier releases report recorded CLI changes and identify the unrecorded details", () => {
  const before = fixture({ [CLI]: undefined, [DOCS]: help("| `KEY` | A key |\n| `--old N` | Old option |") });
  const after = fixture({ [DOCS]: help("| `KEY…` | Keys |", 3) });
  const text = compare(before, after);
  assert.match(text, /Detailed CLI flags, aliases, arity and requiredness need a published snapshot/);
  assert.match(text, /CLI \/commands\/show\/arguments\/KEY\/spelling  "KEY" → "KEY…"/);
  assert.match(text, /\*\*Removed\*\*[\s\S]*CLI \/commands\/show\/arguments\/--old/);
  assert.match(text, /CLI \/errors\/not-found  4 → 3/);
  assert.doesNotMatch(text, /arguments\/file/);
});

test("the hosted report uses GitHub's published release despite newer tags", { skip: process.platform === "win32" }, (t) => {
  const root = mkdtempSync(join(tmpdir(), "ibis-contract-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const dir of ["scripts", "fixtures", "bin"]) mkdirSync(join(root, dir));
  const script = join("scripts", "contract-compat.mjs");
  copyFileSync(new URL("./contract-compat.mjs", import.meta.url), join(root, script));
  const git = (...args) => execFileSync("git", args, { cwd: root, stdio: "pipe" });
  git("init", "-q");
  git("remote", "add", "origin", root);
  for (const [path, value] of Object.entries(fixture({ [ENGINE]: { maximum: 10 } }))) {
    writeFileSync(join(root, path), JSON.stringify(value));
  }
  git("add", "fixtures");
  const commit = () => git("-c", "user.name=Invented Tester", "-c", "user.email=tester@example.org", "-c", "commit.gpgsign=false", "commit", "-qm", "Invented contract");
  commit();
  git("tag", "v0.3.10");
  writeFileSync(join(root, ENGINE), '{"maximum":11}');
  git("add", ENGINE);
  commit();
  for (const tag of ["v0.3.11", "v0.4.0-rc.1", "v0.5.0"]) git("tag", tag);
  writeFileSync(join(root, "bin", "gh"), `#!/usr/bin/env node
    const expected = ['api', 'repos/example/ibis/releases/latest', '--jq', 'select(.draft == false and .prerelease == false) | .tag_name'];
    if (JSON.stringify(process.argv.slice(2)) !== JSON.stringify(expected)) process.exit(2);
    if (process.env.RELEASE_LOOKUP_FAIL) process.exit(1);
    console.log('v0.3.10');
  `, { mode: 0o755 });
  const env = { ...process.env, PATH: `${join(root, "bin")}${delimiter}${process.env.PATH}`, GITHUB_REPOSITORY: "example/ibis" };
  const run = (args, extra = {}) => spawnSync(process.execPath, [script, ...args], { cwd: root, env: { ...env, ...extra }, encoding: "utf8" });
  const published = run(["--published"]);
  assert.equal(published.status, 0, published.stderr);
  assert.match(published.stdout, /Compared with ` v0.3.10 `/);
  assert.match(published.stdout, /Engine schema \/maximum  10 → 11/);
  assert.equal(run(["--base", "refs/tags/v0.3.10"], { RELEASE_LOOKUP_FAIL: "1" }).stdout, published.stdout);
  const failed = run(["--published"], { RELEASE_LOOKUP_FAIL: "1" });
  assert.equal(failed.stdout, "");
  assert.match(failed.stderr, /Contract report skipped/);
});
