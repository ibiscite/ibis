import assert from "node:assert/strict";
import { test } from "node:test";
import { catchProblems, catchBaselineErrors } from "./check-swallowed-errors.mjs";
const check = (source, path = "apps/desktop/src/example.ts") => catchProblems(path, source);

test("empty and explicitly ignored catches require an explanation", () => {
  for (const source of ["try {} catch {}", "try {} catch { ; }", "try {} catch (_) { recover(); }", "p.catch(() => {});", "p.catch(function () {});", "p['catch'](() => {});", "p.catch(() => undefined);", "p.catch(() => void 0);"]) {
    assert.equal(check(source).length, 1, source);
  }
  assert.deepEqual(check("try {} catch (error) { report(error); }"), []);
});

test("comments must explain on the catch's own line", () => {
  for (const source of ["try {} catch {} // optional cleanup has no caller", "p.catch(() => { /* already reported by the caller */ });", "try {} catch (_) { recover(); } // no exception detail is needed"]) assert.deepEqual(check(source), [], source);
  for (const source of ["// optional cleanup\ntry {} catch {}", "try {} catch {\n// optional cleanup\n}", "try {} catch {} // ...", "try {} catch {} // TODO", 'const note = "// explanation"; try {} catch {}']) assert.equal(check(source).length, 1, source);
});

test("AST parsing ignores comments and literals while retaining template expressions", () => {
  assert.deepEqual(check('const text = "catch (_) {}"; // p.catch(() => {})\nconst template = `catch {}`;'), []);
  assert.equal(check('const template = `${p.catch(() => {})}`;').length, 1);
  assert.equal(check("const view = <div title='catch {}'/>; try {} catch {}", "apps/desktop/src/example.tsx").length, 1);
  assert.throws(() => check("try {"), /fix.*syntax/i);
});

test("empty, huge, renamed, Unicode, Windows and CRLF inputs remain precise", () => {
  assert.deepEqual(check(""), []);
  assert.deepEqual(check("try {} catch {\r\n}"), check("try {} catch {\n}"));
  const source = "// harmless\n".repeat(100000) + "try {} catch {}";
  assert.equal(check(source).length, 1);
  assert.deepEqual(check("try {} catch {}\r\n", "apps\\desktop\\src\\資料.ts"), check("try {} catch {}\r\n", "apps/desktop/src/資料.ts"));
  assert.notDeepEqual(check("try {} catch {}", "apps/desktop/src/old.ts"), check("try {} catch {}", "apps/desktop/src/new.ts"));
});

test("existing-case allowances cannot hide a changed or additional catch", () => {
  const problems = check("try {} catch {}");
  const baseline = Object.fromEntries(problems.map(({ key }) => [key, 1]));
  assert.deepEqual(catchBaselineErrors(problems, baseline), []);
  assert.ok(catchBaselineErrors([...problems, ...problems], baseline).length);
  assert.ok(catchBaselineErrors(check("try {} catch (_) {}"), baseline).length);
  assert.ok(catchBaselineErrors([], baseline).length);
});
