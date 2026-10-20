import { FormatClient } from "./client";
import { InlineWorker } from "./inline-worker";
import { formatCitation } from "./formatted";

// Synthetic references; both share a citation key, as two sources can.
const csl = JSON.stringify([
  { id: "same", type: "book", title: "First <i>book</i>", author: [{ family: "Garcia", given: "Ana" }], issued: { "date-parts": [[2021]] } },
  { id: "same", type: "book", title: "Broken", author: [{ family: 5 }] },
]);

describe("formatted citation for the picker", () => {
  const client = new FormatClient(() => new InlineWorker());

  it("formats the chosen references and names the ones it could not", async () => {
    const result = await formatCitation(csl, "apa", client);
    expect(result.text).toBe("(Garcia, 2021); Broken");
    expect(result.html).toBe("(Garcia, 2021); Broken");
    expect(result.warnings).toEqual(["“Broken”: this style could not format the reference, so its title is shown instead"]);
  });

  it("rejects data that is not a list of references", async () => {
    await expect(formatCitation("{}", "apa", client)).rejects.toThrow("the references could not be read");
  });
});
