// The picker's saved formatting and view preferences.
import { useCallback, useEffect, useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { CopyFormat, Density, PickerPreferences } from "../../api";
import { PreferencePatchQueue } from "../../preferencePatchQueue";
import { styleOf, type Choice } from "./choice";
import type { Operation } from "./Notices";

const DENSITIES: Density[] = ["compact", "comfortable", "spacious"];

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
  const [ready, setReady] = useState(false);
  const [available, setAvailable] = useState(false);
  const [format, setFormat] = useState<Choice>("pandoc");
  const [density, setDensity] = useState<Density>("comfortable");
  const [densityFeedback, setDensityFeedback] = useState<{ presentation: number; density: Density } | null>(null);
  const [largerText, setLargerText] = useState(false);
  const [searchMark, setSearchMark] = useState(false);
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
    setDensity(value.density);
    setLargerText(value.larger_text);
    setSearchMark(value.search_mark ?? false);
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
      else report("preferences");
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
  const changeDensity = () => {
    const next = DENSITIES[(DENSITIES.indexOf(density) + 1) % DENSITIES.length] ?? "comfortable";
    setDensity(next);
    setDensityFeedback({ presentation, density: next });
    savePreferences({ density: next });
  };
  const toggleLargerText = () => {
    setLargerText(!largerText);
    savePreferences({ larger_text: !largerText });
  };
  const toggleSearchMark = () => {
    setSearchMark(!searchMark);
    savePreferences({ search_mark: !searchMark });
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

  return { ready, available, format, density, densityAnnouncement: densityFeedback?.presentation === presentation ? densityFeedback.density : undefined, largerText, searchMark, showPastePreview, chooseFormat, changeDensity, toggleLargerText, toggleSearchMark, togglePastePreview, quitAfterSaving, loadPreferences };
}
