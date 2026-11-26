import { expect, test } from "vitest";
import { fixtureDestination, formatAnswer } from "../../testing/format";
import { initialDestination, reduceDestination } from "./destination";

test.each([
  { site: "overleaf.com", choice: "cite" as const, owner: "overleaf.com" },
  { site: "docs.google.com", choice: "apa" as const, owner: "docs.google.com" },
  { site: null, choice: "cite" as const, owner: "Example Browser" },
])("a completed format save belongs to $owner even when its format is unchanged", ({ site, choice, owner }) => {
  const destination = fixtureDestination({ app: "Example Browser", app_id: "example.browser", site_id: site,
    remembered_for: "Example Browser", format: formatAnswer(choice, "app") });
  const picked = reduceDestination({ ...initialDestination, destination, reader: true, ready: true },
    { type: "choose-format", choice, pick: "app" })!;
  const effect = picked.effects[0]!;
  expect(effect.type).toBe("save-destination");
  if (effect.type !== "save-destination") throw new Error("Expected a format save");
  expect(effect.save.site).toBe(site);
  const saved = reduceDestination(picked.destination, { type: "destination-saved", save: effect.save })!;
  expect(saved.destination.held).toBeNull();
  expect(saved.destination.destination?.format).toEqual(formatAnswer(choice, "app"));
  expect(saved.destination.destination?.remembered_for).toBe(owner);
});
