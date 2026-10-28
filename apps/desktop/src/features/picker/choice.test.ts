import { copyLabel, outputName } from "./choice";

describe("the Enter label", () => {
  it("names the format, and says a numeric style copies reference entries (#1129)", () => {
    expect(copyLabel("pandoc", 1)).toBe("Copy as Pandoc");
    expect(copyLabel("style:apa", 2)).toBe("Copy 2 as APA");
    expect(copyLabel("style:ieee", 1)).toBe("Copy IEEE entry");
    expect(copyLabel("style:ieee", 2)).toBe("Copy 2 IEEE entries");
  });
});

test.each([
  ["pandoc", "Pandoc key"], ["typst", "Typst key"], ["keys", "Keys only"],
  ["cite", "\\cite"], ["autocite", "\\autocite"], ["parencite", "\\parencite"], ["textcite", "\\textcite"],
  ["style:apa", "APA · text"], ["style:chicago-author-date", "Chicago · author-date text"],
  ["style:ieee", "IEEE · reference entry"], ["style:harvard-cite-them-right", "Harvard · text"],
  ["style:modern-language-association", "MLA · text"],
] as const)("%s names what is actually pasted", (choice, expected) => {
  expect(outputName(choice)).toBe(expected);
});
