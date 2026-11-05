import { copyLabel } from "./choice";
import { outputName } from "../../format/outputs";
import { OUTPUTS } from "../../bindings";

describe("the Enter label", () => {
  it("names the format, and says a numeric style copies reference entries (#1129)", () => {
    expect(copyLabel("pandoc", 1)).toBe("Copy as Pandoc");
    expect(copyLabel("apa", 2)).toBe("Copy 2 as APA");
    expect(copyLabel("ieee", 1)).toBe("Copy IEEE entry");
    expect(copyLabel("ieee", 2)).toBe("Copy 2 IEEE entries");
    // Any numbered style the engine lists, not one picked by name.
    expect(copyLabel("vancouver-nlm", 1)).toBe("Copy Vancouver entry");
  });
});

test("every output is named by the engine's one name", () => {
  for (const output of OUTPUTS) expect(outputName(output.id)).toBe(output.name);
  expect(outputName("pandoc")).toBe("Pandoc");
  expect(outputName("autocite")).toBe("BibLaTeX \\autocite");
  expect(outputName("apa")).toBe("APA");
  expect(outputName("chicago-notes-bibliography")).toBe("Chicago (notes)");
});
