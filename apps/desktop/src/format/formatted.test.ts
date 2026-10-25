import { CANCELLED, FormatClient } from "./client";
import { InlineWorker } from "./inline-worker";
import { formatCitation } from "./formatted";
import type { CiteItem } from "./engine";
import { plainText } from "./markup";
import { STYLE_TITLES, type StyleId } from "./styles";

// Synthetic references; both share a citation key, as two sources can.
const csl = JSON.stringify([
  { id: "same", type: "book", title: "First <i>book</i>", author: [{ family: "Garcia", given: "Ana" }], issued: { "date-parts": [[2021]] } },
  { id: "same", type: "book", title: "Broken", author: [{ family: 5 }] },
]);

const locatedCsl = JSON.stringify([
  { id: "same", type: "book", title: "Synthetic first", author: [{ family: "Garcia" }], issued: { "date-parts": [[2021]] } },
  { id: "same", type: "book", title: "Synthetic second", author: [{ family: "Okafor" }], issued: { "date-parts": [[2022]] } },
]);
const locators: readonly Omit<CiteItem, "id">[] = [{ locator: "37-39", label: "page" }, { locator: "84", label: "chapter" }];

describe("formatted citation for the picker", () => {
  const client = new FormatClient(() => new InlineWorker());

  it("formats the chosen references and names the ones it could not", async () => {
    const result = await formatCitation(csl, "apa", { client });
    expect(result.text).toBe("(Garcia, 2021); Broken");
    expect(result.html).toBe("(Garcia, 2021); Broken");
    expect(result.warnings).toEqual(["“Broken”: this style could not format the reference, so its title is shown instead"]);
  });

  it("retains fallback warnings when references have citation details", async () => {
    const result = await formatCitation(csl, "apa", { client, cites: locators });
    expect(result.text).toContain("37");
    expect(result.text).toContain("39");
    expect(result.text).toContain("Broken");
    expect(result.warnings).toEqual(["“Broken”: this style could not format the reference, so its title is shown instead"]);
  });

  it("rejects data that is not a list of references", async () => {
    await expect(formatCitation("{}", "apa", { client })).rejects.toThrow("the references could not be read");
  });

  it("does not format a preview that was already replaced", async () => {
    const replaced = new AbortController();
    replaced.abort();
    await expect(formatCitation(csl, "apa", { client, signal: replaced.signal, cites: locators })).rejects.toThrow(CANCELLED);
  });

  it.each([{ cites: [] }, { cites: [{}] }, { cites: [{}, {}, {}] }])("rejects mismatched citation details before starting a worker: %j", async ({ cites }) => {
    const start = vi.fn(() => new InlineWorker());
    await expect(formatCitation(locatedCsl, "apa", { client: new FormatClient(start), cites })).rejects.toThrow("the citation details do not match the references");
    expect(start).not.toHaveBeenCalled();
  });

  it("uses generated positional IDs even when callers supply extra IDs", async () => {
    const local = new FormatClient(() => new InlineWorker());
    const sent = vi.spyOn(local, "format");
    const cites = [{ id: "forged", locator: "37", label: "page" }, { id: "ref-0", locator: "84", label: "page" }];
    const result = await formatCitation(locatedCsl, "apa", { client: local, cites, segments: true });
    expect(sent).toHaveBeenCalledWith(expect.objectContaining({
      items: [expect.objectContaining({ id: "ref-0" }), expect.objectContaining({ id: "ref-1" })],
      cites: [{ id: "ref-0", locator: "37", label: "page" }, { id: "ref-1", locator: "84", label: "page" }],
    }), undefined);
    expect(result.text).toBe("(Garcia, 2021, p. 37; Okafor, 2022, p. 84)");
  });

  for (const style of Object.keys(STYLE_TITLES) as StyleId[]) {
    it(`preserves different locators and canonical ownership for ${style}`, async () => {
      const canonical = await formatCitation(locatedCsl, style, { client, cites: locators });
      const preview = await formatCitation(locatedCsl, style, { client, cites: locators, segments: true });
      expect(preview.text).toContain("37");
      expect(preview.text).toContain("39");
      expect(preview.text).toContain("84");
      expect({ text: preview.text, html: preview.html, warnings: preview.warnings }).toEqual({
        text: canonical.text, html: canonical.html, warnings: [],
      });
      const parts = preview.segments ?? [];
      expect(plainText(parts.flatMap((part) => part.markup))).toBe(canonical.text);
      expect(parts.filter((part) => part.item !== null).map((part) => part.item).sort()).toEqual([0, 1]);
      expect(plainText(parts.filter((part) => part.item === 0).flatMap((part) => part.markup))).toContain("37");
      expect(plainText(parts.filter((part) => part.item === 1).flatMap((part) => part.markup))).toContain("84");
      expect(preview.unrepresented).toEqual([]);
      const pages = await formatCitation(locatedCsl, style, { client, cites: [locators[0]!, { locator: "84", label: "page" }] });
      expect(canonical.text).not.toBe(pages.text);
    });

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
