import type { PickerPreferences } from "../bindings";

type Refusal = { row: string | null; message: "settings-read-only" | "settings-choice-failed" };
export type PreferenceState =
  | { status: "known"; value: PickerPreferences }
  | { status: "refused"; value: PickerPreferences; problem: Refusal }
  | { status: "unknown"; value: PickerPreferences | null; message: "settings-read-failed" | null };

type Action =
  | { type: "read"; value: PickerPreferences | null }
  | { type: "saved" | "change"; value: PickerPreferences }
  | { type: "invalidated" | "read-failed" }
  | { type: "refused"; saved: PickerPreferences | null; problem: Refusal };

export const initialPreferences: PreferenceState = { status: "unknown", value: null, message: null };

/** Choices and their explanation move together. Only a real read can recover
 * certainty after invalidation; an acknowledgement may still update the view. */
export function preferenceState(state: PreferenceState, action: Action): PreferenceState {
  switch (action.type) {
    case "invalidated":
      return { status: "unknown", value: state.value, message: state.status === "unknown" ? state.message : null };
    case "read-failed":
      return state.status === "refused" ? state : { status: "unknown", value: state.value, message: "settings-read-failed" };
    case "read":
      if (!action.value) return { status: "unknown", value: state.value, message: "settings-read-failed" };
      return state.status === "refused" ? { ...state, value: action.value } : { status: "known", value: action.value };
    case "saved":
    case "change":
      return state.status === "unknown" ? { ...state, value: action.value } : { status: "known", value: action.value };
    case "refused":
      return state.status === "unknown" || !action.saved
        ? { status: "unknown", value: state.value, message: "settings-read-failed" }
        : { status: "refused", value: action.saved, problem: action.problem };
  }
}
