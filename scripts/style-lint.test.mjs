import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { lint, rawValues } from "./style-lint.mjs";

test("raw colours and px sizes are found in declarations, not in comments, strings, url() or conditions", () => {
  assert.deepEqual(rawValues(""), []);
  assert.deepEqual(rawValues("a { color: rgb(0 0 0"), ["rgb(0 0 0"], "an unclosed call is still reported");
  assert.deepEqual(rawValues(`
    /* #fff 12px red */
    @media (max-width: 460px) { a { content: "#abc 9px"; background: url("data:image/svg+xml,%23fff' width='12px'"); } }
    @supports (backdrop-filter: blur(1px)) { a { border: 1px solid var(--line); margin: 0 -1px; outline-offset: 0px; } }
    a { color: transparent; fill: currentColor; background: Canvas; border-color: CanvasText; --name-red: var(--white-ish); }
  `), []);
  assert.deepEqual(rawValues(`
    a { color: #FFF; background: light-dark(RGB(0 0 0 / 5%), hsl(0 0% 50%)); border-color: White; }
    b { width: calc(100vw - 16px); padding: var(--space, 4px) .5px 2.5PX; --menu-h: 28px; transform: translateY(-8px); }
  `), ["#fff", "rgb(0 0 0 / 5%)", "hsl(0 0% 50%)", "white", "16px", "4px", ".5px", "2.5px", "28px", "-8px"]);
});

test("comment marks in strings and quotes in comments do not hide declarations", () => {
  assert.deepEqual(rawValues(`a { content: "/*"; color: red; } b { content: "*/"; width: 24px; }`), ["red", "24px"]);
  assert.deepEqual(rawValues(`/* it's */ a { color: red; } /* " */ b { width: 24px; content: '\\'/*'; fill: blue; }`), ["red", "24px", "blue"]);
});

test("exponent lengths are px lengths, and zero or one stays a hairline", () => {
  assert.deepEqual(rawValues("a { width: 1e1px; margin: 1E-1px .5e+2px -1e1px +2px; }"), ["1e1px", "1e-1px", ".5e+2px", "-1e1px", "+2px"]);
  assert.deepEqual(rawValues("a { border-width: 1e0px; margin: 0e5px -1e0px 10e-1px; }"), []);
});

test("a new raw value fails with a pointer to the tokens; an allowlisted one passes", (t) => {
  const repo = mkdtempSync(join(tmpdir(), "ibis-style-lint-"));
  t.after(() => rmSync(repo, { recursive: true, force: true }));
  const src = join(repo, "apps/desktop/src");
  mkdirSync(join(src, "ui"), { recursive: true });
  writeFileSync(join(src, "tokens.css"), ":root { --gap: 8px; --accent-ish: #c23a1c; }");
  writeFileSync(join(src, "ui/Menu.css"), ".menu { gap: var(--gap); width: 264px; }");
  const allowlist = { "apps/desktop/src/ui/Menu.css": { reason: "#1033", values: { "264px": 1 } } };
  assert.deepEqual(lint(repo, allowlist), []);

  writeFileSync(join(src, "ui/Menu.css"), ".menu { gap: var(--gap); width: 264px; } .item { height: 264px; color: #333; }");
  const problems = lint(repo, allowlist);
  assert.equal(problems.length, 2);
  assert.match(problems.join("\n"), /ui\/Menu\.css: raw #333; use a token from apps\/desktop\/src\/tokens\.css, or add one there \(rules: docs\/design\.md\)/);
  assert.match(problems.join("\n"), /ui\/Menu\.css: raw 264px;/);

  writeFileSync(join(src, "ui/Menu.css"), ".menu { gap: var(--gap); }");
  assert.deepEqual(lint(repo, allowlist), ["apps/desktop/src/ui/Menu.css: 264px is allowed 1 time(s) but used less; lower its count in scripts/style-lint.mjs."]);

  mkdirSync(join(repo, "website"));
  writeFileSync(join(repo, "website/site.css"), "body { color: navy; }");
  mkdirSync(join(repo, "website/dist/assets"), { recursive: true });
  writeFileSync(join(repo, "website/dist/assets/tokens.css"), ":root { --x: #c23a1c; --y: 13px; }");
  writeFileSync(join(repo, "website/distant.css"), "a { width: 2px; }");
  assert.deepEqual(lint(repo, {}).map(problem => problem.split(":")[0]), ["website/distant.css", "website/site.css"], "website/dist is build output");
});
