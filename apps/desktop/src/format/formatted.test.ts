import { CANCELLED, FormatClient } from "./client";
import { InlineWorker } from "./inline-worker";
import { formatCitation } from "./formatted";
import { plainText } from "./markup";
import { STYLE_TITLES, type StyleId } from "./styles";

// Synthetic references; both share a citation key, as two sources can.
const csl = JSON.stringify([
  { id: "same", type: "book", title: "First <i>book</i>", author: [{ family: "Garcia", given: "Ana" }], issued: { "date-parts": [[2021]] } },
  { id: "same", type: "book", title: "Broken", author: [{ family: 5 }] },
]);

describe("formatted citation for the picker", () => {
  const client = new FormatClient(() => new InlineWorker());

  it("formats the chosen references and names the ones it could not", async () => {
    const result = await formatCitation(csl, "apa", { client });
    expect(result.text).toBe("(Garcia, 2021); Broken");
    expect(result.html).toBe("(Garcia, 2021); Broken");
    expect(result.warnings).toEqual(["“Broken”: this style could not format the reference, so its title is shown instead"]);
  });

  it("rejects data that is not a list of references", async () => {
    await expect(formatCitation("{}", "apa", { client })).rejects.toThrow("the references could not be read");
  });

  it("does not format a preview that was already replaced", async () => {
    const replaced = new AbortController();
    replaced.abort();
    await expect(formatCitation(csl, "apa", { client, signal: replaced.signal })).rejects.toThrow(CANCELLED);
  });

  for (const style of Object.keys(STYLE_TITLES) as StyleId[]) {
    for (const families of [["Okafor", "García"], ["García", "García"], ["سالم", "לוי"]]) {
      it(`retains canonical output and reference ownership for ${style}: ${families.join(" / ")}`, async () => {
        const items = JSON.stringify(families.map((family, index) => ({
          id: "same", type: "book", title: `Synthetic ${index}`, author: [{ family }], issued: { "date-parts": [[2020 + index]] },
        })));
        const canonical = await formatCitation(items, style, { client });
        const preview = await formatCitation(items, style, { client, segments: true });
        expect({ text: preview.text, html: preview.html, warnings: preview.warnings }).toEqual({
          text: canonical.text, html: canonical.html, warnings: canonical.warnings,
        });
        expect(preview.segments).toBeDefined();
        const parts = preview.segments ?? [];
        expect(plainText(parts.flatMap((part) => part.markup))).toBe(canonical.text);
        expect(parts.filter((part) => part.item !== null).map((part) => part.item).sort()).toEqual([0, 1]);
        expect(preview.unrepresented).toEqual([]);
      });
    }
  }

  it("keeps an unprinted MLA cluster opaque instead of inventing reference text", async () => {
    const items = JSON.stringify([{ type: "book" }, { type: "book" }]);
    const canonical = await formatCitation(items, "modern-language-association", { client });
    const preview = await formatCitation(items, "modern-language-association", { client, segments: true });
    expect(preview.text).toBe(canonical.text);
    expect(preview.html).toBe(canonical.html);
    expect(preview.unrepresented).toEqual([0, 1]);
    expect((preview.segments ?? []).every((part) => part.item === null)).toBe(true);
  });
});
