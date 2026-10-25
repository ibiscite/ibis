import { copyLabel } from "./choice";

describe("the Enter label", () => {
  it("names the format, and says a numeric style copies reference entries (#1129)", () => {
    expect(copyLabel("pandoc", 1)).toBe("Copy as Pandoc");
    expect(copyLabel("style:apa", 2)).toBe("Copy 2 as APA");
    expect(copyLabel("style:ieee", 1)).toBe("Copy IEEE entry");
    expect(copyLabel("style:ieee", 2)).toBe("Copy 2 IEEE entries");
  });
});
