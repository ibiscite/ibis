// The engine's outputs, generated into the bindings from its one list (#1260).
// Each has one id, stored and sent as it is, and one name, shown everywhere.
import { DEFAULT_OUTPUT, DEFAULT_STYLE, OUTPUTS } from "../bindings";
import type { CitedWarning, DisplayTree, Notice } from "../bindings";
import { say } from "../i18n";
import { OUTPUT_DESCRIPTIONS, OUTPUT_NAMES } from "../i18n/outputs";

export type OutputChoice = (typeof OUTPUTS)[number];
export type OutputId = OutputChoice["id"];

const listed = new Map<string, OutputChoice>(OUTPUTS.map((output) => [output.id, output]));

/** What is written when nothing else is chosen, and what an unknown id means. */
export const defaultOutput: OutputId = DEFAULT_OUTPUT;
/** The style for finished text when nothing names one. */
export const defaultStyle: OutputId = DEFAULT_STYLE;

/** The listed output `id` names; anything else is the default. */
export function outputOf(id: string | null | undefined): OutputChoice {
  return (id ? listed.get(id) : undefined) ?? listed.get(defaultOutput)!;
}

/** The id if the engine lists it. */
export function listedOutput(id: string | null | undefined): OutputId | null {
  return id && listed.has(id) ? id as OutputId : null;
}

/** The one name every surface shows, in the interface language. */
export function outputName(id: OutputId): string {
  const output = outputOf(id), message = OUTPUT_NAMES[output.id as keyof typeof OUTPUT_NAMES];
  return message ? say(message) : output.name;
}

/** What the name leaves out, such as a style's edition, in the interface language. */
export function outputDescription(id: OutputId): string | null {
  const output = outputOf(id), message = OUTPUT_DESCRIPTIONS[output.id as keyof typeof OUTPUT_DESCRIPTIONS];
  return message ? say(message) : output.description;
}

/** A citation style, written by the formatter. */
export function isStyle(id: OutputId): boolean {
  return outputOf(id).style;
}

/** The output when it is a style, for what only a style can do. */
export function styleOf(id: OutputId): OutputId | null {
  return isStyle(id) ? id : null;
}

/**
 * A numbered style's number depends on every reference the document cites,
 * which one copy cannot see, so these copy the reference entry instead (#1129).
 */
export function writesEntries(id: OutputId): boolean {
  return outputOf(id).entries;
}

/** The prepared format shows its engine result. Other key formats show the chosen
 * citation keys in the engine’s example pattern; styles show a labeled example.
 * Lookup IDs are never text. */
export function previewOf(id: OutputId, formatted: Pick<FormattedCitation, "text"> | null, keys: readonly string[] = []): { text: string; more: number; example: boolean } | null {
  if (formatted?.text) return { text: formatted.text, more: 0, example: false };
  const output = outputOf(id);
  if (!output.example) return null;
  if (!output.style && keys[0]) return { text: output.example.replace(/\bkey\b/g, keys[0]), more: keys.length - 1, example: false };
  return { text: output.example, more: 0, example: true };
}

/** It shares references (BibTeX, DOI links) rather than citing them: a one-off
 * output for this copy, never remembered as a format (decision 0051). */
export function sharesReferences(id: OutputId): boolean {
  return outputOf(id).shares;
}

/** The outputs in the engine's order: key syntaxes, then ways to share, then common styles. */
export const keyOutputs = OUTPUTS.filter((output) => !output.style && !output.shares);
export const shareOutputs = OUTPUTS.filter((output) => output.shares);
export const styleOutputs = OUTPUTS.filter((output) => output.style);

/** The engine result displayed by the picker. */
export interface FormattedCitation {
  effective_output?: string | null;
  display: DisplayTree;
  text: string;
  warnings: CitedWarning[];
  /** Why a key citation is refused: a key in the bibliography names a different work. */
  project_refusal?: string | null;
  /** The engine's notices about it, such as a full first note. */
  notices?: readonly Notice[];
}
