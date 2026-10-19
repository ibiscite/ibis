import manifest from "../../vendor/csl/manifest.json";
import { escape, Formatter, STYLES, type CslItem } from "./engine";

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

  it("escapes fallback text for each output", () => {
    expect(escape("a & {b} 😀", "rtf")).toBe("a & \\{b\\} \\uc0\\u-10179{}\\uc0\\u-8704{}");
    expect(escape("a\u0000b\tc", "rtf")).toBe("a b c");
    expect(escape("<a>", "html")).toBe("&lt;a&gt;");
    expect(escape("<a>", "text")).toBe("<a>");
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
