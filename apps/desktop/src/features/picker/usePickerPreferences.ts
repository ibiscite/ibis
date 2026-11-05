// The picker's saved formatting and view preferences.
import { useCallback, useEffect, useState } from "react";
import type * as nativeApi from "../../api";
import type { Density, PickerPreferences } from "../../api";
import { defaultOutput, isStyle, listedOutput, type OutputId } from "../../format/outputs";
import { PreferencePatchQueue } from "../../preferencePatchQueue";
import type { Operation } from "./Notices";

type PreferencesApi = Pick<typeof nativeApi, "pickerPreferences" | "setPickerPreferences" | "quit">;

export function usePickerPreferences(
  { pickerPreferences, setPickerPreferences, quit }: PreferencesApi,
  { styles, report, perform, presentation }: {
    /** Immutable picker visibility identity; old feedback never enters a new presentation. */
    presentation: number;
    /** Whether formatted citation styles can be offered. */
    styles: boolean;
    report: (operation: Operation, failed?: boolean) => void;
    perform: (operation: Operation, action: () => Promise<unknown>) => Promise<void>;
  },
) {
  const [smallSurprises, setSmallSurprises] = useState<boolean | null>(null);
  /** Null until read: nothing is recorded or forgotten on a guess. */
  const [recentCitations, setRecentCitations] = useState<boolean | null>(null);
  const [ready, setReady] = useState(false);
  const [available, setAvailable] = useState(false);
  const [format, setFormat] = useState<OutputId>(defaultOutput);
  const [density, setDensity] = useState<Density>("comfortable");
  const [densityFeedback, setDensityFeedback] = useState<{ presentation: number; density: Density } | null>(null);
  const [largerText, setLargerText] = useState(false);
  const [queue] = useState(() => new PreferencePatchQueue({
    read: async () => ({ preferences: await pickerPreferences().catch(() => null) }),
    write: setPickerPreferences,
  }, { readAfterWrites: true }));
  const applyView = useCallback((value: PickerPreferences) => {
    setSmallSurprises(value.small_surprises ?? true);
    setRecentCitations(value.recent_citations ?? true);
    setDensity(value.density);
    // A silent external view change retires its label so the next user choice can repeat it.
    setDensityFeedback((current) => current?.density === value.density ? current : null);
    setLargerText(value.larger_text);
  }, []);
  useEffect(() => queue.subscribe((event) => {
    if (event.type === "saving") {
      void perform("preferences", () => event.promise);
    } else if (event.type === "saved") {
      setAvailable(true);
      applyView(event.value.preferences);
      setFormat(listedOutput(event.value.preferences.output) ?? defaultOutput);
    } else if (event.type === "read") {
      const { preferences: saved } = event.value;
      setReady(true);
      setAvailable(saved !== null);
      if (saved) applyView(saved);
      else {
        // Unknown again: record nothing, forget nothing.
        setRecentCitations(null);
        report("preferences");
      }
      if (event.pendingOutput) return;
      // A style waits for the formatter; until then the default is what copies.
      const savedOutput = listedOutput(saved?.output);
      if (savedOutput && (styles || !isStyle(savedOutput))) setFormat(savedOutput);
    }
  }), [queue, styles, report, perform, applyView]);
  const savePreferences = (change: Partial<PickerPreferences>) => queue.change(change);
  const quitAfterSaving = async () => {
    await queue.finishSaving();
    await quit();
  };
  const chooseFormat = (output: OutputId) => {
    setFormat(output);
    savePreferences({ output });
  };
  const changeDensity = (next: Density) => {
    if (next === density) return;
    setDensity(next);
    setDensityFeedback({ presentation, density: next });
    savePreferences({ density: next });
  };
  const toggleLargerText = () => {
    setLargerText(!largerText);
    savePreferences({ larger_text: !largerText });
  };
  useEffect(() => {
    document.documentElement.dataset.density = density;
    document.documentElement.dataset.textSize = largerText ? "larger" : "normal";
    return () => {
      delete document.documentElement.dataset.density;
      delete document.documentElement.dataset.textSize;
    };
  }, [density, largerText]);
  /** Reads the saved preferences. The picker calls this from its
   * mount effect, after the library status, so the calls keep their order. */
  const loadPreferences = (mounted: () => boolean) => { void queue.load(mounted); };

  return { ready, available, smallSurprises, recentCitations, format, density, densityAnnouncement: densityFeedback?.presentation === presentation ? densityFeedback.density : undefined, largerText, chooseFormat, changeDensity, toggleLargerText, quitAfterSaving, loadPreferences };
}
