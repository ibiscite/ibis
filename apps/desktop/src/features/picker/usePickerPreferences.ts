import type { SearchIcon } from "../../bindings";
// The picker's saved formatting and view preferences.
import { useCallback, useEffect, useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { CopyFormat, Density, PickerPreferences } from "../../api";
import { PreferencePatchQueue } from "../../preferencePatchQueue";
import { styleOf, type Choice } from "./choice";
import type { Operation } from "./Notices";

type PreferencesApi = Pick<typeof nativeApi, "copyFormat" | "pickerPreferences" | "setPickerPreferences" | "quit">;

export function usePickerPreferences(
  { copyFormat, pickerPreferences, setPickerPreferences, quit }: PreferencesApi,
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
  const [format, setFormat] = useState<Choice>("pandoc");
  const [density, setDensity] = useState<Density>("comfortable");
  const [densityFeedback, setDensityFeedback] = useState<{ presentation: number; density: Density } | null>(null);
  const [largerText, setLargerText] = useState(false);
  const [searchIcon, setSearchIcon] = useState<SearchIcon>("magnifier");
  const [showPastePreview, setShowPastePreview] = useState(true);
  const previewVisibility = useRef(true);
  const [queue] = useState(() => new PreferencePatchQueue({
    read: async () => {
      const [key, saved] = await Promise.allSettled([copyFormat(), pickerPreferences()]);
      return {
        preferences: saved.status === "fulfilled" ? saved.value : null,
        copy_format: key.status === "fulfilled" ? key.value : undefined,
        formatFailed: key.status === "rejected",
      };
    },
    write: setPickerPreferences,
  }, { readAfterWrites: true }));
  const applyView = useCallback((value: PickerPreferences) => {
    setSmallSurprises(value.small_surprises ?? true);
    setRecentCitations(value.recent_citations ?? true);
    setDensity(value.density);
    // A silent external view change retires its label so the next user choice can repeat it.
    setDensityFeedback((current) => current?.density === value.density ? current : null);
    setLargerText(value.larger_text);
    setSearchIcon(value.search_icon ?? "magnifier");
    previewVisibility.current = value.show_paste_preview;
    setShowPastePreview(value.show_paste_preview);
  }, []);
  useEffect(() => queue.subscribe((event) => {
    if (event.type === "saving") {
      void perform("preferences", () => event.promise);
    } else if (event.type === "saved") {
      setAvailable(true);
      applyView(event.value.preferences);
      setFormat(event.value.preferences.citation_style ? `style:${event.value.preferences.citation_style}` : event.value.copy_format);
    } else if (event.type === "read") {
      const { preferences: saved, copy_format: key, formatFailed } = event.value;
      setReady(true);
      if (formatFailed) report("format");
      setAvailable(saved !== null);
      if (saved) applyView(saved);
      else {
        // Unknown again: record nothing, forget nothing.
        setRecentCitations(null);
        report("preferences");
      }
      if (event.pendingStyle) return;
      if (saved?.citation_style && styles) setFormat(`style:${saved.citation_style}`);
      else if (key) setFormat(key);
    }
  }), [queue, styles, report, perform, applyView]);
  const savePreferences = (change: Partial<PickerPreferences>, keyFormat: CopyFormat | null = null) => queue.change(change, keyFormat);
  const quitAfterSaving = async () => {
    await queue.finishSaving();
    await quit();
  };
  const chooseFormat = (choice: Choice) => {
    setFormat(choice);
    const chosenStyle = styleOf(choice);
    savePreferences({ citation_style: chosenStyle }, chosenStyle ? null : choice as CopyFormat);
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
  const togglePastePreview = () => {
    const next = !previewVisibility.current;
    previewVisibility.current = next;
    setShowPastePreview(next);
    savePreferences({ show_paste_preview: next });
  };
  useEffect(() => {
    document.documentElement.dataset.density = density;
    document.documentElement.dataset.textSize = largerText ? "larger" : "normal";
    return () => {
      delete document.documentElement.dataset.density;
      delete document.documentElement.dataset.textSize;
    };
  }, [density, largerText]);
  /** Reads the saved key format and preferences. The picker calls this from its
   * mount effect, after the library status, so the calls keep their order. */
  const loadPreferences = (mounted: () => boolean) => { void queue.load(mounted); };

  return { ready, available, smallSurprises, recentCitations, format, density, densityAnnouncement: densityFeedback?.presentation === presentation ? densityFeedback.density : undefined, largerText, searchIcon, showPastePreview, chooseFormat, changeDensity, toggleLargerText, togglePastePreview, quitAfterSaving, loadPreferences };
}
