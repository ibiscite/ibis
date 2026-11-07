// Where the next citation goes and in which format, as part of the picker
// session (decision 0035): the app or writing project in front, the format
// picked for it, and the OutputToken a copy is checked against.
import type { AppPaste, PickerDestination } from "../../api";
import type * as nativeApi from "../../api";
import { listedOutput, type OutputId } from "../../format/outputs";
import type { Operation } from "./Notices";

/** Which opening, format choice and paste choice a copy was made for. */
export type OutputToken = { opening: number; choice: number; delivery: number };
export type ProjectEdit = Parameters<typeof nativeApi.editWritingProject>[1];

export type DestinationState = {
  /** Whether this picker reads its destination at all; without a reader it is always ready. */
  reader: boolean;
  destination: PickerDestination | null;
  /** Beats the app's format until saved or copied: a pick being saved, a pick
   * that could not be saved, or the output a permission return prepared. */
  held: OutputId | null;
  ready: boolean;
  /** Saves still out; they run one after another. */
  saves: number;
  readProblem: string | null; editProblem: string | null;
  /** The newest read, and whether it began an opening: only it may answer. */
  reads: number; resetRead: boolean;
  output: OutputToken;
  /** Settings refreshes can supersede the return's read within this opening. */
  preserveOpeningOutput: boolean;
  capturedId: string | null | undefined;
  observedVersion: [number, number] | null | undefined;
  settlingOpening: boolean; settlingProject: boolean;
  lastPolicy: string | undefined;
};

export const initialDestination: DestinationState = {
  reader: false, destination: null, held: null, ready: false, saves: 0, readProblem: null, editProblem: null,
  reads: 0, resetRead: false, output: { opening: 0, choice: 0, delivery: 0 }, preserveOpeningOutput: false,
  capturedId: undefined, observedVersion: undefined, settlingOpening: false, settlingProject: false, lastPolicy: undefined,
};

type Save = { opening: number; choice: number; appId: string; output: OutputId | null; paste: AppPaste | null; identity: PickerDestination["app_identity"] | null;
  /** What a saved format changes before the destination is read again. */
  remembered: OutputId | null; reread: boolean; clearHeld: boolean; rereadOnFailure: boolean };

export type DestinationEvent =
  | { type: "destination-reader"; present: boolean }
  /** `format` is what the picker shows now: a permission return keeps it. */
  | { type: "read-destination"; reset: boolean; preserve?: boolean; format?: OutputId }
  | { type: "destination-read"; request: number; destination: PickerDestination }
  | { type: "destination-read-failed"; request: number; error: unknown }
  | { type: "project-changed"; version: [number, number] | null }
  | { type: "choose-format"; choice: OutputId; pick: "app" | "default" | "paste" }
  | { type: "choose-once"; choice: OutputId }
  | { type: "choose-paste"; paste: AppPaste; output: OutputId | null }
  | { type: "destination-saved"; save: Save }
  | { type: "destination-save-failed"; save: Save; partial: boolean }
  | { type: "destination-save-settled" }
  | { type: "project-edit" }
  | { type: "project-edit-failed"; opening: number; choice: number; error: unknown }
  | { type: "consume-held"; output: OutputToken };

export type DestinationEffect =
  | { type: "read-destination"; request: number }
  | { type: "save-destination"; save: Save }
  | { type: "choose-default"; output: OutputId }
  | { type: "report"; operation: Operation; failed: boolean };

export type DestinationStep = { destination: DestinationState; effects: DestinationEffect[];
  /** The project context changed: the session retires what it was for. */
  observed?: { clearReplacement: boolean } };

/** The format the next paste uses. */
export function destinationFormat(d: DestinationState, fallback: OutputId) {
  const project = listedOutput(d.destination?.writing_project?.output);
  return d.held ?? project ?? listedOutput(d.destination?.remembered) ?? listedOutput(d.destination?.automatic) ?? fallback;
}
export const projectVersion = (d: DestinationState) => d.observedVersion?.join(":") ?? null;
export const sameOutput = (a: OutputToken, b: OutputToken) => a.opening === b.opening && a.choice === b.choice && a.delivery === b.delivery;

const report = (operation: Operation, failed = true): DestinationEffect => ({ type: "report", operation, failed });
const settled = (d: DestinationState): DestinationState => ({ ...d, settlingOpening: false, settlingProject: false, ready: true });

function read(d: DestinationState, reset: boolean, preserve = false, format?: OutputId): DestinationStep {
  if (!d.reader) return { destination: d, effects: [] };
  const request = d.reads + 1;
  let next: DestinationState = { ...d, reads: request, resetRead: reset, ready: false };
  const effects: DestinationEffect[] = [];
  if (reset) {
    next = { ...next, editProblem: null, settlingOpening: true, output: { ...d.output, opening: d.output.opening + 1 }, lastPolicy: undefined,
      preserveOpeningOutput: preserve, held: preserve ? format ?? null : null };
    effects.push(report("appChoice", false), report("appDelivery", false));
  }
  // A reopening reads after outstanding saves, so it sees the durable choice.
  effects.push({ type: "read-destination", request });
  return { destination: next, effects };
}
const withEffects = (step: DestinationStep, effects: DestinationEffect[]): DestinationStep => ({ ...step, effects: [...effects, ...step.effects] });

function save(d: DestinationState, change: Omit<Save, "opening" | "choice">): DestinationStep {
  const s: Save = { ...change, opening: d.output.opening, choice: d.output.choice };
  return { destination: { ...d, saves: d.saves + 1 }, effects: [{ type: "save-destination", save: s }] };
}

/** Native generations are wrapping u32s; a late event cannot retire newer intent. */
const older = (version: [number, number], previous: [number, number] | null | undefined) => {
  if (!previous) return false;
  const part = version[0] === previous[0] ? 1 : 0;
  return ((version[part] - previous[part]) >>> 0) > 0x7fff_ffff;
};

export function reduceDestination(d: DestinationState, event: DestinationEvent): DestinationStep | null {
  switch (event.type) {
    case "destination-reader":
      return d.reader === event.present ? null : { destination: { ...d, reader: event.present, ready: !event.present || d.ready }, effects: [] };
    case "read-destination": return read(d, event.reset, event.preserve, event.format);
    case "destination-read": {
      if (event.request !== d.reads) return null;
      const next = event.destination;
      const settling = d.settlingOpening || d.settlingProject;
      let output = d.output, held = d.held, preserve = d.preserveOpeningOutput;
      // The app in front changed under the same opening: its format starts afresh.
      if (!settling && !d.resetRead && !preserve && d.capturedId !== undefined && d.capturedId !== next.app_id) {
        output = { ...output, opening: output.opening + 1 };
        held = null;
      }
      const generation = next.context_version?.[0] ?? null;
      const version = next.context_version?.join(":") ?? null;
      const versionChanged = projectVersion(d) !== version;
      const generationChanged = d.observedVersion !== undefined && (d.observedVersion?.[0] ?? null) !== generation;
      if (generationChanged) {
        if (!settling) output = { ...output, choice: output.choice + 1 };
        held = null;
        preserve = false;
      }
      const policy = JSON.stringify([next.app_id, next.app_identity, next.paste, next.can_paste, next.unreadable]);
      if (!d.resetRead && d.lastPolicy !== undefined && d.lastPolicy !== policy) output = { ...output, delivery: output.delivery + 1 };
      return {
        destination: settled({ ...d, capturedId: next.app_id, observedVersion: next.context_version ?? null, output, held, preserveOpeningOutput: preserve,
          lastPolicy: policy, destination: next, readProblem: null }),
        effects: [report("appRead", false), report("appChoices", next.unreadable)],
        ...versionChanged ? { observed: { clearReplacement: generationChanged } } : {},
      };
    }
    case "destination-read-failed": {
      if (event.request !== d.reads) return null;
      const said = typeof event.error === "string";
      return { destination: settled({ ...d, destination: null, readProblem: said ? event.error as string : null }),
        effects: [report("appChoices", false), report("appRead", !said)] };
    }
    case "project-changed": {
      const { version } = event;
      const previous = d.observedVersion;
      if (!version) return read(d, false);
      if (older(version, previous)) return null;
      const generationChanged = previous !== undefined && (previous?.[0] ?? null) !== version[0];
      let next: DestinationState = { ...d, observedVersion: version };
      if (generationChanged) {
        next = { ...next, preserveOpeningOutput: false, held: null,
          output: d.settlingOpening || d.settlingProject ? d.output : { ...d.output, choice: d.output.choice + 1 } };
      }
      const changed = !previous || previous[0] !== version[0] || previous[1] !== version[1];
      return { ...read(next, false), ...changed ? { observed: { clearReplacement: generationChanged } } : {} };
    }
    case "choose-format": {
      let next: DestinationState = { ...d, held: event.choice, output: { ...d.output, choice: d.output.choice + 1 } };
      if (!next.ready && next.reader) return { destination: next, effects: [] };
      const id = d.destination?.app_id;
      if (event.pick === "app" && id) {
        return save(next, { appId: id, output: event.choice, paste: null, identity: d.destination?.app_identity ?? null,
          remembered: event.choice, reread: Boolean(d.destination?.unreadable), clearHeld: true, rereadOnFailure: false });
      }
      if (event.pick !== "default") return { destination: next, effects: [] };
      next = { ...next, held: null };
      return { destination: next, effects: [{ type: "choose-default", output: event.choice }] };
    }
    case "choose-once": return { destination: { ...d, held: event.choice, output: { ...d.output, choice: d.output.choice + 1 } }, effects: [] };
    case "choose-paste": {
      const id = d.destination?.app_id;
      if (!id || !d.ready || d.saves > 0) return null;
      // Persisting delivery does not resume the Enter that opened the chooser.
      // Read the native policy back before showing an intended paste in the footer.
      const next = { ...d, output: { ...d.output, delivery: d.output.delivery + 1 }, lastPolicy: undefined };
      // Its format saves before its delivery choice, so a failure can leave the format saved: read what was.
      return save(next, { appId: id, output: event.output, paste: event.paste, identity: d.destination?.app_identity ?? null,
        remembered: null, reread: true, clearHeld: false, rereadOnFailure: event.output !== null });
    }
    case "destination-saved": {
      const { save: s } = event;
      if (s.opening !== d.output.opening) return null;
      let next = s.remembered && d.destination ? { ...d, destination: { ...d.destination, remembered: s.remembered } } : d;
      if (s.clearHeld && s.choice === d.output.choice) next = { ...next, preserveOpeningOutput: false, held: null };
      const reports = [report("appChoice", false), report("appDelivery", false)];
      return s.reread ? withEffects(read(next, false), reports) : { destination: next, effects: reports };
    }
    case "destination-save-failed": {
      const { save: s } = event;
      if (s.opening !== d.output.opening) return null;
      // A format can save while its paste choice does not: say which was kept.
      const said = report(event.partial ? "appDelivery" : "appChoice");
      return s.rereadOnFailure ? withEffects(read(d, false), [said]) : { destination: d, effects: [said] };
    }
    case "destination-save-settled": return { destination: { ...d, saves: Math.max(0, d.saves - 1) }, effects: [] };
    case "project-edit": {
      // The sheet and the format menu share one mutation owner. Retire old work
      // before the native save, then keep new Enter waiting until its read settles.
      return { destination: { ...d, reads: d.reads + 1, output: { ...d.output, choice: d.output.choice + 1 }, editProblem: null, settlingProject: true,
        preserveOpeningOutput: false, held: null, ready: false, saves: d.saves + 1 }, effects: [], observed: { clearReplacement: true } };
    }
    case "project-edit-failed": {
      const current = event.opening === d.output.opening && event.choice === d.output.choice && typeof event.error === "string";
      return read(current ? { ...d, editProblem: event.error as string } : d, false);
    }
    case "consume-held":
      return sameOutput(event.output, d.output) ? { destination: { ...d, preserveOpeningOutput: false, held: null }, effects: [] } : null;
  }
}
