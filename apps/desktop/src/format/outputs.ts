// The engine's outputs, generated into the bindings from its one list (#1260).
// Each has one id, stored and sent as it is, and one name, shown everywhere.
import { DEFAULT_OUTPUT, DEFAULT_STYLE, OUTPUTS } from "../bindings";
import type { Markup } from "./markup";
import type { CitationSpan } from "../bindings";

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

/** The one name every surface shows. */
export function outputName(id: OutputId): string {
  return outputOf(id).name;
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

/** The outputs in the engine's order: key syntaxes, then common styles. */
export const keyOutputs = OUTPUTS.filter((output) => !output.style);
export const styleOutputs = OUTPUTS.filter((output) => output.style);

/** The engine result displayed by the picker. */
export interface FormattedCitation {
  markup: Markup[];
  text: string;
  html: string;
  warnings: string[];
  spans?: CitationSpan[];
  project_check?: "paused" | "conflict" | "retained" | null;
  project_message?: string | null;
  project_checked_at_ms?: number | null;
  project_unverified_allowed?: boolean | null;
}
