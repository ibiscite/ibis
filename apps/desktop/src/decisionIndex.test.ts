// Every decision file is listed in docs/decisions.md, so the index never omits
// one (0057 went missing once). It runs with the interface's tests, which every
// change to prose selects.
import { expect, test, vi } from "vitest";

const { readFileSync, readdirSync } = await vi.importActual<{
  readFileSync: (path: URL, encoding: string) => string;
  readdirSync: (path: URL) => string[];
}>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const docs = new FileURL("../../../docs/", import.meta.url);

test("the decision index links every decision file", () => {
  const index = readFileSync(new FileURL("decisions.md", docs), "utf8");
  const files = readdirSync(new FileURL("decisions/", docs)).filter((name) => name.endsWith(".md"));
  expect(files.length).toBeGreaterThan(0);
  expect(files.filter((name) => !index.includes(`](decisions/${name})`))).toEqual([]);
});
