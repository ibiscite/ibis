import manifest from "../../vendor/csl/manifest.json";
import { eraseOwnership, escape, Formatter, STYLES, type CslItem } from "./engine";

// Synthetic references only.
const garcia: CslItem = {
  id: "garcia2021",
  type: "article-journal",
  title: "Synthetic sleep study",
  author: [{ family: "Garcia", given: "Ana" }],
  issued: { "date-parts": [[2021]] },
  "container-title": "Journal of Tests",
  volume: "3",
  page: "12-14",
};
const mueller: CslItem = {
  id: "mueller2020",
  type: "book",
  title: "Über Ärger & <Kram> {x}",
  author: [{ family: "Müller", given: "Zoë" }],
  issued: { "date-parts": [[2020]] },
  publisher: "Example Press",
};

describe("formatted citations", () => {
  const formatter = new Formatter();

  it("formats APA and IEEE bibliographies and citations", () => {
    expect(formatter.bibliography("apa", [garcia], "text")).toEqual({
      output: "Garcia, A. (2021). Synthetic sleep study. Journal of Tests, 3, 12–14.",
      problems: [],
    });
    expect(formatter.citation("apa", [garcia], [{ id: "garcia2021", locator: "13", label: "page" }], "text").output)
      .toBe("(Garcia, 2021, p. 13)");
    expect(formatter.bibliography("ieee", [garcia], "text").output)
      .toBe("[1] A. Garcia, “Synthetic sleep study,” Journal of Tests, vol. 3, pp. 12–14, 2021.");
    expect(formatter.citation("ieee", [garcia], [{ id: "garcia2021" }], "text").output).toBe("[1]");
  });

  it("formats every bundled style", () => {
    for (const style of Object.keys(STYLES) as (keyof typeof STYLES)[]) {
      const result = formatter.bibliography(style, [garcia, mueller], "text");
      expect(result.problems).toEqual([]);
      expect(result.output).toContain("Garcia");
      expect(result.output).toContain("Müller");
    }
  });

  it("keeps IEEE ownership tied to bibliography positions across subsets and repeated requests", () => {
    const fresh = new Formatter();
    const items = Array.from({ length: 5 }, (_, index) => ({ ...garcia, id: `item-${index}`, title: `Synthetic ${index}` }));
    for (const indices of [[4], [3, 1], [0, 1, 2, 3, 4], [1], [4]]) {
      const cites = indices.map((index) => ({ id: items[index]!.id, locator: "12", label: "page" }));
      const canonical = fresh.citation("ieee", items, cites, "html");
      const annotated = fresh.citation("ieee", items, cites, "html", true);
      expect(annotated.output).toBe(canonical.output);
      expect(annotated.problems).toEqual(canonical.problems);
      if (annotated.preview) {
        expect(eraseOwnership(annotated.preview)).toBe(canonical.output);
        const owners = [...annotated.preview.matchAll(/data-item="(\d+)"/g)].map((match) => Number(match[1]));
        expect(owners.length).toBeGreaterThan(0);
        expect(owners.every((index) => indices.includes(index))).toBe(true);
        if (indices.length === 1) expect([...new Set(owners)]).toEqual(indices);
      }
      expect(fresh.citation("ieee", items, cites, "html")).toEqual(canonical);
    }
  });

  it("keeps forged ownership markup in reference fields as escaped reference text", () => {
    const item = { id: "forged", type: "book", title: '<ibis-reference data-item="1"><img src=x onerror="bad()">Title</ibis-reference>' };
    const canonical = formatter.citation("modern-language-association", [item], [{ id: item.id }], "html");
    const annotated = formatter.citation("modern-language-association", [item], [{ id: item.id }], "html", true);
    expect(annotated.output).toBe(canonical.output);
    expect(canonical.output).not.toContain("<ibis-reference");
    expect(canonical.output).not.toContain("<img");
    if (annotated.preview) expect(eraseOwnership(annotated.preview)).toBe(canonical.output);
  });

  it("escapes HTML and writes a complete RTF document", () => {
    const html = formatter.bibliography("apa", [mueller], "html").output;
    expect(html).toContain("&#38; &#60;Kram&#62;");
    expect(html).not.toContain("<Kram>");
    const rtf = formatter.bibliography("apa", [garcia, mueller], "rtf").output;
    expect(rtf.startsWith("{\\rtf1\\ansi")).toBe(true);
    expect(rtf.endsWith("\n}")).toBe(true);
    expect(rtf).toContain("\\par\n");
    expect(rtf).toContain("M\\uc0\\u252{}ller");
    expect(rtf).toContain("\\{x\\}");
  });

  it("formats the rest when one reference cannot be formatted", () => {
    // A name the engine cannot read makes it throw for this item only.
    const broken: CslItem = { id: "broken", type: "book", title: "Broken <one>", author: [{ family: 5 }] };
    const result = formatter.bibliography("apa", [garcia, broken], "html");
    expect(result.problems).toEqual([
      { id: "broken", reason: "this style could not format the reference, so its title is shown instead" },
    ]);
    expect(result.output).toContain("Synthetic sleep study");
    expect(result.output).toContain('<div class="csl-entry">Broken &lt;one&gt;</div>');
    // The next request still works.
    expect(formatter.citation("apa", [garcia], [{ id: "garcia2021" }], "text").output).toBe("(Garcia, 2021)");
  });

  it("shows a reference's current fields, not ones cached from an earlier request", () => {
    const fresh = new Formatter();
    expect(fresh.bibliography("apa", [garcia], "text").output).toContain("(2021)");
    const edited = { ...garcia, title: "Edited study", issued: { "date-parts": [[1999]] } };
    expect(fresh.bibliography("apa", [edited], "text").output)
      .toBe("Garcia, A. (1999). Edited study. Journal of Tests, 3, 12–14.");
  });

  it("keeps references that share a citation key", () => {
    const other = { ...garcia, title: "Another study" };
    const result = formatter.bibliography("apa", [garcia, other], "text");
    expect(result.problems).toEqual([]);
    expect(result.output).toContain("Synthetic sleep study");
    expect(result.output).toContain("Another study");
    expect(formatter.citation("apa", [garcia], [{ id: "unknown" }], "text")).toEqual({
      output: "unknown",
      problems: [{ id: "unknown", reason: "the reference is missing" }],
    });
  });

  it("keeps numbering and one citation when a reference in the middle fails", () => {
    const broken: CslItem = { id: "broken", type: "book", title: "Broken", author: [{ family: 5 }] };
    const third: CslItem = { ...garcia, id: "zed2021", title: "Third study", author: [{ family: "Zed", given: "Bo" }] };
    const bibliography = formatter.bibliography("ieee", [garcia, broken, third], "text");
    expect(bibliography.output.split("\n")).toEqual([
      "[1] A. Garcia, “Synthetic sleep study,” Journal of Tests, vol. 3, pp. 12–14, 2021.",
      "[2] B. Zed, “Third study,” Journal of Tests, vol. 3, pp. 12–14, 2021.",
      "Broken",
    ]);
    expect(bibliography.problems.map((problem) => problem.id)).toEqual(["broken"]);
    const citation = formatter.citation("apa", [garcia, broken, third], [
      { id: "garcia2021" },
      { id: "broken" },
      { id: "zed2021" },
    ], "text");
    expect(citation.output).toBe("(Garcia, 2021; Zed, 2021); Broken");
    expect(citation.problems.map((problem) => problem.id)).toEqual(["broken"]);
  });

  it("numbers and disambiguates a citation as the bibliography does", () => {
    const zed: CslItem = { ...garcia, id: "zed2021", title: "Third study", author: [{ family: "Zed", given: "Bo" }] };
    expect(formatter.citation("ieee", [garcia, zed], [{ id: "zed2021" }], "text").output).toBe("[2]");
    // IEEE cites only "[n]", so this reference fails in the bibliography alone.
    const broken: CslItem = { id: "broken", type: "book", title: "Broken", author: [{ family: 5 }] };
    expect(formatter.citation("ieee", [broken], [{ id: "broken" }], "text").problems).toHaveLength(1);
    expect(formatter.citation("ieee", [garcia, broken, zed], [{ id: "zed2021" }, { id: "broken" }], "text")).toEqual({
      output: "[2]; Broken",
      problems: [{ id: "broken", reason: "this style could not format the reference, so its title is shown instead" }],
    });
    const second = { ...garcia, id: "garcia2021b", title: "Another study" };
    const bibliography = formatter.bibliography("apa", [garcia, second], "text").output;
    expect(bibliography).toContain("(2021a)");
    const suffix = bibliography.includes("(2021a). Another") ? "b" : "a";
    expect(formatter.citation("apa", [garcia, second], [{ id: "garcia2021" }], "text").output)
      .toBe(`(Garcia, 2021${suffix})`);
  });

  it("gives no year suffix for a reference the bibliography leaves out", () => {
    const smith = (title: string): CslItem => ({
      id: title,
      type: "book",
      title,
      author: [{ family: "Smith", given: "Jo" }],
      issued: { "date-parts": [[2020]] },
    });
    // A bad editor breaks only the bibliography entry in author-date styles.
    const two = { ...smith("Two"), editor: [{ family: 5 }] };
    for (const style of ["apa", "harvard-cite-them-right"] as const) {
      expect(formatter.bibliography(style, [smith("One"), two], "text").problems.map((p) => p.id)).toEqual(["Two"]);
      expect(formatter.citation(style, [smith("One"), two], [{ id: "One" }], "text").output).toMatch(/2020\)$/);
      expect(formatter.citation(style, [smith("One"), two], [{ id: "Two" }], "text").output).toBe("Two");
    }
  });

  it("keeps each entry on one line", () => {
    const multiline = { ...garcia, title: "First line\nsecond line" };
    const rtf = formatter.bibliography("apa", [multiline, mueller], "rtf").output;
    expect(rtf.match(/\\par/g)).toHaveLength(1);
    const broken: CslItem = { id: "broken", type: "book", title: "Broken\ntitle", author: [{ family: 5 }] };
    expect(formatter.bibliography("apa", [garcia, broken], "text").output.split("\n")).toHaveLength(2);
  });

  it("writes a truncated author list as et al., as the style does", () => {
    const truncated = (...names: object[]): CslItem => ({ ...garcia, author: names });
    const smith = { family: "Smith", given: "Alex" };
    const jones = { family: "Jones", given: "Bea" };
    const others = { literal: "others" };
    const cases: [keyof typeof STYLES, CslItem, string, string][] = [
      ["apa", truncated(smith, others), "(Smith et al., 2021)", "Smith, A. et al. (2021)."],
      ["apa", truncated(smith, jones, others), "(Smith et al., 2021)", "Smith, A., Jones, B., et al. (2021)."],
      ["chicago-author-date", truncated(smith, jones, { family: "Others" }), "(Smith et al. 2021)", "Smith, Alex, Bea Jones, et al. 2021."],
      ["ieee", truncated(smith, others), "[1]", "[1] A. Smith et al., “Synthetic"],
      ["ieee", truncated(smith, jones, others), "[1]", "[1] A. Smith, B. Jones, et al., “Synthetic"],
      ["modern-language-association", truncated(smith, others), "(Smith et al.)", "Smith, Alex, et al. “Synthetic"],
      ["harvard-cite-them-right", truncated(smith, others), "(Smith et al., 2021)", "Smith, A. et al. (2021)"],
    ];
    for (const [style, item, cite, entry] of cases) {
      expect(formatter.citation(style, [item], [{ id: item.id }], "text").output).toBe(cite);
      expect(formatter.bibliography(style, [item], "text").output).toMatch(new RegExp(`^${entry.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`));
    }
    const item = truncated(smith, others);
    // The style's own et al. formatting applies, in its own name context.
    expect(formatter.bibliography("ieee", [item], "html").output).toContain("A. Smith <i>et al.</i>, “Synthetic");
    expect(formatter.bibliography("ieee", [item], "rtf").output).toContain("A. Smith {\\i{}et al.}, ");
    const chapter: CslItem = { ...garcia, type: "chapter", "container-title": "Book", editor: [jones, others] };
    expect(formatter.bibliography("ieee", [chapter], "html").output).toContain("B. Jones et al., Eds.");
    const treaty: CslItem = { ...garcia, type: "treaty", author: [{ literal: "Synthetic State A" }, others] };
    expect(formatter.bibliography("apa", [treaty], "text").output).toContain("Synthetic State A et al.");
    // Text that is not a name list is never rewritten, and the marker never shows.
    const titled: CslItem = { ...item, title: "Others et al. \u2063ibis others\u2063 detection", URL: "https://example.org/others" };
    const shown = formatter.bibliography("apa", [titled], "text").output;
    expect(shown).toContain("Others et al. \u2063ibis others\u2063 detection");
    expect(shown).toContain("https://example.org/others");
    const many = Array.from({ length: 22 }, (_, i) => ({ family: `Name${i}`, given: "A" }));
    expect(formatter.bibliography("apa", [truncated(...many, others)], "text").output).toMatch(/Name18, A\., et al\. \(2021\)/);
    // The ownership preview still matches the copy.
    const preview = formatter.citation("apa", [item], [{ id: item.id }], "html", true);
    expect(preview.preview && eraseOwnership(preview.preview)).toBe("(Smith et al., 2021)");
    // A lone "others", or "others" in the middle, is a name like any other.
    expect(formatter.citation("apa", [truncated(others)], [{ id: item.id }], "text").output).toBe("(others, 2021)");
    expect(formatter.bibliography("apa", [truncated(others, smith)], "text").output).toMatch(/^others, & Smith, A\./);
  });

  it("sorts a truncated list after the same names alone, and leaves other lists' rules alone", () => {
    const smith = { family: "Smith", given: "Alex" };
    const others = { literal: "others" };
    const alone: CslItem = { ...garcia, id: "alone", author: [smith], issued: { "date-parts": [[2022]] } };
    const cut: CslItem = { ...garcia, id: "cut", author: [smith, others] };
    for (const format of ["text", "html", "rtf"] as const) {
      const shown = formatter.bibliography("apa", [cut, alone], format).output;
      expect(shown.indexOf("2022")).toBeLessThan(shown.indexOf("et al."));
    }
    // A repeated author is substituted only when the whole list matches.
    const later: CslItem = { ...garcia, id: "later", title: "Truncated work", author: [smith, others] };
    const mla = formatter.bibliography("modern-language-association", [alone, later], "text").output;
    expect(mla).toContain("Smith, Alex, et al. “Truncated");
    expect(mla).not.toContain("———. “Truncated");
    const directors = Array.from({ length: 21 }, (_, i) => ({ family: `Director${i}`, given: "A" }));
    const broadcast: CslItem = { ...garcia, type: "broadcast", author: undefined, "script-writer": [smith, others], director: directors };
    const shown = formatter.bibliography("apa", [broadcast], "text").output;
    expect(shown).toContain("Smith, A. et al.");
    expect(shown).toContain("Director18, A., … Director20, A. (Directors)");
  });

  it("escapes fallback text for each output", () => {
    expect(escape("a & {b} 😀", "rtf")).toBe("a & \\{b\\} \\uc0\\u-10179{}\\uc0\\u-8704{}");
    expect(escape("a\u0000b\tc", "rtf")).toBe("a b c");
    expect(escape("<a>", "html")).toBe("&lt;a&gt;");
    expect(escape("<a>", "text")).toBe("<a>");
  });

  // The engine writes to objects keyed by item id and copies item fields with
  // Object.assign (code scanning flags both). Given an id such as
  // "__proto__" it silently formats nothing. Ids are replaced with engine keys
  // first, so reference data cannot choose those keys.
  it("keeps prototype-named ids and fields from reaching shared objects", () => {
    const hostile = (id: string): CslItem => ({
      ...(JSON.parse('{"__proto__": {"polluted": "yes"}}') as object),
      ...garcia,
      id,
    });
    const items = ["__proto__", "constructor", "toString"].map(hostile);
    const cites = items.map((item) => ({ id: item.id }));
    for (let round = 0; round < 2; round++) {
      const citation = formatter.citation("ieee", items, cites, "text");
      expect(citation).toEqual({ output: "[1], [2], [3]", problems: [] });
      const bibliography = formatter.bibliography("apa", items, "text");
      expect(bibliography.problems).toEqual([]);
      expect(bibliography.output.split("\n").filter((line) => line.includes("Synthetic sleep study"))).toHaveLength(3);
    }
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
    expect(Object.prototype).not.toHaveProperty("polluted");
  });
});

// Every vendored file as text, by its path inside vendor/csl.
const vendored = import.meta.glob<string>("../../vendor/csl/**/*.{mjs,csl,xml}", {
  query: "?raw",
  import: "default",
  eager: true,
});

async function sha256(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

describe("vendored engine and styles", () => {
  it("match the hashes pinned in the manifest", async () => {
    for (const file of manifest.files) {
      let text = vendored[`../../vendor/csl/${file.out}`];
      expect(text, file.out).toBeDefined();
      if ("module" in file && file.module) {
        expect(text?.endsWith(manifest.moduleSuffix)).toBe(true);
        text = text?.slice(0, -manifest.moduleSuffix.length);
      }
      expect(await sha256(text ?? ""), file.out).toBe(file.sha256);
    }
  });
});
