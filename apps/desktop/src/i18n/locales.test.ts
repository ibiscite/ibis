import { parse, type Pattern } from "@fluent/syntax";
import { expect, test } from "vitest";
import { createInterfaceMessages, LANGUAGES, type InterfaceLanguage } from ".";

/** Each message, attribute and term by name, with what a translation must keep:
 * its variables and references (across variants) and its selectors' keys. */
function shape(source: string) {
  const entries = new Map<string, { variables: string[]; references: string[]; selectors: string[][] }>();
  const junk: string[] = [];
  for (const entry of parse(source, { withSpans: false }).body) {
    if (entry.type === "Junk") junk.push(entry.content.slice(0, 80));
    if (entry.type !== "Message" && entry.type !== "Term") continue;
    const id = `${entry.type === "Term" ? "-" : ""}${entry.id.name}`;
    const parts: [string, Pattern | null][] = [[id, entry.value], ...entry.attributes.map((attribute) => [`${id}.${attribute.id.name}`, attribute.value] as [string, Pattern])];
    for (const [name, pattern] of parts) {
      const variables = new Set<string>(), references = new Set<string>(), selectors: string[][] = [];
      const walk = (node: unknown): void => {
        if (!node || typeof node !== "object") return;
        const typed = node as { type?: string; id?: { name: string }; variants?: { default: boolean; key: { name?: string; value?: string } }[] };
        if (typed.type === "VariableReference") variables.add(typed.id!.name);
        if (typed.type === "MessageReference" || typed.type === "TermReference") references.add(typed.id!.name);
        if (typed.type === "SelectExpression") selectors.push(typed.variants!.map((variant) => `${variant.default ? "*" : ""}${variant.key.name ?? variant.key.value}`));
        for (const value of Object.values(node)) Array.isArray(value) ? value.forEach(walk) : walk(value);
      };
      walk(pattern);
      entries.set(name, { variables: [...variables].sort(), references: [...references].sort(), selectors });
    }
  }
  return { entries, junk };
}

const english = shape(LANGUAGES["en-US"].source);
const translations = (Object.keys(LANGUAGES) as InterfaceLanguage[]).filter((language) => language !== "en-US");
const PLURAL = /^\*?(zero|one|two|few|many|other)$/;

test.each(translations)("%s says every English message with the same placeables, and nothing else", (language) => {
  const { entries, junk } = shape(LANGUAGES[language].source);
  expect(junk, "unparsable lines").toEqual([]);
  expect([...entries.keys()].filter((id) => !english.entries.has(id)), "unknown messages").toEqual([]);
  expect([...english.entries.keys()].filter((id) => !entries.has(id)), "missing messages").toEqual([]);
  const categories = new Intl.PluralRules(LANGUAGES[language].intl).resolvedOptions().pluralCategories.slice().sort();
  const differences: string[] = [];
  for (const [id, source] of english.entries) {
    const translated = entries.get(id)!;
    if (translated.variables.join() !== source.variables.join()) differences.push(`${id}: variables ${translated.variables} ≠ ${source.variables}`);
    if (translated.references.join() !== source.references.join()) differences.push(`${id}: references ${translated.references} ≠ ${source.references}`);
    if (translated.selectors.length !== source.selectors.length) { differences.push(`${id}: selectors differ`); continue; }
    source.selectors.forEach((keys, index) => {
      const theirs = translated.selectors[index]!;
      const plural = keys.some((key) => PLURAL.test(key) && !/other/.test(key));
      if (!plural) {
        // Selectors on text (a platform, a state) and exact numbers keep their keys.
        if (theirs.join() !== keys.join()) differences.push(`${id}: keys ${theirs} ≠ ${keys}`);
        return;
      }
      // A count says each plural form its language has, `other` the default.
      const forms = theirs.filter((key) => PLURAL.test(key)).map((key) => key.replace("*", "")).sort();
      if (forms.join() !== categories.join() || !theirs.includes("*other")) differences.push(`${id}: plural forms ${theirs} ≠ ${categories}`);
    });
  }
  expect(differences).toEqual([]);
});

test.each(translations)("%s builds, formats every count and isolates its arguments", (language) => {
  const messages = createInterfaceMessages(LANGUAGES[language].source, false, language);
  expect(messages.language).toBe(language);
  // A Latin name keeps its place inside a translated sentence.
  expect(messages.text("shortcuts-picker-conflict", { command: "Synthetic" })).toContain("⁨Synthetic⁩");
  // Digits stay Latin, as in citations, keys and page numbers.
  expect(messages.text("settings-results-count", { count: 12 })).toMatch(/12/);
});
