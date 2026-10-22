// The picker's saved formatting and view preferences.
import { useEffect, useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { CopyFormat, Density, PickerPreferences } from "../../api";
import { styleOf, type Choice } from "./choice";
import type { Operation } from "./Notices";

const DENSITIES: Density[] = ["compact", "comfortable", "spacious"];
const SAVE_BEFORE_QUIT_MS = 2_000;

type PreferencesApi = Pick<typeof nativeApi, "copyFormat" | "pickerPreferences" | "setPickerPreferences" | "quit">;

export function usePickerPreferences(
  { copyFormat, pickerPreferences, setPickerPreferences, quit }: PreferencesApi,
  { styles, report, perform }: {
    /** Whether formatted citation styles can be offered. */
    styles: boolean;
    report: (operation: Operation, failed?: boolean) => void;
    perform: (operation: Operation, action: () => Promise<unknown>) => Promise<void>;
  },
) {
  const [format, setFormat] = useState<Choice>("pandoc");
  // Saved preferences, once read; choices made before that wait in `pending`
  // so a slow read never overwrites them and a failed read never resets the
  // file to defaults.
  const [density, setDensity] = useState<Density>("comfortable");
  const [largerText, setLargerText] = useState(false);
  const [showPastePreview, setShowPastePreview] = useState(true);
  const previewVisibility = useRef(true);
  const savedPreferences = useRef<PickerPreferences | null>(null);
  const pending = useRef<Partial<PickerPreferences>>({});
  const pendingKeyFormat = useRef<{ format: CopyFormat; saved: boolean } | null>(null);
  const preferenceSave = useRef<Promise<unknown>>(Promise.resolve());
  const preferenceLoad = useRef<Promise<void>>(Promise.resolve());
  const savePreferences = (change: Partial<PickerPreferences>, keyFormat: CopyFormat | null = null) => {
    if (keyFormat !== null) pendingKeyFormat.current = { format: keyFormat, saved: false };
    pending.current = { ...pending.current, ...change };
    if (!savedPreferences.current) return;
    const next = { ...savedPreferences.current, ...pending.current } as PickerPreferences;
    const chosenKey = pendingKeyFormat.current;
    pending.current = {};
    savedPreferences.current = next;
    // Preserve choice order and retry an unsaved key. Queued density/style
    // changes do not rewrite a key whose earlier save already succeeded.
    const save = preferenceSave.current.catch(() => undefined)
      .then(() => setPickerPreferences(next, chosenKey && !chosenKey.saved ? chosenKey.format : null))
      .then(() => {
        if (chosenKey) chosenKey.saved = true;
        if (pendingKeyFormat.current === chosenKey) pendingKeyFormat.current = null;
      });
    preferenceSave.current = save;
    void perform("preferences", () => save);
  };
  const quitAfterSaving = async () => {
    // Drain choices already made, including ones awaiting the initial read.
    // A failed or stuck settings command must never trap someone in the app.
    let timer: number | undefined;
    await Promise.race([
      preferenceLoad.current.then(() => preferenceSave.current).catch(() => undefined),
      new Promise<void>((resolve) => { timer = window.setTimeout(resolve, SAVE_BEFORE_QUIT_MS); }),
    ]);
    window.clearTimeout(timer);
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
  const loadPreferences = (mounted: () => boolean) => {
    // The saved style wins over the saved key format; a choice made while
    // these load wins over both.
    preferenceLoad.current = Promise.allSettled([copyFormat(), pickerPreferences()]).then(([key, saved]) => {
      if (!mounted()) return;
      if (key.status === "rejected") report("format");
      if (saved.status === "rejected") {
        // Without saved preferences the saved key format still applies.
        report("preferences");
        if (pending.current.citation_style === undefined && key.status === "fulfilled") setFormat(key.value);
        return;
      }
      const loaded = { ...saved.value, ...pending.current } as PickerPreferences;
      savedPreferences.current = saved.value;
      setDensity(loaded.density);
      setLargerText(loaded.larger_text);
      previewVisibility.current = loaded.show_paste_preview;
      setShowPastePreview(loaded.show_paste_preview);
      const chosen = pending.current.citation_style !== undefined;
      if (Object.keys(pending.current).length > 0) savePreferences({});
      if (chosen) return;
      const style = saved.value.citation_style;
      if (style && styles) setFormat(`style:${style}`);
      else if (key.status === "fulfilled") setFormat(key.value);
    });
  };
  return { format, density, largerText, showPastePreview, chooseFormat, changeDensity, toggleLargerText, togglePastePreview, quitAfterSaving, loadPreferences };
}
